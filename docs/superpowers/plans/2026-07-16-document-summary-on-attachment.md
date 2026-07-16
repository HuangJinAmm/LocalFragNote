# 附件文档摘要 / ZIP 结构提取 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 添加文档/zip 附件时，用 markitdown-rs 提取内容并调 LLM 生成摘要（或对 zip 直接返回文件结构），追加到笔记末尾，由编辑器工具栏开关控制。

**Architecture:** 新增无状态后端命令 `summarize_document_content(blob, filename)`，前端在统一文件添加入口拦截，开关开启且类型支持时异步调用，结果通过 `EditorController.appendMarkdown` 追加。LLM 调用复用从 `suggest_tags` 抽取的共享 helper。

**Tech Stack:** Rust + Tauri 2 + markitdown-rs 0.1.11 + ureq 2；React + TypeScript + CodeMirror 6 + react-hot-toast + lucide-react。

## Global Constraints

- ort crate 版本必须与 ONNX Runtime DLL 匹配（ort 2.0.0-rc.12 ↔ ONNX Runtime 1.24.x）—— 不要改动 ort 相关依赖。
- ONNX Runtime DLL 由 build.rs 管理 —— 不要改动 build.rs。
- 不引入 DB schema 变更、不新增迁移。
- 回答/UI 使用用户当前语言；代码注释使用中文（与现有代码一致）。
- markitdown 依赖版本：`0.1.11`（最新 release）。
- LLM 失败 → 静默跳过追加 + toast 提示；markitdown 失败 → 静默跳过（无 toast）。
- 文档送 LLM 前截断到前 6000 字符。
- 开关默认关闭，持久化到 localStorage（与 `AUTO_TAG_STORAGE_KEY` 同模式）。

**参考设计文档**: `docs/superpowers/specs/2026-07-16-document-summary-on-attachment-design.md`

---

## 文件结构

### 新增文件
- `src-tauri/src/ai/llm_call.rs` — 非流式 LLM 调用共享 helper（`call_first_provider`）
- `src-tauri/src/commands/document_summary.rs` — 文档摘要 IPC 命令
- `src/components/MemoEditor/services/documentSummaryService.ts` — 前端服务封装

### 修改文件
- `src-tauri/Cargo.toml` — +markitdown 依赖
- `src-tauri/src/ai/mod.rs` — +`pub mod llm_call;`
- `src-tauri/src/commands/mod.rs` — +`pub mod document_summary;`
- `src-tauri/src/commands/memo.rs` — 重构 `suggest_tags` 使用 helper（行为不变）
- `src-tauri/src/main.rs` — 注册新命令
- `src/components/MemoEditor/constants.ts` — +`SUMMARY_STORAGE_KEY`
- `src/components/MemoEditor/types/editorController.ts` — +`appendMarkdown`
- `src/components/MemoEditor/Editor/controller.ts` — 实现 `appendMarkdown`
- `src/components/MemoEditor/types/components.ts` — +`onFileAdded` / `summaryEnabled` props
- `src/components/MemoEditor/components/EditorContent.tsx` — 用 `onFileAdded` 替代直接 dispatch
- `src/components/MemoEditor/Toolbar/InsertMenu.tsx` — 用 `onFileAdded` 替代直接 dispatch
- `src/components/MemoEditor/Toolbar/EditorToolbar.tsx` — +summary toggle 按钮
- `src/components/MemoEditor/services/index.ts` — +export documentSummaryService
- `src/components/MemoEditor/index.tsx` — +state/handler/props 串联
- `src/locales/zh-Hans.json` / `src/locales/en.json` — +i18n keys

---

## Task 1: 添加 markitdown 依赖并验证编译

**Files:**
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Produces: `markitdown = "0.1.11"` 可用，`MarkItDown::new()` / `convert_bytes()` / `ConversionOptions` / `DocumentConverterResult` 类型可导入。

- [ ] **Step 1: 添加依赖**

编辑 `src-tauri/Cargo.toml`，在 `[dependencies]` 末尾（`chrono = "0.4"` 之后）追加：

```toml
markitdown = "0.1.11"
```

- [ ] **Step 2: 验证编译**

Run: `cd src-tauri && cargo check`
Expected: 编译通过，无与 ort/image/rusqlite/ureq 的版本冲突。若出现冲突，记录错误并停止，回退依赖。

- [ ] **Step 3: 冒烟测试 markitdown API 可用**

在 `src-tauri/src/main.rs` 的 `ping` 命令下方临时加一个测试函数（仅用于编译验证，不注册为命令）：

```rust
#[allow(dead_code)]
fn _markitdown_smoke() {
    let mut md = markitdown::MarkItDown::new();
    let opts = markitdown::ConversionOptions {
        file_extension: Some(".txt".into()),
        url: None,
        llm_client: None,
        llm_model: None,
    };
    let _ = md.convert_bytes(b"hello", Some(opts));
}
```

Run: `cd src-tauri && cargo check`
Expected: 通过。若通过，删除该冒烟函数（Step 4 会用真实模块替代）。

- [ ] **Step 4: 删除冒烟函数**

删除 Step 3 临时加的 `_markitdown_smoke` 函数。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml
git commit -m "chore: add markitdown-rs dependency for document summary feature"
```

---

## Task 2: 新增 LLM 调用共享 helper

**Files:**
- Create: `src-tauri/src/ai/llm_call.rs`
- Modify: `src-tauri/src/ai/mod.rs`

**Interfaces:**
- Produces: `crate::ai::llm_call::call_first_provider(store: &Store, system_prompt: &str, user_message: &str) -> IpcResult<String>` — 使用首个已配置 provider 发起非流式 OpenAI 兼容 chat completion，返回 assistant 文本。未配置 provider → `IpcError::BadRequest`；HTTP/解析失败 → `IpcError::Internal`。

- [ ] **Step 1: 创建 `src-tauri/src/ai/llm_call.rs`**

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
        &response
            .into_string()
            .map_err(|e| IpcError::Internal(format!("读取响应失败: {e}")))?,
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

- [ ] **Step 2: 注册模块**

编辑 `src-tauri/src/ai/mod.rs`，在末尾追加：

```rust
pub mod llm_call;
```

完整内容应为：

```rust
//! AI 相关模块：provider 配置、工具、SSE 解析

pub mod provider;
pub mod sse;
pub mod tools;
pub mod llm_call;
```

- [ ] **Step 3: 验证编译**

Run: `cd src-tauri && cargo check`
Expected: 通过（会有 `call_first_provider` 未使用的警告，Task 3 会消除）。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ai/llm_call.rs src-tauri/src/ai/mod.rs
git commit -m "feat(ai): add shared non-streaming LLM call helper"
```

---

## Task 3: 重构 suggest_tags 使用 helper

**Files:**
- Modify: `src-tauri/src/commands/memo.rs:386-485`（`suggest_tags` 函数）

**Interfaces:**
- Consumes: `crate::ai::llm_call::call_first_provider`（Task 2）
- Produces: `suggest_tags` 行为不变，仅内部实现替换。

**目标**: 把 `suggest_tags` 内部 provider 加载 / HTTP / JSON 解析替换为 `call_first_provider` 调用，消除重复代码。保留 system_prompt、user_message 构造、标签解析逻辑。

- [ ] **Step 1: 读取当前 suggest_tags 全文**

Run: 用 Read 工具读取 `src-tauri/src/commands/memo.rs` 第 386-485 行，确认当前实现细节（system_prompt 常量、user_message 构造、逗号分隔解析）。

- [ ] **Step 2: 替换 suggest_tags 函数体**

用 Edit 工具，将 `suggest_tags` 函数（从 `pub async fn suggest_tags(` 到函数结束 `}`）替换为以下内容。**保留原有的 system_prompt 字符串、existing_tags 提取、system_tags 查询、user_message 构造、标签解析逻辑**，只把 provider 加载 + HTTP + JSON 解析换成 `call_first_provider`：

```rust
/// AI 建议标签：根据笔记内容调用 LLM 生成标签建议
/// 将系统已有标签一并发送给 AI，优先复用已有标签，排除笔记中已存在的标签
#[tauri::command]
pub async fn suggest_tags(
    state: tauri::State<'_, AppState>,
    content: String,
) -> IpcResult<Vec<String>> {
    let store = state.store();

    // 笔记中已有的标签，用于排除
    let existing_tags: Vec<String> = markdown::extract_tags(&content);

    // 查询系统已有标签，提供给 AI 优先复用
    let system_tags: Vec<String> = store.with_conn(|c| -> memos_core::CoreResult<Vec<String>> {
        Ok(memos_core::tag::list_tags(c)?
            .into_iter()
            .map(|(name, _)| name)
            .collect())
    })?;

    let system_prompt = r"你是一位专业的笔记标签建议专家，擅长精准提取笔记的核心主题和所属科目。
    标签分两种类型：
    1. 分类标签: 用于笔记归类的科目类别（至少包含1个）。
    2. 主题标签: 精准概括笔记的核心主题。
    根据用户提供的笔记内容，建议 3-5 个合适的标签。至少需要1个分类标签。
    规则：
    1. 只返回标签名，不含 # 号
    2. 用逗号分隔
    3. 优先从「系统已有标签」中选择与笔记相关的标签，避免创建含义重复的新标签
    4. 只有当已有标签都无法归类的科目类别或概括笔记主题时，才创建新标签
    5. 不要返回笔记中已经包含的标签
    6. 标签应简短（1-4个字/词），能概括笔记主题
    7. 只返回标签列表，不要其他文字";

    let user_message = if system_tags.is_empty() {
        format!("笔记内容：\n\n{}", content)
    } else {
        format!(
            "系统已有标签：\n{}\n\n笔记内容：\n\n{}",
            system_tags.join(", "),
            content
        )
    };

    let ai_text = crate::ai::llm_call::call_first_provider(&store, system_prompt, &user_message)?;

    // 解析 AI 返回的标签（逗号或顿号分隔），去除 # 前缀，排除笔记中已有的标签
    let suggested: Vec<String> = ai_text
        .split([',', '，', '、'])
        .map(|s| s.trim().replace('#', "").trim().to_string())
        .filter(|s| !s.is_empty() && !existing_tags.contains(s))
        .take(10)
        .collect();

    Ok(suggested)
}
```

- [ ] **Step 3: 清理未使用的导入**

检查 `src-tauri/src/commands/memo.rs` 顶部的 `use` 语句，移除重构后不再使用的导入（例如 `serde_json::Value` 若仅 suggest_tags 用到则保留——它可能在文件其它地方也用到，需 grep 确认）。**不要**移除 `ureq`、`crate::ai::provider::{load_providers, save_providers, ProviderConfig}` 之外的导入除非确认未使用。

Run: `cd src-tauri && cargo check 2>&1 | grep warning`
Expected: 无 `unused import` 警告新增（原有警告保持原状）。

- [ ] **Step 4: 验证编译通过**

Run: `cd src-tauri && cargo check`
Expected: 通过，无错误。

- [ ] **Step 5: 运行现有测试确保无回归**

Run: `cd src-tauri && cargo test --lib`
Expected: 现有测试全部通过（特别是 `provider::tests` 模块的 serde 往返测试）。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/memo.rs
git commit -m "refactor(suggest_tags): use shared LLM helper, behavior unchanged"
```

---

## Task 4: 新增文档摘要后端命令

**Files:**
- Create: `src-tauri/src/commands/document_summary.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/main.rs`（注册命令）

**Interfaces:**
- Consumes: `crate::ai::llm_call::call_first_provider`（Task 2）
- Produces: Tauri 命令 `summarize_document_content(state, blob: Vec<u8>, filename: String) -> IpcResult<DocumentSummaryResult>`，其中 `DocumentSummaryResult { kind: "summary"|"structure"|"skipped", markdown: String }`。

- [ ] **Step 1: 创建 `src-tauri/src/commands/document_summary.rs`**

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

- [ ] **Step 2: 注册模块**

编辑 `src-tauri/src/commands/mod.rs`，在 `pub mod attachment;` 之后追加：

```rust
pub mod document_summary;
```

完整内容：

```rust
//! IPC 命令模块汇总
//!
//! 每个子模块对应一个领域：memo、attachment、reaction、memo_relation、setting

pub mod ai_chat;
pub mod attachment;
pub mod document_summary;
pub mod import_export;
pub mod lan;
pub mod llm_runner;
pub mod memo;
pub mod memo_relation;
pub mod reaction;
pub mod review;
pub mod setting;
```

- [ ] **Step 3: 在 main.rs 注册命令**

编辑 `src-tauri/src/main.rs`，在 `invoke_handler` 数组中 `commands::attachment::get_attachment_thumbnail,` 之后追加一行：

```rust
commands::document_summary::summarize_document_content,
```

具体定位：找到 `// attachment` 注释块下的最后一个 attachment 命令 `commands::attachment::get_attachment_thumbnail,`，在其后插入新行。

- [ ] **Step 4: 验证编译**

Run: `cd src-tauri && cargo check`
Expected: 通过。

- [ ] **Step 5: 运行单元测试**

Run: `cd src-tauri && cargo test --lib document_summary`
Expected: 3 个 `extract_ext_*` 测试通过。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/document_summary.rs src-tauri/src/commands/mod.rs src-tauri/src/main.rs
git commit -m "feat(commands): add summarize_document_content IPC command"
```

---

## Task 5: 前端 documentSummaryService

**Files:**
- Create: `src/components/MemoEditor/services/documentSummaryService.ts`
- Modify: `src/components/MemoEditor/services/index.ts`

**Interfaces:**
- Produces:
  - `documentSummaryService.summarize(file: File): Promise<DocumentSummaryResult>`
  - `isSummarizable(filename: string): boolean`
  - `DocumentSummaryResult` 类型 `{ kind: "summary" | "structure" | "skipped"; markdown: string }`

- [ ] **Step 1: 创建 `src/components/MemoEditor/services/documentSummaryService.ts`**

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

- [ ] **Step 2: 从 services/index.ts 导出**

编辑 `src/components/MemoEditor/services/index.ts`，在末尾追加：

```typescript
export * from "./documentSummaryService";
```

完整内容：

```typescript
export * from "./cacheService";
export * from "./documentSummaryService";
export * from "./errorService";
export * from "./memoService";
export * from "./transcriptionService";
export * from "./uploadService";
export * from "./validationService";
```

- [ ] **Step 3: 验证 TypeScript 编译**

Run: `npm run tauri -- ls` 或 `npx tsc --noEmit`（若项目有该脚本；否则用 `npm run build` 中的 tsc 步骤）。具体命令：

Run: `npx tsc --noEmit`
Expected: 无新增错误（`documentSummaryService` 在 Task 9 才会被使用，此时可能有"未使用"提示但不报错）。

- [ ] **Step 4: Commit**

```bash
git add src/components/MemoEditor/services/documentSummaryService.ts src/components/MemoEditor/services/index.ts
git commit -m "feat(editor): add documentSummaryService frontend wrapper"
```

---

## Task 6: EditorController 增加 appendMarkdown

**Files:**
- Modify: `src/components/MemoEditor/types/editorController.ts`
- Modify: `src/components/MemoEditor/Editor/controller.ts`

**Interfaces:**
- Produces: `EditorController.appendMarkdown(markdown: string): void` — 在文档末尾追加 markdown 文本作为独立块，并滚动到新内容。

- [ ] **Step 1: 在 EditorController 接口增加方法声明**

编辑 `src/components/MemoEditor/types/editorController.ts`，在 `insertMarkdown` 声明之后追加：

```typescript
  /** Append markdown at the end of the document as its own block. */
  appendMarkdown(markdown: string): void;
```

完整接口片段应为：

```typescript
  /** Insert markdown at the cursor as its own block. */
  insertMarkdown(markdown: string): void;
  /** Append markdown at the end of the document as its own block. */
  appendMarkdown(markdown: string): void;
  scrollToCursor(): void;
```

- [ ] **Step 2: 在 controller.ts 实现 appendMarkdown**

编辑 `src/components/MemoEditor/Editor/controller.ts`，在 `insertMarkdown` 实现之后、`scrollToCursor` 之前追加 `appendMarkdown` 方法。它复用 `blockPad` 确保块分隔，并把光标移到文档末尾：

```typescript
    appendMarkdown: (markdown) => {
      if (!markdown) return;
      const docLen = view.state.doc.length;
      const doc = view.state.doc.toString();
      const { prefix, suffix } = blockPad(doc, "");
      const insert = prefix + markdown + suffix;
      const caret = docLen + insert.length;
      view.dispatch({
        changes: { from: docLen, to: docLen, insert },
        selection: { anchor: caret },
        scrollIntoView: true,
      });
      view.focus();
    },
```

完整 controller 应为：

```typescript
import { EditorSelection, type EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import type { EditorController, FormattingController } from "../types/editorController";

const isEmptyDoc = (state: EditorState) => state.doc.toString().trim() === "";

/** Block padding for insertMarkdown: ensure the inserted text is its own block. */
function blockPad(before: string, after: string): { prefix: string; suffix: string } {
  const prefix = before.length === 0 || before.endsWith("\n\n") ? "" : before.endsWith("\n") ? "\n" : "\n\n";
  const suffix = after.length === 0 || after.startsWith("\n\n") ? "" : after.startsWith("\n") ? "\n" : "\n\n";
  return { prefix, suffix };
}

export function createController(view: EditorView, formatting: FormattingController): EditorController {
  return {
    focus: () => view.focus(),
    hasFocus: () => view.hasFocus,
    isEmpty: () => isEmptyDoc(view.state),
    getMarkdown: () => view.state.doc.toString(),
    setMarkdown: (markdown) => {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: markdown } });
    },
    insertMarkdown: (markdown) => {
      if (!markdown) return;
      const { from, to } = view.state.selection.main;
      const doc = view.state.doc.toString();
      const { prefix, suffix } = blockPad(doc.slice(0, from), doc.slice(to));
      const insert = prefix + markdown + suffix;
      const caret = from + insert.length;
      view.dispatch({ changes: { from, to, insert }, selection: { anchor: caret }, scrollIntoView: true });
      view.focus();
    },
    appendMarkdown: (markdown) => {
      if (!markdown) return;
      const docLen = view.state.doc.length;
      const doc = view.state.doc.toString();
      const { prefix, suffix } = blockPad(doc, "");
      const insert = prefix + markdown + suffix;
      const caret = docLen + insert.length;
      view.dispatch({
        changes: { from: docLen, to: docLen, insert },
        selection: { anchor: caret },
        scrollIntoView: true,
      });
      view.focus();
    },
    scrollToCursor: () => view.dispatch({ effects: EditorView.scrollIntoView(view.state.selection.main.head) }),
    selectAll: () => view.dispatch({ selection: EditorSelection.range(0, view.state.doc.length) }),
    formatting,
  };
}
```

- [ ] **Step 3: 验证 TypeScript 编译**

Run: `npx tsc --noEmit`
Expected: 无新增错误。

- [ ] **Step 4: Commit**

```bash
git add src/components/MemoEditor/types/editorController.ts src/components/MemoEditor/Editor/controller.ts
git commit -m "feat(editor): add appendMarkdown to EditorController"
```

---

## Task 7: 新增 i18n keys

**Files:**
- Modify: `src/locales/zh-Hans.json`
- Modify: `src/locales/en.json`

**Interfaces:**
- Produces: i18n keys `editor.summary.toggle` / `editor.summary.toggle-hint` / `editor.summary.generating` / `editor.summary.done` / `editor.summary.failed`。

- [ ] **Step 1: 找到 zh-Hans.json 中 editor.auto-tag 区块定位**

Run: 用 Grep 在 `src/locales/zh-Hans.json` 搜索 `"auto-tag"`，找到 editor 对象内 auto-tag 子对象的位置，确定插入 summary 子对象的位置（紧随其后）。

- [ ] **Step 2: 在 zh-Hans.json 的 editor 对象内追加 summary 子对象**

用 Edit 工具，在 editor.auto-tag 子对象结束 `}` 之后（同级）追加：

```json
    "summary": {
      "toggle": "添加文档附件时生成摘要",
      "toggle-hint": "对 PDF/DOCX/PPT/ZIP 等自动提取内容追加到笔记末尾",
      "generating": "正在为 {{name}} 生成摘要…",
      "done": "已生成摘要并追加到笔记末尾",
      "failed": "为 {{name}} 生成摘要失败：{{reason}}"
    },
```

注意 JSON 逗号：若 auto-tag 子对象后原本是 `}`（editor 对象结束），需在 summary 前补逗号；若 auto-tag 后还有其它同级 key，则 summary 后需补逗号。**实际编辑时根据上下文调整**。

- [ ] **Step 3: 在 en.json 对应位置追加英文翻译**

```json
    "summary": {
      "toggle": "Summarize document attachments",
      "toggle-hint": "Auto-extract content from PDF/DOCX/PPT/ZIP and append to memo",
      "generating": "Generating summary for {{name}}…",
      "done": "Summary generated and appended to memo",
      "failed": "Failed to summarize {{name}}: {{reason}}"
    },
```

- [ ] **Step 4: 验证 JSON 合法性**

Run: `node -e "JSON.parse(require('fs').readFileSync('src/locales/zh-Hans.json','utf8')); JSON.parse(require('fs').readFileSync('src/locales/en.json','utf8')); console.log('OK')"`
Expected: 输出 `OK`。若报错，修正 JSON 语法。

- [ ] **Step 5: Commit**

```bash
git add src/locales/zh-Hans.json src/locales/en.json
git commit -m "i18n(editor): add summary feature translations"
```

---

## Task 8: 工具栏开关按钮 + 常量 + 类型

**Files:**
- Modify: `src/components/MemoEditor/constants.ts`
- Modify: `src/components/MemoEditor/types/components.ts`
- Modify: `src/components/MemoEditor/Toolbar/EditorToolbar.tsx`

**Interfaces:**
- Produces:
  - `SUMMARY_STORAGE_KEY` 常量
  - `EditorToolbarProps.summaryEnabled: boolean` / `onToggleSummary: () => void`
  - EditorToolbar 渲染一个 Switch + label 切换按钮（仿 autoTag）

- [ ] **Step 1: 在 constants.ts 增加 SUMMARY_STORAGE_KEY**

编辑 `src/components/MemoEditor/constants.ts`，在 `AUTO_TAG_STORAGE_KEY` 之后追加：

```typescript
// localStorage key for the document-summary toggle. Defaults to off.
export const SUMMARY_STORAGE_KEY = "memos-editor-summary";
```

- [ ] **Step 2: 在 EditorToolbarProps 增加 summary 字段**

编辑 `src/components/MemoEditor/types/components.ts`，在 `EditorToolbarProps` 接口的 `onToggleAutoTag: () => void;` 之后追加：

```typescript
  /** Whether document-summary is enabled (persisted preference). */
  summaryEnabled: boolean;
  onToggleSummary: () => void;
```

- [ ] **Step 3: 在 EditorToolbar.tsx 渲染 summary 开关**

编辑 `src/components/MemoEditor/Toolbar/EditorToolbar.tsx`：

3a. 在 props 解构中增加 `summaryEnabled, onToggleSummary`：

```typescript
export const EditorToolbar: FC<EditorToolbarProps> = ({
  onSave,
  onCancel,
  memoName,
  onAudioRecorderClick,
  isFormattingToolbarVisible,
  onToggleFormattingToolbar,
  autoTagEnabled,
  onToggleAutoTag,
  summaryEnabled,
  onToggleSummary,
}) => {
```

3b. 在 autoTag 的 `<label>...</label>` 之后追加 summary 的 `<label>`：

```tsx
        <label className="flex items-center gap-1.5 px-2 cursor-pointer select-none text-sm text-muted-foreground">
          <Switch checked={summaryEnabled} onCheckedChange={onToggleSummary} />
          <span>{t("editor.summary.toggle")}</span>
        </label>
```

完整 JSX 片段：

```tsx
      <div className="flex flex-row justify-start items-center gap-1">
        <InsertMenu
          isUploading={isUploading}
          location={location}
          onLocationChange={handleLocationChange}
          onToggleFocusMode={handleToggleFocusMode}
          memoName={memoName}
          onAudioRecorderClick={onAudioRecorderClick}
          isFormattingToolbarVisible={isFormattingToolbarVisible}
          onToggleFormattingToolbar={onToggleFormattingToolbar}
        />
        <VisibilitySelector value={visibility} onChange={handleVisibilityChange} />
        <label className="flex items-center gap-1.5 px-2 cursor-pointer select-none text-sm text-muted-foreground">
          <Switch checked={autoTagEnabled} onCheckedChange={onToggleAutoTag} />
          <span>{t("editor.auto-tag.toggle")}</span>
        </label>
        <label className="flex items-center gap-1.5 px-2 cursor-pointer select-none text-sm text-muted-foreground">
          <Switch checked={summaryEnabled} onCheckedChange={onToggleSummary} />
          <span>{t("editor.summary.toggle")}</span>
        </label>
      </div>
```

- [ ] **Step 4: 验证 TypeScript 编译**

Run: `npx tsc --noEmit`
Expected: 报错"缺少 summaryEnabled / onToggleSummary prop"（来自 `MemoEditorImpl` 中的 `<EditorToolbar>` 调用）—— 这是预期的，Task 9 会修复。若其它错误则修正。

- [ ] **Step 5: Commit**

```bash
git add src/components/MemoEditor/constants.ts src/components/MemoEditor/types/components.ts src/components/MemoEditor/Toolbar/EditorToolbar.tsx
git commit -m "feat(editor): add summary toggle to EditorToolbar"
```

---

## Task 9: 串联 handleFileAdded + 透传 onFileAdded prop

**Files:**
- Modify: `src/components/MemoEditor/types/components.ts`（`EditorContentProps`、`InsertMenuProps`）
- Modify: `src/components/MemoEditor/components/EditorContent.tsx`
- Modify: `src/components/MemoEditor/Toolbar/InsertMenu.tsx`
- Modify: `src/components/MemoEditor/index.tsx`

**Interfaces:**
- Produces:
  - `EditorContentProps.onFileAdded?: (file: LocalFile) => void`
  - `InsertMenuProps.onFileAdded?: (file: LocalFile) => void`
  - `MemoEditorImpl` 定义 `handleFileAdded(localFile: LocalFile): void`，先 `dispatch(addLocalFile)`，再按 `summaryEnabled` 异步调摘要。

- [ ] **Step 1: 在 EditorContentProps 增加 onFileAdded**

编辑 `src/components/MemoEditor/types/components.ts`，在 `EditorContentProps` 接口追加：

```typescript
export interface EditorContentProps {
  placeholder?: string;
  /** Invoked by the in-editor save shortcut (Cmd/Ctrl+Enter). */
  onSubmit: () => void;
  /** Called when a file is added via drag-drop or paste. If omitted, falls
   *  back to dispatching addLocalFile directly. */
  onFileAdded?: (file: LocalFile) => void;
}
```

需要在文件顶部导入 `LocalFile` 类型。检查现有导入——该文件目前未导入 LocalFile，需追加：

```typescript
import type { LocalFile } from "../types/attachment";
```

（若已存在则跳过。）

- [ ] **Step 2: 在 InsertMenuProps 增加 onFileAdded**

在同一文件 `InsertMenuProps` 接口追加：

```typescript
export interface InsertMenuProps {
  isUploading?: boolean;
  location?: Location;
  onLocationChange: (location?: Location) => void;
  onToggleFocusMode?: () => void;
  memoName?: string;
  onAudioRecorderClick?: () => void;
  isFormattingToolbarVisible?: boolean;
  onToggleFormattingToolbar?: () => void;
  /** Called when a file is added via the file input. If omitted, falls back
   *  to dispatching addLocalFile directly. */
  onFileAdded?: (file: LocalFile) => void;
}
```

- [ ] **Step 3: 修改 EditorContent.tsx 使用 onFileAdded**

编辑 `src/components/MemoEditor/components/EditorContent.tsx`：

3a. 在组件 props 解构增加 `onFileAdded`：

```typescript
export const EditorContent = forwardRef<EditorController, EditorContentProps>(({ placeholder, onSubmit, onFileAdded }, ref) => {
```

3b. 修改 drag-drop 回调，优先调用 onFileAdded：

```typescript
  const { dragHandlers } = useDragAndDrop((files: FileList) => {
    const localFiles: LocalFile[] = Array.from(files).map((file) => ({
      file,
      previewUrl: createBlobUrl(file),
      origin: "upload",
    }));
    localFiles.forEach((localFile) => {
      if (onFileAdded) onFileAdded(localFile);
      else dispatch(actions.addLocalFile(localFile));
    });
  });
```

3c. 修改 paste 回调：

```typescript
    const localFiles: LocalFile[] = files.map((file) => ({
      file,
      previewUrl: createBlobUrl(file),
      origin: "upload",
    }));
    localFiles.forEach((localFile) => {
      if (onFileAdded) onFileAdded(localFile);
      else dispatch(actions.addLocalFile(localFile));
    });
    event.preventDefault();
```

- [ ] **Step 4: 修改 InsertMenu.tsx 使用 onFileAdded**

编辑 `src/components/MemoEditor/Toolbar/InsertMenu.tsx`：

4a. 在 props 解构增加 `onFileAdded`：

```typescript
  const {
    location: initialLocation,
    onLocationChange,
    onToggleFocusMode,
    onToggleFormattingToolbar,
    isFormattingToolbarVisible,
    isUploading: isUploadingProp,
    onFileAdded,
  } = props;
```

4b. 修改 useFileUpload 回调：

```typescript
  const { fileInputRef, selectingFlag, handleFileInputChange, handleUploadClick } = useFileUpload((newFiles: LocalFile[]) => {
    newFiles.forEach((file) => {
      if (onFileAdded) onFileAdded(file);
      else dispatch(actions.addLocalFile(file));
    });
  });
```

- [ ] **Step 5: 在 MemoEditorImpl 增加 summaryEnabled state 与 handleFileAdded**

编辑 `src/components/MemoEditor/index.tsx`：

5a. 在顶部导入区追加：

```typescript
import { documentSummaryService, isSummarizable } from "./services";
import { SUMMARY_STORAGE_KEY } from "./constants";
```

（`useLocalStorage` 已在文件中导入，复用。）

5b. 在 `const [autoTagEnabled, setAutoTagEnabled] = useLocalStorage(AUTO_TAG_STORAGE_KEY, false);` 之后追加：

```typescript
  const [summaryEnabled, setSummaryEnabled] = useLocalStorage(SUMMARY_STORAGE_KEY, false);
```

5c. 在 `handleToggleAutoTag` 附近增加 `handleFileAdded` 与 `handleToggleSummary`：

```typescript
  const handleFileAdded = useCallback(async (localFile: LocalFile) => {
    // 先入队，避免阻塞 UI 与附件流程
    dispatch(actions.addLocalFile(localFile));

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

  const handleToggleSummary = useCallback(() => {
    setSummaryEnabled((v) => !v);
  }, [setSummaryEnabled]);
```

5d. 修改原本直接 `dispatch(actions.addLocalFile(localFile))` 的位置（音频转写失败/为空回退、录音完成非转写模式）改调 `handleFileAdded(localFile)`。

定位以下行（约第 137、145、155、171 行）：

- `dispatch(actions.addLocalFile(localFile));` in `handleTranscribeRecordedAudio` (3 处)
- `dispatch(actions.addLocalFile(localFile));` in `audioRecorder.onRecordingComplete` (1 处)

全部替换为 `void handleFileAdded(localFile);`（注意 `handleFileAdded` 是 async，需 `void` 调用）。若在 `useCallback` 依赖数组中需加入 `handleFileAdded`，则同步更新。

具体替换：

第 137 行附近（`handleTranscribeRecordedAudio` 中 `if (!canTranscribe)` 分支）：
```typescript
      if (!canTranscribe) {
        void handleFileAdded(localFile);
        setIsTranscribingAudio(false);
        setIsAudioRecorderOpen(false);
        return;
      }
```

第 145 行附近（转写为空分支）：
```typescript
        if (!text) {
          void handleFileAdded(localFile);
          toast.error(t("editor.audio-recorder.transcribe-empty"));
          return;
        }
```

第 155 行附近（转写异常分支）：
```typescript
      } catch (error) {
        console.error(error);
        toast.error(errorService.getErrorMessage(error) || t("editor.audio-recorder.transcribe-error"));
        void handleFileAdded(localFile);
      } finally {
```

第 171 行附近（`onRecordingComplete` 非转写模式）：
```typescript
    onRecordingComplete: (localFile, mode) => {
      if (mode === "transcribe") {
        void handleTranscribeRecordedAudio(localFile);
        return;
      }

      void handleFileAdded(localFile);
      setIsAudioRecorderOpen(false);
    },
```

注意：`handleTranscribeRecordedAudio` 内部原本也直接 dispatch，现在改为调 `handleFileAdded`——但 `handleTranscribeRecordedAudio` 在 `handleFileAdded` 之后定义会导致 TDZ。**解决**：把 `handleFileAdded` 定义在 `handleTranscribeRecordedAudio` 之前。检查文件中两者顺序，必要时调整声明顺序。

5e. 修改 `<EditorContent>` 与 `<EditorToolbar>` 的 props：

```tsx
        <EditorContent ref={editorRef} placeholder={placeholder} onSubmit={handleSave} onFileAdded={handleFileAdded} />
```

```tsx
        <EditorToolbar
          onSave={handleSave}
          onCancel={onCancel}
          memoName={memoName}
          onAudioRecorderClick={handleAudioRecorderClick}
          isFormattingToolbarVisible={isFormattingToolbarVisible}
          onToggleFormattingToolbar={handleToggleFormattingToolbar}
          autoTagEnabled={autoTagEnabled}
          onToggleAutoTag={handleToggleAutoTag}
          summaryEnabled={summaryEnabled}
          onToggleSummary={handleToggleSummary}
        />
```

5f. `InsertMenu` 接收的 `onFileAdded` 需要从 `EditorToolbar` 透传。检查 `EditorToolbar.tsx` 当前是否把额外 props 传给 `InsertMenu`——目前不传。**方案**：在 `EditorToolbarProps` 已加 `onFileAdded`（Step 2），现在 `EditorToolbar` 需把它透传给 `InsertMenu`。

编辑 `src/components/MemoEditor/Toolbar/EditorToolbar.tsx`，在 props 解构增加 `onFileAdded`：

```typescript
export const EditorToolbar: FC<EditorToolbarProps> = ({
  onSave,
  onCancel,
  memoName,
  onAudioRecorderClick,
  isFormattingToolbarVisible,
  onToggleFormattingToolbar,
  autoTagEnabled,
  onToggleAutoTag,
  summaryEnabled,
  onToggleSummary,
  onFileAdded,
}) => {
```

并在 `<InsertMenu>` 调用处增加 `onFileAdded={onFileAdded}`：

```tsx
        <InsertMenu
          isUploading={isUploading}
          location={location}
          onLocationChange={handleLocationChange}
          onToggleFocusMode={handleToggleFocusMode}
          memoName={memoName}
          onAudioRecorderClick={onAudioRecorderClick}
          isFormattingToolbarVisible={isFormattingToolbarVisible}
          onToggleFormattingToolbar={onToggleFormattingToolbar}
          onFileAdded={onFileAdded}
        />
```

同时更新 `EditorToolbarProps` 接口（在 `types/components.ts`）增加 `onFileAdded?: (file: LocalFile) => void;`（Step 1/2 已对 EditorContentProps 与 InsertMenuProps 加过，这里对 EditorToolbarProps 也加）。**修正 Step 2**：`EditorToolbarProps` 也需要 `onFileAdded` 字段。

回到 `types/components.ts`，在 `EditorToolbarProps` 追加：

```typescript
  /** Passed through to InsertMenu for file-add interception. */
  onFileAdded?: (file: LocalFile) => void;
```

5g. `<EditorToolbar>` 调用处（`MemoEditorImpl` 中）也需传入 `onFileAdded={handleFileAdded}`：

```tsx
        <EditorToolbar
          onSave={handleSave}
          onCancel={onCancel}
          memoName={memoName}
          onAudioRecorderClick={handleAudioRecorderClick}
          isFormattingToolbarVisible={isFormattingToolbarVisible}
          onToggleFormattingToolbar={handleToggleFormattingToolbar}
          autoTagEnabled={autoTagEnabled}
          onToggleAutoTag={handleToggleAutoTag}
          summaryEnabled={summaryEnabled}
          onToggleSummary={handleToggleSummary}
          onFileAdded={handleFileAdded}
        />
```

- [ ] **Step 6: 验证 TypeScript 编译**

Run: `npx tsc --noEmit`
Expected: 无错误。若有 "X is not defined" / "missing prop" 错误，按错误信息修正。

- [ ] **Step 7: Commit**

```bash
git add src/components/MemoEditor/types/components.ts src/components/MemoEditor/components/EditorContent.tsx src/components/MemoEditor/Toolbar/InsertMenu.tsx src/components/MemoEditor/Toolbar/EditorToolbar.tsx src/components/MemoEditor/index.tsx
git commit -m "feat(editor): wire document summary on file add with toolbar toggle"
```

---

## Task 10: 手动验证 + 最终清理

**Files:**
- 无代码改动（仅运行验证）

- [ ] **Step 1: 完整编译检查**

Run: `cd src-tauri && cargo check`
Expected: 通过，无错误。

Run: `npx tsc --noEmit`
Expected: 通过，无错误。

- [ ] **Step 2: 运行所有 Rust 单元测试**

Run: `cd src-tauri && cargo test --lib`
Expected: 全部通过，包括 `document_summary::tests::extract_ext_*`、`provider::tests::*`。

- [ ] **Step 3: 启动开发模式**

Run: `npm run tauri dev`
Expected: 应用启动，无控制台错误。

- [ ] **Step 4: 验证开关默认关闭行为**

在编辑器中：
1. 确认工具栏出现两个 Switch：auto-tag 与 summary，均默认关闭。
2. 点击文件上传按钮，选择一个 PDF 文件。
3. 预期：附件正常入队，笔记内容**无变化**（无摘要追加）。

- [ ] **Step 5: 验证开关开启 + PDF 摘要**

1. 打开 summary 开关。
2. 上传一个 PDF 文件。
3. 预期：
   - toast 显示"正在为 xxx.pdf 生成摘要…"
   - 数秒后 toast 变为"已生成摘要并追加到笔记末尾"
   - 笔记末尾出现 `## 📄 xxx.pdf 摘要` + 摘要正文
4. 若未配置 AI provider，预期 toast 显示"未配置 AI provider"错误，附件仍入队，笔记无追加。

- [ ] **Step 6: 验证 ZIP 文件结构**

1. summary 开关保持开启。
2. 上传一个 .zip 文件。
3. 预期：笔记末尾出现 `## 🗜️ xxx.zip 文件结构` + 代码块包裹的文件列表。

- [ ] **Step 7: 验证不支持类型被跳过**

1. summary 开关保持开启。
2. 上传一张图片或一个 .txt 文件。
3. 预期：附件正常入队，笔记无变化，无 toast（静默跳过）。

- [ ] **Step 8: 验证拖拽与粘贴入口**

1. summary 开关保持开启。
2. 拖拽一个 PDF 到编辑器。
3. 预期：同 Step 5，生成摘要。
4. 复制一个 PDF 文件粘贴到编辑器。
5. 预期：同 Step 5。

- [ ] **Step 9: 验证开关持久化**

1. 打开 summary 开关。
2. 刷新页面（或重启应用）。
3. 预期：summary 开关保持开启状态。

- [ ] **Step 10: 验证 autoTag 仍正常工作**

1. 同时打开 autoTag 与 summary 开关。
2. 保存一条带 PDF 附件的笔记。
3. 预期：保存时 autoTag 弹出标签建议对话框，功能不受 summary 影响。

- [ ] **Step 11: 最终提交（若有清理改动）**

若以上步骤发现任何小问题已修正，提交：

```bash
git add -A
git commit -m "chore: final verification fixes for document summary feature"
```

若无改动，跳过此步。

---

## Self-Review 记录

**1. Spec coverage 检查**:
- R1（添加文档时即时生成摘要）→ Task 9 handleFileAdded + Task 4 后端命令 ✓
- R2（ZIP 提取文件结构）→ Task 4 后端 is_zip 分支 ✓
- R3（开关默认关闭，工具栏+持久化）→ Task 8 + Task 9 useLocalStorage ✓
- R4（上传/拖拽/粘贴统一入口）→ Task 9 onFileAdded 透传到 EditorContent/InsertMenu ✓
- R5（LLM 失败跳过+toast）→ Task 4 返回 Err + Task 9 catch toast.error ✓
- R6（markitdown 失败静默跳过）→ Task 4 返回 kind="skipped" + Task 9 前端 dismiss toast ✓
- R7（截断 6000 字符）→ Task 4 MAX_CHARS ✓
- N1（无 DB schema 变更）→ 全程无迁移文件 ✓
- N2（后端命令无状态）→ Task 4 命令接收 blob+filename ✓
- N3（不破坏现有流程）→ Task 9 onFileAdded 可选，fallback 到原 dispatch ✓
- N4（无版本冲突）→ Task 1 cargo check 验证 ✓
- 共享 LLM helper → Task 2 + Task 3 重构 suggest_tags ✓
- EditorController.appendMarkdown → Task 6 ✓
- i18n → Task 7 ✓

**2. Placeholder 扫描**: 无 TBD/TODO/"implement later"，所有代码块完整。

**3. 类型一致性**:
- `DocumentSummaryResult` 在 Task 4（Rust）与 Task 5（TS）字段名一致：`kind` / `markdown` ✓
- `call_first_provider(store, system_prompt, user_message)` 签名在 Task 2 定义、Task 3 与 Task 4 调用一致 ✓
- `EditorController.appendMarkdown(markdown: string)` 在 Task 6 接口与实现一致 ✓
- `onFileAdded?: (file: LocalFile) => void` 在 EditorContentProps / InsertMenuProps / EditorToolbarProps 一致 ✓
- `SUMMARY_STORAGE_KEY` 在 Task 8 定义、Task 9 使用一致 ✓

**4. 潜在问题**:
- Task 9 Step 5d/5e 中 `handleFileAdded` 是 async，在同步回调中用 `void` 调用——已注明。
- Task 9 Step 5d 提到 `handleTranscribeRecordedAudio` 与 `handleFileAdded` 声明顺序——已注明需调整顺序避免 TDZ。
- Task 7 JSON 逗号需根据实际上下文调整——已注明。
- markitdown 0.1.11 的 `convert_bytes` 返回类型为 `Result<Option<DocumentConverterResult>, MarkitdownError>`——Task 4 代码已用 `let Some(conversion_result) = result else {...}` 处理。**需在 Task 1 cargo check 后确认 API 完全匹配**（若 API 有差异，调整 Task 4 代码）。

执行计划完成。
