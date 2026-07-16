# 附件文档摘要 / ZIP 结构提取 — 设计

- **日期**: 2026-07-16
- **状态**: 设计已确认，待实现
- **作者**: 助手 + 用户协同设计

## 1. 背景与目标

LocalFragNote 目前允许用户给笔记添加文件附件（图片、音频、文档等），附件以 blob 形式存入数据库或本地文件系统。用户希望在添加文档类附件时，能够：

1. 对 **PDF / DOCX / PPTX / XLSX / DOC / HTML / CSV / XML** 等文档，自动提取前若干页文本，调用 LLM 生成总结，追加到笔记末尾。
2. 对 **ZIP** 压缩包，提取内部文件结构信息（文件列表），追加到笔记末尾。
3. 该功能由用户开关控制，默认关闭。

使用 [`markitdown-rs`](https://github.com/uhobnil/markitdown-rs)（Rust 版 markitdown，支持 xlsx/docx/pptx/pdf/images/audio/html/csv/xml/zip）做文档转文本。

## 2. 需求与约束

### 2.1 功能需求

| ID | 需求 |
|----|------|
| R1 | 添加文档类附件时（开关开启），即时调用 LLM 生成总结并追加到笔记末尾 |
| R2 | 添加 ZIP 附件时（开关开启），提取内部文件结构并追加到笔记末尾 |
| R3 | 开关默认关闭；位置：编辑器工具栏切换按钮 + localStorage 持久化（与 `autoTagEnabled` 同模式） |
| R4 | 仅在"添加附件时"触发（上传按钮 / 拖拽 / 粘贴统一入口），不干扰既有附件流程 |
| R5 | LLM 调用失败 → **静默跳过追加 + toast 提示**，不阻塞笔记创建/保存 |
| R6 | markitdown 转换失败或不支持类型 → 静默跳过，无 toast |
| R7 | 文档送 LLM 前截断到前 6000 字符 |

### 2.2 非功能需求

- **N1**: 不引入 DB schema 变更、不新增迁移。
- **N2**: 后端命令无状态，不依赖附件已持久化（接收原始字节 + 文件名）。
- **N3**: 不破坏现有 `autoTagEnabled`、`uploadService`、`memoService.save` 流程。
- **N4**: 与现有依赖（ort 2.0.0-rc.12 / image 0.25 / rusqlite 0.32 / ureq 2）无版本冲突。
- **N5**: 回答/UI 使用用户当前语言（zh-Hans / en 等已有 i18n 体系）。

### 2.3 范围外（Out of Scope）

- 不支持对已存档的旧附件批量补做摘要（YAGNI）。
- 不持久化摘要结果到附件元数据（`attachment.payload`），摘要直接进笔记内容。
- 不为图片/音频/视频做摘要（图片走 `thumbnail`、音频走 `transcriptionService`，已有通路）。
- 不支持流式 SSE 推送（摘要是一次性追加，非流式场景）。

## 3. 架构设计

### 3.1 数据流

```
用户添加文件（上传/拖拽/粘贴）
       │
       ▼
编辑器 onFilesSelected 回调（统一入口）
       │
       ├─ 开关关闭 OR 类型不支持 ──► dispatch(ADD_LOCAL_FILE)  [现有行为]
       │
       └─ 开关开启 AND 类型支持
              │
              ├─ dispatch(ADD_LOCAL_FILE)         [先正常入队，避免阻塞 UI]
              │
              └─ 异步: toast("正在为 {filename} 生成摘要…")
                       │
                       ▼
                  invoke("summarize_document_content", { blob, filename })
                       │
                       ▼
             后端 commands::document_summary::summarize_document_content
                       │
                       ├─ markitdown::MarkItDown.convert_bytes(blob, opts)
                       │       │
                       │       ▼
                       │   text_content (String)
                       │       │
                       │   ┌───┴───────────────────────────┐
                       │   │ zip?                          │
                       │   ├─ yes → kind="structure",      │
                       │   │       markdown = markitdown 原文│
                       │   │       (即 zip 内文件结构列表)  │
                       │   │                                │
                       │   └─ no  → 截断到前 6000 字符        │
                       │           → call_llm_first_provider │
                       │           → kind="summary",        │
                       │             markdown = 格式化总结块 │
                       │
                       ├─ markitdown 失败/不支持 → kind="skipped"
                       │
                       └─ LLM 失败 → 返回 Err（前端 toast，不追加）
                       │
                       ▼
             前端收到 DocumentSummaryResult
                       │
                       ├─ kind="skipped" → 静默，无操作
                       ├─ kind="structure" / "summary"
                       │       │
                       │       ▼
                       │   editor.appendMarkdown("\n\n" + markdown)
                       │       │
                       │       ▼
                       │   toast("已生成摘要并追加到笔记末尾")
                       │
                       └─ 错误（Err） → toast(失败信息)
```

### 3.2 关键决策

| 决策点 | 选择 | 理由 |
|--------|------|------|
| 触发时机 | 添加附件时即时生成 | 所见即所得，用户可在保存前编辑摘要 |
| 开关位置 | 编辑器工具栏 + localStorage | 与 `autoTagEnabled` 一致，颗粒度灵活，无需 DB 迁移 |
| 后端命令签名 | 接收 `{ blob, filename }` | 不依赖附件已持久化，匹配"添加附件时即时生成" |
| 截断预算 | 6000 字符 | 用户已确认 |
| LLM 失败策略 | 静默跳过 + toast | 用户已确认；避免追加不完整原文造成噪音 |
| markitdown 失败策略 | 静默跳过（无 toast） | 不支持的类型本就不应打扰用户 |
| 摘要追加位置 | 笔记内容末尾 | 满足"补充到笔记后面"语义 |
| LLM 调用复用 | 抽取共享 helper | 与 `suggest_tags` 同构，避免重复 |
| zip 处理 | 不调 LLM，直接返回结构 | 文件结构本身即信息，无需总结；省 token |

## 4. 详细设计

### 4.1 后端

#### 4.1.1 依赖

**`src-tauri/Cargo.toml`** 新增：

```toml
markitdown = "0.1.11"
```

#### 4.1.2 共享 LLM helper

新增 `src-tauri/src/ai/llm_call.rs`：

```rust
//! 非流式 LLM 调用 helper：复用于 suggest_tags 与 document_summary

use crate::ai::provider::load_providers;
use crate::error::{IpcError, IpcResult};
use memos_core::Store;
use serde_json::{json, Value};

/// 使用首个已配置 provider 发起非流式 chat completion，返回 assistant 文本。
///
/// - 未配置 provider → BadRequest
/// - HTTP/解析失败 → Internal
pub fn call_first_provider(
    store: &Store,
    system_prompt: &str,
    user_message: &str,
) -> IpcResult<String> {
    let providers = load_providers(store);
    let provider = providers
        .first()
        .cloned()
        .ok_or_else(|| IpcError::BadRequest("未配置 AI provider，请先在设置中配置".into()))?;

    let body = json!({
        "model": provider.model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_message },
        ],
        "stream": false,
    });

    let url = format!("{}/chat/completions", provider.base_url.trim_end_matches('/'));
    let mut req = ureq::post(&url).set("Content-Type", "application/json");
    if !provider.api_key.is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", provider.api_key));
    }

    let response = req
        .send_string(&body.to_string())
        .map_err(|e| IpcError::Internal(format!("AI 请求失败: {e}")))?;

    if response.status() >= 400 {
        let status = response.status();
        let body_text = response.into_string().unwrap_or_default();
        return Err(IpcError::Internal(format!("HTTP {status}: {body_text}")));
    }

    let resp_json: Value = serde_json::from_str(
        &response.into_string().map_err(|e| IpcError::Internal(format!("读取响应失败: {e}")))?,
    )
    .map_err(|e| IpcError::Internal(format!("解析响应 JSON 失败: {e}")))?;

    Ok(resp_json
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string())
}
```

`src-tauri/src/ai/mod.rs` 增加 `pub mod llm_call;`。

**重构 `suggest_tags`** ([memo.rs:386-485](file:///d:/6-ai/LocalFragNote/src-tauri/src/commands/memo.rs#L386-L485))：将其内部的 provider 加载 / HTTP / JSON 解析替换为调用 `llm_call::call_first_provider`，保留 system_prompt 与 user_message 构造、解析逗号分隔标签的逻辑。这是为避免重复而做的最小化重构，**不改变其行为**。

#### 4.1.3 文档摘要命令

新增 `src-tauri/src/commands/document_summary.rs`：

```rust
//! 文档摘要命令：使用 markitdown-rs 转换文档为文本，
//! - zip: 直接返回文件结构
//! - 其它文档: 截断后调 LLM 生成总结

use crate::ai::llm_call::call_first_provider;
use crate::error::{IpcError, IpcResult};
use crate::state::AppState;
use markitdown::{ConversionOptions, MarkItDown};
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
    tauri::async_runtime::spawn_blocking(move || {
        let ext = extract_extension(&filename);

        // 1. 判定类型
        let is_zip = ZIP_EXTS.contains(&ext.as_str());
        let is_doc = SUPPORTED_DOC_EXTS.contains(&ext.as_str());
        if !is_zip && !is_doc {
            return Ok(DocumentSummaryResult {
                kind: "skipped".into(),
                markdown: String::new(),
            });
        }

        // 2. markitdown 转换
        let mut md = MarkItDown::new();
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
                return Ok(DocumentSummaryResult {
                    kind: "skipped".into(),
                    markdown: String::new(),
                });
            }
        };

        let Some(conversion_result) = result else {
            return Ok(DocumentSummaryResult {
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
            return Ok(DocumentSummaryResult {
                kind: "structure".into(),
                markdown,
            });
        }

        // 3b. 文档: 截断 + LLM 总结
        let truncated: String = text.chars().take(MAX_CHARS).collect();
        if truncated.trim().is_empty() {
            return Ok(DocumentSummaryResult {
                kind: "skipped".into(),
                markdown: String::new(),
            });
        }

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
    })
    .await
    .map_err(|e| IpcError::Internal(format!("摘要任务失败: {e}")))?
}

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
```

`src-tauri/src/commands/mod.rs` 增加 `pub mod document_summary;`。

**`src-tauri/src/main.rs`** 在 `invoke_handler` 数组中注册：

```rust
commands::document_summary::summarize_document_content,
```

（位置：放在 `commands::attachment::*` 之后）

### 4.2 前端

#### 4.2.1 新服务

新增 `src/components/MemoEditor/services/documentSummaryService.ts`：

```typescript
import { invoke } from "@tauri-apps/api/core";

export interface DocumentSummaryResult {
  kind: "summary" | "structure" | "skipped";
  markdown: string;
}

/** 支持文档摘要的扩展名（小写，不含点号） */
const SUPPORTED_DOC_EXTS = new Set([
  "pdf", "docx", "doc", "pptx", "ppt", "xlsx", "xls",
  "html", "htm", "csv", "xml", "rss", "atom",
]);
const ZIP_EXTS = new Set(["zip"]);

/** 判断文件是否应触发摘要（开关开启时由调用方决定是否真的调用） */
export function isSummarizable(filename: string): boolean {
  const ext = extractExt(filename);
  return SUPPORTED_DOC_EXTS.has(ext) || ZIP_EXTS.has(ext);
}

/** 调用后端命令 */
export const documentSummaryService = {
  async summarize(file: File): Promise<DocumentSummaryResult> {
    const blob = new Uint8Array(await file.arrayBuffer());
    return invoke<DocumentSummaryResult>("summarize_document_content", {
      blob,
      filename: file.name,
    });
  },
};

function extractExt(filename: string): string {
  const idx = filename.lastIndexOf(".");
  if (idx <= 0 || idx === filename.length - 1) return "";
  return filename.slice(idx + 1).toLowerCase();
}
```

#### 4.2.2 编辑器集成

**`src/components/MemoEditor/constants.ts`** 新增：

```typescript
export const SUMMARY_STORAGE_KEY = "memo-editor.summary-enabled";
```

**`src/components/MemoEditor/index.tsx`** 改动：

1. 引入 `useLocalStorage`、`documentSummaryService`、`isSummarizable`、`SUMMARY_STORAGE_KEY`。
2. 在 `MemoEditorImpl` 内增加状态：
   ```typescript
   const [summaryEnabled, setSummaryEnabled] = useLocalStorage(SUMMARY_STORAGE_KEY, false);
   ```
3. 增加文件添加钩子（统一拦截上传/拖拽/粘贴）：
   ```typescript
   const handleFileAdded = useCallback(async (localFile: LocalFile) => {
     dispatch(actions.addLocalFile(localFile)); // 先入队，不阻塞 UI
     if (!summaryEnabled) return;
     if (!isSummarizable(localFile.file.name)) return;

     const toastId = toast.loading(t("editor.summary.generating", { name: localFile.file.name }));
     try {
       const result = await documentSummaryService.summarize(localFile.file);
       if (result.kind === "skipped") {
         toast.dismiss(toastId);
         return;
       }
       const editor = editorRef.current;
       if (editor) {
         editor.appendMarkdown(`\n\n${result.markdown}`);
       }
       toast.success(t("editor.summary.done"), { id: toastId });
     } catch (e) {
       toast.error(
         t("editor.summary.failed", { name: localFile.file.name, reason: String(e) }),
         { id: toastId },
       );
     }
   }, [actions, dispatch, summaryEnabled, t]);
   ```
4. 在所有调用 `dispatch(actions.addLocalFile(...))` 的位置改为调用 `handleFileAdded(...)`：
   - [index.tsx:137](file:///d:/6-ai/LocalFragNote/src/components/MemoEditor/index.tsx#L137)（`handleTranscribeRecordedAudio` 失败回退）
   - [index.tsx:145](file:///d:/6-ai/LocalFragNote/src/components/MemoEditor/index.tsx#L145)（转写为空回退）
   - [index.tsx:155](file:///d:/6-ai/LocalFragNote/src/components/MemoEditor/index.tsx#L155)（转写异常回退）
   - [index.tsx:171](file:///d:/6-ai/LocalFragNote/src/components/MemoEditor/index.tsx#L171)（录音完成非转写模式）
   - 其它组件中通过 `onFilesSelected` / `dispatch(actions.addLocalFile)` 的路径：需让 `onFilesSelected` 回调内部走 `handleFileAdded`（详见 4.2.4）。

   **注意**: 转写失败/为空的回退路径**不再触发摘要**（音频文件本就不在 `isSummarizable` 范围内，会被静默跳过）。但为语义清晰，仍统一走 `handleFileAdded`。

5. 向 `EditorToolbar` 透传 props：
   ```typescript
   <EditorToolbar
     ...现有 props
     summaryEnabled={summaryEnabled}
     onToggleSummary={() => setSummaryEnabled(v => !v)}
   />
   ```

#### 4.2.3 EditorController 扩展

`src/components/MemoEditor/Editor/controller.ts` 增加方法：

```typescript
/** 在笔记内容末尾追加 markdown 文本 */
appendMarkdown(text: string): void {
  // 实现细节取决于现有 controller API（基于 CodeMirror / TipTap）：
  // 1. 将光标移至文档末尾
  // 2. 插入 text
  // 3. 滚动到新光标位置
  // 若 controller 已暴露 insertMarkdown(text, position) 类似 API，可直接复用
}
```

**实现细节**：在实现阶段会先读 `controller.ts` 与 `editor.css`、`extensions.ts`，选择最小侵入的实现。若 controller 已有 `insertMarkdown`，则 `appendMarkdown` 等价于"将选区设到末尾 + insertMarkdown"，无需新依赖。

#### 4.2.4 上传入口统一

`useFileUpload.ts` 的 `handleFileInputChange` 目前直接调 `onFilesSelected(localFiles)`。**不改动此 hook**，而是在 `MemoEditorImpl` 传入 `onFilesSelected` 时，把回调改成遍历 `localFiles` 调 `handleFileAdded`。这样上传按钮、拖拽、粘贴若都最终汇入 `onFilesSelected` 即统一覆盖。

需要先在实现阶段检索所有 `onFilesSelected` 与 `dispatch(actions.addLocalFile` 的调用点，确认是否所有入口都经过 `onFilesSelected`，若有直接 dispatch 的路径则一并改为 `handleFileAdded`。

#### 4.2.5 工具栏按钮

`src/components/MemoEditor/Toolbar/EditorToolbar.tsx` 仿 `autoTagEnabled` 按钮增加 `summaryEnabled` / `onToggleSummary` 按钮。复用现有 button 样式与 toggle 行为，仅 icon 与 tooltip 不同（icon 建议 `FileText` 或 `Sparkles`，来自 lucide-react）。

#### 4.2.6 i18n

`src/locales/zh-Hans.json` 与 `src/locales/en.json` 新增：

```json
{
  "editor": {
    "summary": {
      "toggle": "添加文档附件时生成摘要",
      "toggle-hint": "对 PDF/DOCX/PPT/ZIP 等自动提取内容追加到笔记末尾",
      "generating": "正在为 {{name}} 生成摘要…",
      "done": "已生成摘要并追加到笔记末尾",
      "failed": "为 {{name}} 生成摘要失败：{{reason}}"
    }
  }
}
```

其它语种不强制翻译，fallback 到 en。

### 4.3 错误处理矩阵

| 场景 | 后端返回 | 前端行为 |
|------|----------|----------|
| 不支持的扩展名 | `kind="skipped"` | 静默，无 toast |
| markitdown 转换失败 | `kind="skipped"` | 静默，无 toast（后端 `tracing::warn`） |
| markitdown 返回 None | `kind="skipped"` | 静默 |
| 文档类，转换成功但文本为空 | `kind="skipped"` | 静默 |
| 文档类，未配置 provider | `Err(BadRequest)` | toast "未配置 AI provider" |
| 文档类，LLM HTTP 4xx/5xx | `Err(Internal)` | toast 失败原因 |
| zip，转换成功 | `kind="structure"` | 追加 + toast 成功 |
| 文档类，LLM 成功 | `kind="summary"` | 追加 + toast 成功 |
| IPC 序列化/反序列化错误 | `Err` | toast 失败原因 |

## 5. 测试策略

### 5.1 单元测试（Rust）

- `extract_extension`: 各种文件名边界（含多 `.`、无扩展名、大写）。
- `summarize_document_content` 不支持类型 → `kind="skipped"`（用最小字节）。
- markitdown 对真实小 PDF / 小 zip 的转换 smoke test（可放入 `tests/` 集成测试，依赖外部 fixture 文件，若不便则省略，依赖手动验证）。

### 5.2 手动验证清单

1. **编译**: `cargo check` 通过，无 markitdown 与现有依赖冲突。
2. **前端编译**: `npm run build` 或 `tsc --noEmit` 通过。
3. 开关关闭：
   - 添加 PDF → 笔记无变化，附件正常入队。
4. 开关开启：
   - 添加 PDF → 末尾出现 `## 📄 xxx.pdf 摘要` 块。
   - 添加 DOCX/PPTX/XLSX → 同上。
   - 添加 ZIP → 末尾出现 `## 🗜️ xxx.zip 文件结构` 块，含文件列表。
   - 添加图片/音频 → 无变化（被 `isSummarizable` 拒绝）。
   - 添加 .txt → 无变化（不在支持列表）。
5. 失败路径：
   - 未配置 provider → 添加 PDF → toast 失败，附件仍入队，笔记无追加。
   - 断网 / provider 不可达 → 同上。
6. 多文件：一次添加 PDF + 图片 → 仅 PDF 触发摘要，图片正常入队。
7. 持久化：刷新页面后开关状态保留。
8. 编辑摘要：用户可在末尾手动编辑/删除追加的摘要块。

## 6. 影响范围

### 6.1 新增文件

- `src-tauri/src/ai/llm_call.rs`
- `src-tauri/src/commands/document_summary.rs`
- `src/components/MemoEditor/services/documentSummaryService.ts`

### 6.2 修改文件

- `src-tauri/Cargo.toml`（+markitdown 依赖）
- `src-tauri/src/ai/mod.rs`（+pub mod llm_call）
- `src-tauri/src/commands/mod.rs`（+pub mod document_summary）
- `src-tauri/src/commands/memo.rs`（重构 `suggest_tags` 使用 helper，**行为不变**）
- `src-tauri/src/main.rs`（注册新命令）
- `src/components/MemoEditor/constants.ts`（+SUMMARY_STORAGE_KEY）
- `src/components/MemoEditor/index.tsx`（+state/handler/props）
- `src/components/MemoEditor/Editor/controller.ts`（+appendMarkdown）
- `src/components/MemoEditor/Toolbar/EditorToolbar.tsx`（+toggle 按钮）
- `src/locales/zh-Hans.json`、`src/locales/en.json`（+i18n keys）

### 6.3 不变

- DB schema、迁移、`attachment` 表结构。
- `uploadService`、`memoService.save` 流程。
- `connect.ts` 的 attachment 适配层。
- 现有 `autoTagEnabled` 行为。
- ONNX Runtime / embedding 模块。

## 7. 风险与缓解

| 风险 | 缓解 |
|------|------|
| markitdown 依赖与 ort/image 版本冲突 | 实现第一步 `cargo check` 验证；markitdown 0.1.11 依赖树较轻，预计无冲突 |
| 大文件 IPC 传大 buffer 性能 | 本地 IPC，单次传输可接受；markitdown 内部处理字节流；截断后送 LLM 控制成本 |
| LLM 调用慢导致用户等待 | toast 显示 loading；不阻塞 UI（先 `addLocalFile`，再异步摘要） |
| `appendMarkdown` 实现与现有 controller 耦合 | 实现阶段先读 controller.ts，优先复用 `insertMarkdown`；若有现成末尾插入 API 则直接用 |
| 漏改某个文件添加入口 | 实现阶段 grep 所有 `dispatch(actions.addLocalFile` 与 `onFilesSelected` 调用点，统一改为 `handleFileAdded` |
| `suggest_tags` 重构引入回归 | 保持 system_prompt / user_message / 标签解析逻辑不变，仅替换 HTTP/JSON 部分；保留现有测试 |

## 8. 开放问题

无。所有关键决策已在设计评审中确认。

---

**评审记录**:
- 2026-07-16: 用户确认触发时机（即时生成）、开关位置（工具栏+持久化）、截断预算（6000 字符）、LLM 失败策略（跳过+toast）。
