//! 文档摘要命令：使用 markitdown-rs 转换文档为文本，
//! - zip: 直接返回文件结构
//! - 其它文档: 截断后调 LLM 生成总结

use crate::ai::llm_call::call_first_provider;
use crate::error::{IpcError, IpcResult};
use crate::state::AppState;
use markitdown::{model::ConversionOptions, MarkItDown};
use serde::Serialize;

/// 命令返回结果
#[derive(Debug, Serialize)]
pub struct DocumentSummaryResult {
    /// "summary" | "structure" | "skipped"
    pub kind: String,
    /// 已格式化、可直接追加到笔记末尾的 markdown 块
    pub markdown: String,
}

/// 支持的文档扩展名（不含点号，小写）
const SUPPORTED_DOC_EXTS: &[&str] = &[
    "pdf", "docx", "doc", "pptx", "ppt", "xlsx", "xls",
    "html", "htm", "csv", "xml", "rss", "atom",
];
/// zip 扩展名
const ZIP_EXTS: &[&str] = &["zip"];
/// 送 LLM 前的字符截断预算
const MAX_CHARS: usize = 6000;

const SUMMARY_SYSTEM_PROMPT: &str = "你是一位专业的文档摘要助手，擅长从结构化或半结构化的文本中提炼核心信息。
根据用户提供的文档内容，生成一份简洁的摘要，包含：
1. 文档主题与目的
2. 关键论点、数据或结构
3. 重要结论或可操作要点
要求：
- 使用用户提问的语言
- 摘要控制在 200-400 字
- 使用 markdown 列表/段落组织，清晰易读
- 不要复述原文，要提炼";

/// markitdown 转换阶段（spawn_blocking 内）的产物。
///
/// 不访问 `state`，避免 `tauri::State<'_>` 跨 `'static` 闭包的生命周期问题。
/// LLM 调用留到 async body 执行（与 `suggest_tags` 同模式）。
enum Converted {
    /// 已完成（skipped 或 structure），可直接返回
    Done(DocumentSummaryResult),
    /// 需 LLM 总结：携带文件名与截断后的文本
    NeedsSummary { filename: String, truncated: String },
}

/// 提取文档摘要 / zip 结构
///
/// - blob: 文件原始字节
/// - filename: 文件名（用于推断扩展名与展示）
#[tauri::command]
pub async fn summarize_document_content(
    state: tauri::State<'_, AppState>,
    blob: Vec<u8>,
    filename: String,
) -> IpcResult<DocumentSummaryResult> {
    // Phase 1: markitdown 转换（阻塞，放 spawn_blocking；不访问 state）
    let converted = tauri::async_runtime::spawn_blocking(move || -> Converted {
        let ext = extract_extension(&filename);

        // 1. 判定类型
        let is_zip = ZIP_EXTS.contains(&ext.as_str());
        let is_doc = SUPPORTED_DOC_EXTS.contains(&ext.as_str());
        if !is_zip && !is_doc {
            return Converted::Done(DocumentSummaryResult {
                kind: "skipped".into(),
                markdown: String::new(),
            });
        }

        // 2. markitdown 转换
        let md = MarkItDown::new();
        let options = ConversionOptions {
            file_extension: Some(format!(".{}", ext)),
            url: None,
            llm_client: None,
            llm_model: None,
        };
        // markitdown 失败 → 静默跳过（R6），不 toast
        let result = match md.convert_bytes(&blob, Some(options)) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("markitdown 转换 {} 失败: {}", filename, e);
                return Converted::Done(DocumentSummaryResult {
                    kind: "skipped".into(),
                    markdown: String::new(),
                });
            }
        };

        let Some(conversion_result) = result else {
            return Converted::Done(DocumentSummaryResult {
                kind: "skipped".into(),
                markdown: String::new(),
            });
        };

        let text = conversion_result.text_content;

        // 3a. zip: 直接返回结构
        if is_zip {
            let markdown = format!(
                "## 🗜️ {} 文件结构\n\n```text\n{}\n```",
                filename, text
            );
            return Converted::Done(DocumentSummaryResult {
                kind: "structure".into(),
                markdown,
            });
        }

        // 3b. 文档: 截断后交由 Phase 2 调 LLM 总结
        let truncated: String = text.chars().take(MAX_CHARS).collect();
        if truncated.trim().is_empty() {
            return Converted::Done(DocumentSummaryResult {
                kind: "skipped".into(),
                markdown: String::new(),
            });
        }

        Converted::NeedsSummary { filename, truncated }
    })
    .await
    .map_err(|e| IpcError::Internal(format!("摘要任务失败: {e}")))?;

    // Phase 2: 组装结果（LLM 调用在 async body，state 可用）
    match converted {
        Converted::Done(result) => Ok(result),
        Converted::NeedsSummary { filename, truncated } => {
            let store = state.store();
            let summary = call_first_provider(
                &store,
                SUMMARY_SYSTEM_PROMPT,
                &format!(
                    "以下是文档《{}》的前 {} 字符内容，请生成一份简洁的中文摘要，概括其核心主题、关键信息与结构要点。只输出摘要正文，不要额外说明。\n\n---\n{}",
                    filename,
                    truncated.chars().count(),
                    truncated
                ),
            )?; // LLM 失败 → 返回 Err，前端 toast

            let markdown = format!("## 📄 {} 摘要\n\n{}", filename, summary.trim());

            Ok(DocumentSummaryResult {
                kind: "summary".into(),
                markdown,
            })
        }
    }
}

/// 从文件名提取扩展名（小写，不含点号）
fn extract_extension(filename: &str) -> String {
    filename
        .rsplit('.')
        .next()
        .filter(|s| !s.is_empty() && s.len() < filename.len())
        .map(|s| s.to_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_ext_pdf() {
        assert_eq!(extract_extension("report.PDF"), "pdf");
        assert_eq!(extract_extension("doc.docx"), "docx");
        assert_eq!(extract_extension("archive.ZIP"), "zip");
    }

    #[test]
    fn extract_ext_no_ext() {
        assert_eq!(extract_extension("noext"), "");
        assert_eq!(extract_extension(""), "");
    }

    #[test]
    fn extract_ext_multiple_dots() {
        assert_eq!(extract_extension("my.report.final.pdf"), "pdf");
    }
}
