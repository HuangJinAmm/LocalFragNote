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

static EMBEDDER: OnceLock<Mutex<Embedder>> = OnceLock::new();

/// 获取模型缓存目录（用户数据目录下的 models/all-MiniLM-L6-v2）
fn model_dir() -> IpcResult<PathBuf> {
    let dir = dirs::data_dir()
        .ok_or_else(|| IpcError::Internal("无法获取用户数据目录".into()))?
        .join("memos")
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
fn download_file(url: &str, dest: &std::path::Path) -> IpcResult<()> {
    if dest.exists() {
        return Ok(());
    }
    tracing::info!("下载模型文件: {} -> {}", url, dest.display());
    let response = ureq::get(url)
        .call()
        .map_err(|e| IpcError::Internal(format!("下载失败 {url}: {e}")))?;
    let mut file = std::fs::File::create(dest)
        .map_err(|e| IpcError::Internal(format!("创建文件失败 {}: {e}", dest.display())))?;
    std::io::copy(&mut response.into_reader(), &mut file)
        .map_err(|e| IpcError::Internal(format!("写入文件失败: {e}")))?;
    Ok(())
}

/// 确保模型文件已下载
fn ensure_model_files() -> IpcResult<ModelPaths> {
    let paths = model_paths()?;
    download_file(&format!("{MODEL_BASE_URL}/onnx/model.onnx"), &paths.model)?;
    download_file(&format!("{MODEL_BASE_URL}/tokenizer.json"), &paths.tokenizer)?;
    Ok(paths)
}

/// 获取 Embedder 单例（懒加载，首次调用时下载模型并初始化）
fn get_embedder() -> &'static Mutex<Embedder> {
    EMBEDDER.get_or_init(|| {
        let paths = ensure_model_files().expect("无法下载 embedding 模型文件");

        // 加载 ONNX session
        let session = Session::builder()
            .expect("创建 SessionBuilder 失败")
            .with_optimization_level(GraphOptimizationLevel::Level1)
            .expect("设置优化级别失败")
            .with_intra_threads(1)
            .expect("设置线程数失败")
            .commit_from_file(&paths.model)
            .expect("加载 ONNX 模型失败");

        // 加载 tokenizer 并配置 truncation/padding
        let mut tokenizer = Tokenizer::from_file(&paths.tokenizer)
            .expect("加载 tokenizer.json 失败");

        let mut trunc = TruncationParams::default();
        trunc.max_length = MAX_SEQ_LEN;
        trunc.direction = TruncationDirection::Right;
        trunc.strategy = TruncationStrategy::LongestFirst;
        tokenizer
            .with_truncation(Some(trunc))
            .expect("设置 truncation 失败");

        let mut pad = PaddingParams::default();
        pad.strategy = PaddingStrategy::BatchLongest;
        pad.direction = PaddingDirection::Right;
        pad.pad_id = 0;
        pad.pad_type_id = 0;
        pad.pad_token = "[PAD]".to_string();
        tokenizer.with_padding(Some(pad));

        Mutex::new(Embedder { session, tokenizer })
    })
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

/// 生成单条文本的 embedding（384维 f32）
pub fn embed(text: &str) -> IpcResult<Vec<f32>> {
    let mutex = get_embedder();
    let embedder = mutex
        .lock()
        .expect("embedding 模型 Mutex poisoned");

    // 1. tokenize
    let encoding = embedder
        .tokenizer
        .encode(text, true)
        .map_err(|e| IpcError::Internal(format!("tokenize 失败: {e}")))?;

    let seq_len = encoding.get_ids().len();
    let input_ids: Vec<i64> = encoding.get_ids().iter().map(|v| *v as i64).collect();
    let attention_mask: Vec<i64> = encoding.get_attention_mask().iter().map(|v| *v as i64).collect();
    let token_type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|v| *v as i64).collect();

    // 2. 构造输入张量 [1, seq_len]
    let input_ids_arr = Array2::from_shape_vec((1, seq_len), input_ids)
        .map_err(|e| IpcError::Internal(format!("输入张量构造失败: {e}")))?;
    let attention_mask_arr = Array2::from_shape_vec((1, seq_len), attention_mask.clone())
        .map_err(|e| IpcError::Internal(format!("输入张量构造失败: {e}")))?;
    let token_type_ids_arr = Array2::from_shape_vec((1, seq_len), token_type_ids)
        .map_err(|e| IpcError::Internal(format!("输入张量构造失败: {e}")))?;

    // 3. 推理（按输入名传递，all-MiniLM-L6-v2 有三个输入）
    let outputs = embedder
        .session
        .run(ort::inputs! {
            "input_ids" => input_ids_arr,
            "attention_mask" => attention_mask_arr,
            "token_type_ids" => token_type_ids_arr,
        }
        .map_err(|e| IpcError::Internal(format!("构造 ONNX 输入失败: {e}")))?)
        .map_err(|e| IpcError::Internal(format!("ONNX 推理失败: {e}")))?;

    // 4. 提取 last_hidden_state [1, seq_len, 384]
    let last_hidden = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| IpcError::Internal(format!("提取输出张量失败: {e}")))?;
    let view = last_hidden.view();

    // 5. Mean pooling + L2 normalize
    let mut token_emb = Vec::with_capacity(seq_len * HIDDEN);
    for l in 0..seq_len {
        for h in 0..HIDDEN {
            token_emb.push(view[[0, l, h]]);
        }
    }
    let mut pooled = mean_pool(&token_emb, &attention_mask, HIDDEN);
    l2_normalize(&mut pooled);

    Ok(pooled)
}

/// 生成 embedding 并序列化为 JSON 字符串（sqlite-vec 接受 JSON 格式）
pub fn embed_to_json(text: &str) -> IpcResult<String> {
    let vec = embed(text)?;
    serde_json::to_string(&vec)
        .map_err(|e| IpcError::Internal(format!("序列化 embedding 失败: {e}")))
}
