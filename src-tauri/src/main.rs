// release 构建隐藏 Windows 控制台窗口；debug 构建保留控制台便于调试退出流程。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod ai;
mod embedding;
mod error;
mod file_storage;
pub mod lan;
mod llm_runner;
mod mcp;
mod protocol;
mod state;
mod thumbnail;

/// 在 main() 最早期设置 ONNX Runtime DLL 路径
/// build.rs 通过 cargo:rustc-env 编译期注入路径，运行期设置环境变量供 ort load-dynamic 读取
fn setup_ort_dylib_path() {
    if let Some(path) = option_env!("ORT_DYLIB_PATH") {
        if !path.is_empty() {
            // Rust 2024 edition 中 set_var 标记为 unsafe（多线程环境下可能引发 UB）
            unsafe { std::env::set_var("ORT_DYLIB_PATH", path); }
        }
    }
}

use state::AppState;
use std::sync::atomic::Ordering;
use tauri::Manager;

/// 退出时给清理逻辑一个有限窗口，避免卡死在后台任务收尾。
const EXIT_CLEANUP_TIMEOUT_SECS: u64 = 2;
/// 超过该时间仍未正常退出，则直接结束进程，避免残留后台进程。
const EXIT_FORCE_TIMEOUT_SECS: u64 = 5;

fn current_pid() -> u32 {
    std::process::id()
}

fn init_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .init();
}

fn stop_lan_with_timeout(app_handle: &tauri::AppHandle) {
    tracing::info!(
        pid = current_pid(),
        timeout_secs = EXIT_CLEANUP_TIMEOUT_SECS,
        "退出清理：开始停止 LAN 模块"
    );

    match tauri::async_runtime::block_on(async {
        tokio::time::timeout(
            std::time::Duration::from_secs(EXIT_CLEANUP_TIMEOUT_SECS),
            lan::endpoint::stop_lan_module(app_handle),
        )
        .await
    }) {
        Ok(Ok(())) => tracing::info!(pid = current_pid(), "退出清理：LAN 模块已停止"),
        Ok(Err(e)) => tracing::warn!(pid = current_pid(), "退出清理：LAN 模块停止失败，继续退出: {}", e),
        Err(_) => tracing::warn!(pid = current_pid(), "退出清理：LAN 模块停止超时，继续退出"),
    }
}

fn cleanup_app_resources(app_handle: &tauri::AppHandle) {
    let state = app_handle.state::<AppState>();
    let shutdown_was_set = state
        .shutdown
        .swap(true, Ordering::SeqCst);
    tracing::info!(pid = current_pid(), shutdown_was_set, "退出清理：开始");

    commands::ai_chat::abort_all();
    stop_lan_with_timeout(app_handle);
    stop_llm_runner(app_handle);
    stop_mcp_with_timeout(app_handle);
    tracing::info!(pid = current_pid(), "退出清理：完成");
}

/// 退出时停止本地 MCP 服务器
fn stop_mcp_with_timeout(app_handle: &tauri::AppHandle) {
    tracing::info!(
        pid = current_pid(),
        timeout_secs = EXIT_CLEANUP_TIMEOUT_SECS,
        "退出清理：开始停止 MCP 模块"
    );

    match tauri::async_runtime::block_on(async {
        tokio::time::timeout(
            std::time::Duration::from_secs(EXIT_CLEANUP_TIMEOUT_SECS),
            mcp::stop_mcp_module(app_handle),
        )
        .await
    }) {
        Ok(Ok(())) => tracing::info!(pid = current_pid(), "退出清理：MCP 模块已停止"),
        Ok(Err(e)) => tracing::warn!(pid = current_pid(), "退出清理：MCP 模块停止失败，继续退出: {}", e),
        Err(_) => tracing::warn!(pid = current_pid(), "退出清理：MCP 模块停止超时，继续退出"),
    }
}

/// 退出时停止本地 LLM 服务（前台模式 kill 子进程；守护模式调用 lms server stop）
fn stop_llm_runner(app_handle: &tauri::AppHandle) {
    let state = app_handle.state::<AppState>();
    let runner = state
        .llm
        .read()
        .expect("LLM RwLock poisoned")
        .clone();
    let Some(runner) = runner else {
        tracing::info!(pid = current_pid(), "退出清理：LLM 启动器未初始化，跳过");
        return;
    };
    if !runner.is_running() {
        tracing::info!(pid = current_pid(), "退出清理：LLM 启动器未运行，跳过");
        return;
    }
    tracing::info!(pid = current_pid(), "退出清理：停止本地 LLM 服务");
    let app_clone = app_handle.clone();
    match tauri::async_runtime::block_on(async {
        tauri::async_runtime::spawn_blocking(move || {
            llm_runner::runner::stop_runner(runner, app_clone)
        })
        .await
    }) {
        Ok(Ok(())) => tracing::info!(pid = current_pid(), "退出清理：LLM 服务已停止"),
        Ok(Err(e)) => tracing::warn!(pid = current_pid(), "退出清理：LLM 服务停止失败，继续退出: {}", e),
        Err(e) => tracing::warn!(pid = current_pid(), "退出清理：LLM 服务停止任务 join 失败: {}", e),
    }
}

fn spawn_exit_watchdog() {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(EXIT_FORCE_TIMEOUT_SECS));
        tracing::warn!(
            pid = current_pid(),
            timeout_secs = EXIT_FORCE_TIMEOUT_SECS,
            "退出看门狗：正常退出超时，执行强制退出"
        );
        std::process::exit(0);
    });
}

fn spawn_cleanup_and_exit(app_handle: tauri::AppHandle) {
    std::thread::spawn(move || {
        cleanup_app_resources(&app_handle);
        tracing::info!(pid = current_pid(), "退出清理：请求应用退出");
        app_handle.exit(0);
    });
}

/// 健康检查命令 — 验证 Store 已初始化
#[tauri::command]
fn ping(state: tauri::State<'_, AppState>) -> String {
    let store = state.store();
    match store.with_conn(|c| {
        let count: i32 = c.query_row("SELECT count(*) FROM memo", [], |row| row.get(0))?;
        Ok(count)
    }) {
        Ok(count) => format!("Store 就绪，当前 memo 数: {}", count),
        Err(e) => format!("Store 错误: {}", e),
    }
}

fn main() {
    setup_ort_dylib_path();
    init_tracing();

    tracing::info!(pid = current_pid(), "应用启动，控制台日志已启用");

    tauri::Builder::default()
        .register_uri_scheme_protocol("attachment", |ctx, request| {
            let state = ctx.app_handle().state::<AppState>();
            protocol::handle_attachment_request(state.inner(), &request)
        })
        .setup(|app| {
            tracing::info!(pid = current_pid(), "setup: begin");
            // 应用数据统一存储在用户目录下的 localFragNote 文件夹
            #[allow(deprecated)]
            let data_dir = dirs::home_dir()
                .expect("无法获取用户目录")
                .join("localFragNote");
            std::fs::create_dir_all(&data_dir).expect("无法创建数据目录");
            let db_path = data_dir.join("memos.db");
            tracing::info!("数据库路径: {}", db_path.display());

            let store = memos_core::Store::open(&db_path).expect("无法打开 Store");

            // 从配置读取附件目录：绝对路径直接使用，相对路径基于 data_dir
            let storage_config = commands::setting::load_storage_config(&store);
            let attachments_dir = if std::path::Path::new(&storage_config.local_storage_path).is_absolute() {
                std::path::PathBuf::from(&storage_config.local_storage_path)
            } else {
                data_dir.join(&storage_config.local_storage_path)
            };
            std::fs::create_dir_all(&attachments_dir).expect("无法创建附件目录");
            tracing::info!("附件目录: {}（模板: {}）", attachments_dir.display(), storage_config.filepath_template);

            app.manage(AppState {
                store: std::sync::Mutex::new(store),
                attachments_dir,
                lan: std::sync::RwLock::new(None),
                llm: std::sync::RwLock::new(None),
                mcp: std::sync::RwLock::new(None),
                shutdown: std::sync::atomic::AtomicBool::new(false),
                cleanup_started: std::sync::atomic::AtomicBool::new(false),
            });

            // 根据持久化设置决定是否在启动时拉起 LAN 模块
            let lan_enabled = {
                let state = app.state::<AppState>();
                let store = state.store();
                lan::endpoint::load_enabled(&store)
            };
            if lan_enabled {
                let app_handle = app.handle().clone();
                tracing::info!("setup: 检测到 LAN 已启用，开始启动 LAN 模块");
                let result = tauri::async_runtime::block_on(async {
                    lan::endpoint::start_lan_module(&app_handle).await
                });
                match result {
                    Ok(_) => tracing::info!("LAN 模块启动成功"),
                    Err(e) => tracing::warn!("LAN 模块启动失败（应用其他功能不受影响）: {}", e),
                }
            }

            // 根据持久化配置决定是否在启动时拉起本地 LLM 服务
            let llm_auto_start = {
                let state = app.state::<AppState>();
                let store = state.store();
                llm_runner::load_config(&store).auto_start
            };
            if llm_auto_start {
                let app_handle = app.handle().clone();
                tracing::info!("setup: 检测到 LLM 启动器配置 auto_start=true，开始启动本地 LLM 服务");
                tauri::async_runtime::spawn(async move {
                    let runner = match commands::llm_runner::llm_start(app_handle.clone()).await {
                        Ok(r) => r,
                        Err(e) => {
                            tracing::warn!("LLM 服务启动失败（应用其他功能不受影响）: {}", e);
                            return;
                        }
                    };
                    tracing::info!(
                        pid = current_pid(),
                        running = runner.running,
                        "LLM 服务启动流程结束"
                    );
                });
            }

            // 根据持久化配置决定是否在启动时拉起 MCP 服务器
            let mcp_auto_start = {
                let state = app.state::<AppState>();
                let store = state.store();
                mcp::load_config(&store).auto_start
            };
            if mcp_auto_start {
                let app_handle = app.handle().clone();
                tracing::info!("setup: 检测到 MCP 配置 auto_start=true，开始启动 MCP 服务器");
                tauri::async_runtime::spawn(async move {
                    match commands::mcp::mcp_start(app_handle.clone()).await {
                        Ok(status) => {
                            tracing::info!(
                                pid = current_pid(),
                                running = status.running,
                                endpoint = %status.endpoint_url,
                                "MCP 服务器启动流程结束"
                            );
                        }
                        Err(e) => {
                            tracing::warn!("MCP 服务器启动失败（应用其他功能不受影响）: {}", e);
                        }
                    }
                });
            }

            tracing::info!(pid = current_pid(), "setup: end");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            // memo
            commands::memo::create_memo,
            commands::memo::get_memo,
            commands::memo::list_memos,
            commands::memo::list_memo_comments,
            commands::memo::count_memo_comments_batch,
            commands::memo::update_memo,
            commands::memo::delete_memo,
            commands::memo::render_memo_content,
            commands::memo::list_tags,
            commands::memo::rebuild_tag_table,
            commands::memo::list_memo_timestamps,
            commands::memo::embed_text,
            commands::memo::suggest_tags,
            // attachment
            commands::attachment::create_attachment,
            commands::attachment::get_attachment,
            commands::attachment::list_attachments,
            commands::attachment::update_attachment,
            commands::attachment::delete_attachment,
            commands::attachment::get_attachment_thumbnail,
            commands::document_summary::summarize_document_content,
            // reaction
            commands::reaction::upsert_reaction,
            commands::reaction::list_reactions,
            commands::reaction::delete_reaction,
            // memo_relation
            commands::memo_relation::upsert_memo_relation,
            commands::memo_relation::list_memo_relations,
            commands::memo_relation::delete_memo_relation,
            // setting
            commands::setting::get_app_setting,
            commands::setting::upsert_app_setting,
            commands::setting::delete_app_setting,
            commands::setting::get_instance_setting,
            commands::setting::upsert_instance_setting,
            commands::setting::delete_instance_setting,
            commands::setting::get_instance_stats,
            commands::setting::get_storage_config,
            commands::setting::update_storage_config,
            // ai chat
            commands::ai_chat::ai_chat,
            commands::ai_chat::ai_abort,
            commands::ai_chat::list_providers,
            commands::ai_chat::save_providers_cmd,
            commands::chat_session::chat_list_sessions,
            commands::chat_session::chat_create_session,
            commands::chat_session::chat_rename_session,
            commands::chat_session::chat_delete_session,
            commands::chat_session::chat_list_messages,
            commands::chat_session::chat_append_message,
            commands::chat_session::chat_clear_messages,
            // lan discovery
            commands::lan::lan_discover_peers,
            commands::lan::lan_get_status,
            commands::lan::lan_set_enabled,
            commands::lan::lan_get_local_identity,
            commands::lan::lan_update_display_name,
            commands::lan::lan_get_acl_rules,
            commands::lan::lan_save_acl_rules,
            commands::lan::lan_get_remote_profile,
            commands::lan::lan_list_remote_memos,
            commands::lan::lan_get_remote_memo,
            commands::lan::lan_get_remote_attachment,
            commands::lan::lan_copy_memo_to_local,
            // review
            commands::review::review_list_decks,
            commands::review::review_create_deck,
            commands::review::review_update_deck,
            commands::review::review_delete_deck,
            commands::review::review_list_cards,
            commands::review::review_list_due_cards,
            commands::review::review_delete_card,
            commands::review::review_score_card,
            commands::review::review_generate_cards,
            commands::review::review_regenerate_card,
            commands::review::review_deck_stats,
            commands::review::review_total_due_count,
            commands::review::review_list_review_timestamps,
            commands::review::review_check_new_memos,
            // import/export
            commands::import_export::export_memos_json,
            commands::import_export::import_memos_json,
            // local llm runner
            commands::llm_runner::llm_get_config,
            commands::llm_runner::llm_update_config,
            commands::llm_runner::llm_start,
            commands::llm_runner::llm_stop,
            commands::llm_runner::llm_get_status,
            commands::llm_runner::llm_test_connection,
            // local mcp server
            commands::mcp::mcp_get_config,
            commands::mcp::mcp_update_config,
            commands::mcp::mcp_start,
            commands::mcp::mcp_stop,
            commands::mcp::mcp_get_status,
            commands::mcp::mcp_test_connection,
        ])
        .build(tauri::generate_context!())
        .expect("构建 Tauri 应用时出错")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let state = app_handle.state::<AppState>();
                if !state.begin_shutdown() {
                    tracing::debug!(pid = current_pid(), "退出流程已启动，忽略重复 ExitRequested");
                    return;
                }

                api.prevent_exit();
                tracing::info!(pid = current_pid(), "收到退出请求，开始执行退出清理");
                spawn_exit_watchdog();
                spawn_cleanup_and_exit(app_handle.clone());
            }
        });
}
