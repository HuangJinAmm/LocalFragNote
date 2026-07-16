//! Embedding 服务：直接基于 ort (ONNX Runtime) + tokenizers 本地生成文本向量
//!
//! 模型：all-MiniLM-L6-v2（384维），ONNX Runtime，离线运行
//! 首次使用时从 HuggingFace 下载模型（约 90MB），缓存到用户目录

use crate::error::{IpcError, IpcResult};
use ndarray::Array2;
use ort::session::{builder::GraphOptimizationLevel, Session};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tokenizers::tokenizer::Tokenizer;
use tokenizers::utils::padding::{PaddingDirection, PaddingParams, PaddingStrategy};
use tokenizers::utils::truncation::{TruncationDirection, TruncationParams, TruncationStrategy};

const MAX_SEQ_LEN: usize = 256;
const HIDDEN: usize = 384;
const MODEL_BASE_URL: &str = "https://www.modelscope.cn/models/sentence-transformers/all-MiniLM-L6-v2/resolve/master";

/// 模型文件路径
struct ModelPaths {
    model: PathBuf,
    tokenizer: PathBuf,
}

/// 已初始化的模型实例（Session + Tokenizer）
struct Embedder {
    session: Session,
    tokenizer: Tokenizer,
}

/// 全局模型槽位：
/// - `None`：尚未尝试初始化
/// - `Some(Ok(embedder))`：已就绪
/// - `Some(Err(err))`：初始化失败，错误被缓存（重启应用才会重试）
///
/// 用 `OnceLock<Mutex<Option<...>>>` 而非 `OnceLock::get_or_try_init`，
/// 因为后者在 stable Rust 上仍是 unstable feature（`once_cell_try`）。
static EMBEDDER_SLOT: OnceLock<Mutex<Option<Result<Embedder, String>>>> = OnceLock::new();

/// 获取模型槽位的全局 Mutex 引用（首次调用惰性创建）
fn embedder_slot() -> &'static Mutex<Option<Result<Embedder, String>>> {
    EMBEDDER_SLOT.get_or_init(|| Mutex::new(None))
}

/// 获取模型缓存目录（用户目录下 localFragNote/models/all-MiniLM-L6-v2）
fn model_dir() -> IpcResult<PathBuf> {
    #[allow(deprecated)]
    let dir = dirs::home_dir()
        .ok_or_else(|| IpcError::Internal("无法获取用户目录".into()))?
        .join("localFragNote")
        .join("models")
        .join("all-MiniLM-L6-v2");
    std::fs::create_dir_all(&dir)
        .map_err(|e| IpcError::Internal(format!("创建模型目录失败: {e}")))?;
    Ok(dir)
}

/// 获取模型文件路径
fn model_paths() -> IpcResult<ModelPaths> {
    let dir = model_dir()?;
    Ok(ModelPaths {
        model: dir.join("model.onnx"),
        tokenizer: dir.join("tokenizer.json"),
    })
}

/// 下载单个文件（支持 HuggingFace resolve URL 重定向）
/// 先写入 `.part` 临时文件，下载成功后再原子重命名，避免中断后留下损坏的半成品
fn download_file(url: &str, dest: &std::path::Path) -> IpcResult<()> {
    if dest.exists() {
        tracing::debug!("模型文件已存在，跳过下载: {}", dest.display());
        return Ok(());
    }
    tracing::info!("下载模型文件: {} -> {}", url, dest.display());
    let dl_start = std::time::Instant::now();

    let tmp = dest.with_extension("onnx.part");
    // 清理上次中断遗留的 .part 文件
    let _ = std::fs::remove_file(&tmp);

    let response = ureq::get(url)
        .call()
        .map_err(|e| IpcError::Internal(format!("下载失败 {url}: {e}")))?;
    let content_length = response.header("Content-Length").and_then(|s| s.parse::<u64>().ok());
    tracing::info!(
        "模型下载响应就绪: status={}, 预期大小={}",
        response.status(),
        content_length.map(|b| format!("{} bytes", b)).unwrap_or_else(|| "未知".into())
    );

    let bytes_written = {
        let mut file = std::fs::File::create(&tmp)
            .map_err(|e| IpcError::Internal(format!("创建临时文件失败 {}: {e}", tmp.display())))?;
        let n = std::io::copy(&mut response.into_reader(), &mut file)
            .map_err(|e| {
                // 下载失败时清理 .part 文件，避免下次误判为有效
                let _ = std::fs::remove_file(&tmp);
                IpcError::Internal(format!("写入文件失败: {e}"))
            })?;
        // flush + sync 确保数据落盘后再重命名
        let _ = file.sync_all();
        n
    };

    std::fs::rename(&tmp, dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        IpcError::Internal(format!(
            "重命名 {} -> {} 失败: {e}",
            tmp.display(),
            dest.display()
        ))
    })?;

    tracing::info!(
        "模型文件下载完成: {} ({} bytes), 耗时 {:?}",
        dest.display(),
        bytes_written,
        dl_start.elapsed()
    );
    Ok(())
}

/// 确保模型文件已下载
fn ensure_model_files() -> IpcResult<ModelPaths> {
    let paths = model_paths()?;
    download_file(&format!("{MODEL_BASE_URL}/onnx/model.onnx"), &paths.model)?;
    download_file(&format!("{MODEL_BASE_URL}/tokenizer.json"), &paths.tokenizer)?;
    Ok(paths)
}

/// Mean pooling：用 attention_mask 加权平均 token embeddings
fn mean_pool(token_embeddings: &[f32], attention_mask: &[i64], hidden: usize) -> Vec<f32> {
    let mut summed = vec![0f32; hidden];
    let mut count = 0f32;
    for (i, &m) in attention_mask.iter().enumerate() {
        if m == 0 {
            continue;
        }
        let mf = m as f32;
        for h in 0..hidden {
            summed[h] += token_embeddings[i * hidden + h] * mf;
        }
        count += mf;
    }
    let denom = count.max(1e-9);
    summed.iter().map(|v| v / denom).collect()
}

/// L2 归一化（sentence-transformers 默认后处理）
fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-12 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// 初始化 Embedder（下载模型 + 加载 ONNX session + 配置 tokenizer）
fn build_embedder() -> Result<Embedder, String> {
    let init_start = std::time::Instant::now();
    tracing::info!("开始初始化 embedding 模型 (all-MiniLM-L6-v2)");

    let paths = ensure_model_files().map_err(|e| {
        tracing::warn!("embedding 模型文件准备失败: {}", e);
        e.to_string()
    })?;
    tracing::debug!(
        "模型文件路径: model={}, tokenizer={}",
        paths.model.display(),
        paths.tokenizer.display()
    );

    let session_start = std::time::Instant::now();
    let session = Session::builder()
        .map_err(|e| format!("创建 SessionBuilder 失败: {e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level1)
        .map_err(|e| format!("设置优化级别失败: {e}"))?
        .with_intra_threads(1)
        .map_err(|e| format!("设置线程数失败: {e}"))?
        .commit_from_file(&paths.model)
        .map_err(|e| {
            tracing::warn!("加载 ONNX 模型失败: {}", e);
            format!("加载 ONNX 模型失败: {e}（可能是下载不完整，删除 {} 后重启）", paths.model.display())
        })?;
    tracing::info!("ONNX session 加载完成，耗时 {:?}", session_start.elapsed());

    let tok_start = std::time::Instant::now();
    let mut tokenizer = Tokenizer::from_file(&paths.tokenizer)
        .map_err(|e| format!("加载 tokenizer.json 失败: {e}"))?;

    let mut trunc = TruncationParams::default();
    trunc.max_length = MAX_SEQ_LEN;
    trunc.direction = TruncationDirection::Right;
    trunc.strategy = TruncationStrategy::LongestFirst;
    tokenizer
        .with_truncation(Some(trunc))
        .map_err(|e| format!("设置 truncation 失败: {e}"))?;

    let mut pad = PaddingParams::default();
    pad.strategy = PaddingStrategy::BatchLongest;
    pad.direction = PaddingDirection::Right;
    pad.pad_id = 0;
    pad.pad_type_id = 0;
    pad.pad_token = "[PAD]".to_string();
    tokenizer.with_padding(Some(pad));
    tracing::debug!("tokenizer 配置完成，耗时 {:?}", tok_start.elapsed());

    tracing::info!("embedding 模型初始化完成，总耗时 {:?}", init_start.elapsed());
    Ok(Embedder { session, tokenizer })
}

/// 生成单条文本的 embedding（384维 f32）
///
/// 持有全局槽位锁期间完成"按需初始化 + tokenize + ONNX 推理"全流程。
/// 初始化失败时返回错误而非 panic；失败状态被缓存，避免每次调用都重试下载。
/// 重启应用后才会重新尝试初始化。
///
/// 注意：模型下载/初始化可能耗时数十秒，此处持锁会阻塞并发的 embedding 请求。
/// 这是可接受的——本地单用户场景下并发请求极少，且保证正确性优先于吞吐。
pub fn embed(text: &str) -> IpcResult<Vec<f32>> {
    let total_start = std::time::Instant::now();
    let text_len = text.chars().count();
    let text_bytes = text.len();
    tracing::debug!(
        "embed 开始: chars={}, bytes={}",
        text_len,
        text_bytes
    );

    let mutex = embedder_slot();
    let lock_start = std::time::Instant::now();
    let mut guard = mutex
        .lock()
        .map_err(|e| IpcError::Internal(format!("embedding 槽位 Mutex poisoned: {e}")))?;
    let lock_wait = lock_start.elapsed();
    if lock_wait.as_millis() > 50 {
        tracing::info!("embed 获取槽位锁等待 {:?}（可能有并发 embedding 请求排队）", lock_wait);
    } else {
        tracing::debug!("embed 获取槽位锁耗时 {:?}", lock_wait);
    }

    // 按需初始化（仅首次调用执行；失败结果同样缓存，避免反复重试下载）
    let was_uninit = guard.is_none();
    if was_uninit {
        tracing::info!("embed 首次调用，触发模型初始化");
        *guard = Some(build_embedder());
    }

    // 取出 Result 引用：失败则返回缓存错误，成功则执行推理
    // 需要 &mut Embedder，因为 ort 2.0.0-rc.12 的 Session::run 要求 &mut self
    let embedder = match guard.as_mut().unwrap() {
        Ok(e) => e,
        Err(err) => {
            tracing::warn!("embed 返回缓存错误（模型初始化曾失败）: {}", err);
            return Err(IpcError::Internal(format!(
                "embedding 模型初始化失败（重启应用才会重试）: {err}"
            )));
        }
    };

    // 1. tokenize
    let tok_start = std::time::Instant::now();
    let encoding = embedder
        .tokenizer
        .encode(text, true)
        .map_err(|e| {
            tracing::warn!("embed tokenize 失败: {}", e);
            IpcError::Internal(format!("tokenize 失败: {e}"))
        })?;

    let seq_len = encoding.get_ids().len();
    let input_ids: Vec<i64> = encoding.get_ids().iter().map(|v| *v as i64).collect();
    let attention_mask: Vec<i64> = encoding.get_attention_mask().iter().map(|v| *v as i64).collect();
    let token_type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|v| *v as i64).collect();
    tracing::debug!(
        "embed tokenize 完成: seq_len={}, 耗时 {:?}",
        seq_len,
        tok_start.elapsed()
    );

    // 2. 构造输入张量 [1, seq_len]
    //    ort 2.0.0-rc.12 起，ndarray 需先用 Tensor::from_array 包装为 ort::value::Tensor
    use ort::value::Tensor;
    let input_ids_tensor = Tensor::from_array(
        Array2::from_shape_vec((1, seq_len), input_ids)
            .map_err(|e| IpcError::Internal(format!("输入张量构造失败: {e}")))?,
    )
    .map_err(|e| IpcError::Internal(format!("创建 input_ids 张量失败: {e}")))?;
    let attention_mask_tensor = Tensor::from_array(
        Array2::from_shape_vec((1, seq_len), attention_mask.clone())
            .map_err(|e| IpcError::Internal(format!("输入张量构造失败: {e}")))?,
    )
    .map_err(|e| IpcError::Internal(format!("创建 attention_mask 张量失败: {e}")))?;
    let token_type_ids_tensor = Tensor::from_array(
        Array2::from_shape_vec((1, seq_len), token_type_ids)
            .map_err(|e| IpcError::Internal(format!("输入张量构造失败: {e}")))?,
    )
    .map_err(|e| IpcError::Internal(format!("创建 token_type_ids 张量失败: {e}")))?;

    // 3. 推理（ort 2.0.0-rc.12 的 inputs! 宏直接返回 SessionInputs，不再是 Result）
    let infer_start = std::time::Instant::now();
    let outputs = embedder
        .session
        .run(ort::inputs! {
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
            "token_type_ids" => token_type_ids_tensor,
        })
        .map_err(|e| {
            tracing::warn!("embed ONNX 推理失败: {}", e);
            IpcError::Internal(format!("ONNX 推理失败: {e}"))
        })?;
    let infer_elapsed = infer_start.elapsed();
    // 推理通常是主要耗时阶段，统一用 info 级别输出便于排查
    tracing::info!(
        "embed ONNX 推理完成: seq_len={}, 耗时 {:?}",
        seq_len,
        infer_elapsed
    );

    // 4. 提取 last_hidden_state [1, seq_len, 384]
    //    ort 2.0.0-rc.12 的 try_extract_tensor 返回 (&Shape, &[f32])，而非 ArrayView
    let (_shape, tensor_data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| IpcError::Internal(format!("提取输出张量失败: {e}")))?;

    // 5. Mean pooling + L2 normalize
    //    tensor_data 按 row-major 布局：[seq_len, 384]
    let post_start = std::time::Instant::now();
    let mut token_emb = Vec::with_capacity(seq_len * HIDDEN);
    for l in 0..seq_len {
        for h in 0..HIDDEN {
            token_emb.push(tensor_data[l * HIDDEN + h]);
        }
    }
    let mut pooled = mean_pool(&token_emb, &attention_mask, HIDDEN);
    l2_normalize(&mut pooled);
    tracing::debug!("embed 后处理（pooling+归一化）耗时 {:?}", post_start.elapsed());

    tracing::info!(
        "embed 完成: chars={}, seq_len={}, 总耗时 {:?} (首次初始化={})",
        text_len,
        seq_len,
        total_start.elapsed(),
        was_uninit
    );
    Ok(pooled)
}

/// 生成 embedding 并序列化为 JSON 字符串（sqlite-vec 接受 JSON 格式）
pub fn embed_to_json(text: &str) -> IpcResult<String> {
    let start = std::time::Instant::now();
    let vec = embed(text)?;
    let json = serde_json::to_string(&vec)
        .map_err(|e| IpcError::Internal(format!("序列化 embedding 失败: {e}")))?;
    tracing::debug!(
        "embed_to_json 完成: dims={}, json_len={}, 序列化耗时 {:?}",
        vec.len(),
        json.len(),
        start.elapsed()
    );
    Ok(json)
}
