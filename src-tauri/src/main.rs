// 防止 release 模式下出现控制台窗口（Windows）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod ai;
mod embedding;
mod error;
mod file_storage;
pub mod lan;
mod protocol;
mod state;
mod thumbnail;

/// 在 main() 最早期设置 ONNX Runtime DLL 路径
/// build.rs 通过 cargo:rustc-env 编译期注入路径，运行期设置环境变量供 ort load-dynamic 读取
fn setup_ort_dylib_path() {
    if let Some(path) = option_env!("ORT_DYLIB_PATH") {
        if !path.is_empty() {
            std::env::set_var("ORT_DYLIB_PATH", path);
        }
    }
}

use state::AppState;
use tauri::Manager;

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

/// 后台批量生成缺失 embedding 的历史 memo
/// 在应用启动后异步执行，不阻塞 UI；首次会触发模型下载
fn backfill_embeddings(app: &tauri::AppHandle) {
    use rusqlite::params;
    let state = app.state::<AppState>();

    // 查询没有 embedding 的 NORMAL 状态 memo
    let ids: Vec<i32> = match state.store().with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT m.id FROM memo m
             LEFT JOIN memo_vec v ON m.id = v.rowid
             WHERE v.rowid IS NULL AND m.row_status = 'NORMAL'",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }) {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!("查询缺失 embedding 的 memo 失败: {}", e);
            return;
        }
    };

    if ids.is_empty() {
        return;
    }
    tracing::info!("开始为 {} 条历史 memo 生成 embedding", ids.len());

    for id in ids {
        let content: String = match state.store().with_conn(|c| {
            let content: String = c.query_row("SELECT content FROM memo WHERE id = ?", [id], |r| r.get(0))?;
            Ok(content)
        }) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("查询 memo {} 内容失败: {}", id, e);
                continue;
            }
        };
        match crate::embedding::embed_to_json(&content) {
            Ok(embedding_json) => {
                if let Err(e) = state.store().with_conn(|c| {
                    c.execute(
                        "INSERT INTO memo_vec(rowid, embedding) VALUES (?, ?)",
                        params![id, &embedding_json],
                    )?;
                    Ok(())
                }) {
                    tracing::warn!("为 memo {} 插入 embedding 失败: {}", id, e);
                }
            }
            Err(e) => tracing::warn!("为 memo {} 生成 embedding 失败: {}", id, e),
        }
    }
    tracing::info!("历史 embedding 生成完成");
}

fn main() {
    setup_ort_dylib_path();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();

    tauri::Builder::default()
        .register_uri_scheme_protocol("attachment", |ctx, request| {
            let state = ctx.app_handle().state::<AppState>();
            protocol::handle_attachment_request(state.inner(), &request)
        })
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("无法获取数据目录");
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
                lan: None,
            });

            // 后台懒加载历史 memo 的 embedding（不阻塞 UI）
            // 首次启动会触发模型下载，后续仅为缺失 embedding 的 memo 生成
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                backfill_embeddings(&app_handle);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            // memo
            commands::memo::create_memo,
            commands::memo::get_memo,
            commands::memo::list_memos,
            commands::memo::update_memo,
            commands::memo::delete_memo,
            commands::memo::render_memo_content,
            commands::memo::list_tags,
            commands::memo::list_memo_timestamps,
            commands::memo::embed_text,
            // attachment
            commands::attachment::create_attachment,
            commands::attachment::get_attachment,
            commands::attachment::list_attachments,
            commands::attachment::update_attachment,
            commands::attachment::delete_attachment,
            commands::attachment::get_attachment_thumbnail,
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
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用时出错");
}
