# AI Chat Image Input Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 AI 聊天框添加图片输入能力，支持文件选择器和剪贴板粘贴，图片以 base64 data URL 内联到消息 content array 中发送给 AI API。

**Architecture:** 前端在 Composer 添加图片按钮 + onPaste 处理，图片转 base64 后构造 OpenAI vision content array。前后端 ChatMessage.content 从 string 改为联合类型/serde_json::Value 以支持数组格式。后端透传 content 给 AI API，无需额外处理。

**Tech Stack:** React 19, TypeScript, Tauri 2, serde_json

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `src/components/AiChat/types.ts` | Modify | 新增 ContentPart/PendingImage 类型，content 改为联合类型 |
| `src/components/AiChat/AiChatComposer.tsx` | Modify | 添加图片按钮、粘贴处理、预览区、content array 构造 |
| `src/components/AiChat/hooks.ts` | Modify | send 签名改为接受 content 联合类型 |
| `src/components/AiChat/AiChatMessages.tsx` | Modify | 用户消息渲染支持图片 |
| `src-tauri/src/commands/ai_chat.rs` | Modify | ChatMessage.content 从 String 改为 serde_json::Value |
| `src/locales/en.json` | Modify | 新增 3 个 i18n 键 |
| `src/locales/zh-Hans.json` | Modify | 新增 3 个 i18n 键 |

---

### Task 1: 后端 ChatMessage.content 改为 serde_json::Value

**Files:**
- Modify: `src-tauri/src/commands/ai_chat.rs:27-37`

- [ ] **Step 1: 修改 ChatMessage 结构体**

将 `src-tauri/src/commands/ai_chat.rs` 第 27-37 行的 `ChatMessage` 结构体中 `content` 字段从 `String` 改为 `serde_json::Value`：

修改前：
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// 助手消息的 tool_calls（OpenAI 格式）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Value>,
    /// tool 角色消息的 tool_call_id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}
```

修改后：
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    /// content 可以是字符串（纯文本）或数组（OpenAI vision content array）
    pub content: Value,
    /// 助手消息的 tool_calls（OpenAI 格式）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Value>,
    /// tool 角色消息的 tool_call_id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}
```

- [ ] **Step 2: 检查 agent_loop 中的 content 使用**

在 `agent_loop` 函数中（第 110-249 行），messages 通过 `serde_json::to_value(m)` 序列化后直接放入请求 body。由于 `content` 现在是 `Value`，序列化时会自动输出为 JSON 字符串或数组，无需修改 agent_loop 逻辑。

但需要检查 `agent_loop` 中是否有地方把 `m.content` 当作 `String` 使用（如字符串拼接、`.as_str()` 调用等）。搜索 `m.content` 或 `.content` 的使用点，确认都是透传给 JSON body。

如果有 `let content = m.content.clone()` 后当字符串用的地方，需要改为 `let content = m.content.clone()`（Value 类型，仍可放入 json!宏）。

- [ ] **Step 3: 检查 tool 消息构造**

在 `agent_loop` 中，工具执行结果会构造 tool 角色的 ChatMessage。确认这些地方的 `content` 用 `json!(result_string)` 或 `Value::String(...)` 而非裸字符串。

搜索 `agent_loop` 中所有 `ChatMessage {` 构造点，确保 `content:` 字段用 `Value::String(...)` 或 `json!(...)` 包裹。

- [ ] **Step 4: 验证编译**

Run: `cargo check -p memos-app`
Expected: 编译通过。如果有类型不匹配错误，根据错误信息修复（通常是 `.content` 需要用 `Value::String(...)` 包裹）

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/ai_chat.rs
git commit -m "refactor(backend): ChatMessage.content to serde_json::Value for vision support"
```

---

### Task 2: 前端类型定义 + i18n 键

**Files:**
- Modify: `src/components/AiChat/types.ts`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-Hans.json`

- [ ] **Step 1: 修改 types.ts**

在 `src/components/AiChat/types.ts` 中：

1. 在文件顶部（`ProviderConfig` 之前）新增 `ContentPart` 和 `PendingImage` 类型：

```typescript
/// OpenAI vision content array 中的部分
export type ContentPart =
  | { type: "text"; text: string }
  | { type: "image_url"; image_url: { url: string } };

/// 待发送的图片（前端临时状态）
export interface PendingImage {
  id: string;
  dataUrl: string;
  name: string;
  size: number;
}
```

2. 修改 `ChatMessage.content`（第 14 行）：

修改前：
```typescript
  content: string;
```

修改后：
```typescript
  content: string | ContentPart[];
```

3. 修改 `WireMessage.content`（第 26 行）：

修改前：
```typescript
  content: string;
```

修改后：
```typescript
  content: string | ContentPart[];
```

- [ ] **Step 2: 修改 en.json**

在 `src/locales/en.json` 的 `"aiChat"` 对象中（在 `"error": "Error"` 之后）新增 3 个键：

```json
    "attachImage": "Attach image",
    "imageTooLarge": "Image size must not exceed 5MB",
    "tooManyImages": "Maximum 4 images per message",
```

- [ ] **Step 3: 修改 zh-Hans.json**

在 `src/locales/zh-Hans.json` 的 `"aiChat"` 对象中对应位置新增：

```json
    "attachImage": "添加图片",
    "imageTooLarge": "图片大小不能超过 5MB",
    "tooManyImages": "最多只能添加 4 张图片",
```

- [ ] **Step 4: 验证类型检查**

Run: `npx tsc --noEmit`
Expected: 可能会有 `hooks.ts` 和 `AiChatComposer.tsx` 的类型错误（因为 send 签名和 content 类型变了），这些会在后续 Task 修复。只确认 types.ts 本身无错误。

- [ ] **Step 5: Commit**

```bash
git add src/components/AiChat/types.ts src/locales/en.json src/locales/zh-Hans.json
git commit -m "feat(aichat): add ContentPart/PendingImage types and i18n keys"
```

---

### Task 3: hooks.ts send 函数支持 content 联合类型

**Files:**
- Modify: `src/components/AiChat/hooks.ts:103-151`

- [ ] **Step 1: 修改 send 函数签名和实现**

在 `src/components/AiChat/hooks.ts` 中：

1. 修改 import（第 5 行），添加 `ContentPart`：

修改前：
```typescript
import type { ChatMessage, WireMessage } from "./types";
```

修改后：
```typescript
import type { ChatMessage, ContentPart, WireMessage } from "./types";
```

2. 修改 `send` 函数（第 103-151 行）。将签名从 `async (text: string)` 改为 `async (content: string | ContentPart[])`，并更新 userMsg 构造和 wireMessages 映射：

修改前（第 103-127 行）：
```typescript
  const send = useCallback(
    async (text: string) => {
      if (!providerId) {
        toast.error("请先选择 Provider");
        return;
      }
      if (isStreaming) return;

      const userMsg: ChatMessage = {
        id: crypto.randomUUID(),
        role: "user",
        content: text,
      };
      const assistantMsg: ChatMessage = {
        id: crypto.randomUUID(),
        role: "assistant",
        content: "",
        streaming: true,
      };

      // 构造发送给后端的消息（截断到最近 20 条）
      const wireMessages: WireMessage[] = [...messages, userMsg]
        .filter((m) => m.role !== "tool" && !m.isToolCall)
        .slice(-MAX_MESSAGES_TO_SEND)
        .map((m) => ({ role: m.role, content: m.content }));
```

修改后：
```typescript
  const send = useCallback(
    async (content: string | ContentPart[]) => {
      if (!providerId) {
        toast.error("请先选择 Provider");
        return;
      }
      if (isStreaming) return;

      const userMsg: ChatMessage = {
        id: crypto.randomUUID(),
        role: "user",
        content,
      };
      const assistantMsg: ChatMessage = {
        id: crypto.randomUUID(),
        role: "assistant",
        content: "",
        streaming: true,
      };

      // 构造发送给后端的消息（截断到最近 20 条）
      const wireMessages: WireMessage[] = [...messages, userMsg]
        .filter((m) => m.role !== "tool" && !m.isToolCall)
        .slice(-MAX_MESSAGES_TO_SEND)
        .map((m) => ({ role: m.role, content: m.content as string | ContentPart[] }));
```

- [ ] **Step 2: 验证类型检查**

Run: `npx tsc --noEmit`
Expected: `hooks.ts` 无错误。`AiChatComposer.tsx` 仍可能有错误（onSend 签名不匹配），下个 Task 修复。

- [ ] **Step 3: Commit**

```bash
git add src/components/AiChat/hooks.ts
git commit -m "refactor(aichat): send accepts string | ContentPart[]"
```

---

### Task 4: AiChatComposer 添加图片按钮 + 粘贴 + 预览

**Files:**
- Modify: `src/components/AiChat/AiChatComposer.tsx`

- [ ] **Step 1: 重写 AiChatComposer.tsx**

将整个 `src/components/AiChat/AiChatComposer.tsx` 文件替换为以下内容：

```tsx
import { PaperclipIcon, SendIcon, SquareIcon, XIcon } from "lucide-react";
import { useRef, useState } from "react";
import { useTranslate } from "@/utils/i18n";
import { cn } from "@/lib/utils";
import toast from "react-hot-toast";
import type { ContentPart, PendingImage } from "./types";

const MAX_IMAGE_SIZE = 5 * 1024 * 1024; // 5MB
const MAX_IMAGES = 4;

interface AiChatComposerProps {
  isStreaming: boolean;
  disabled: boolean;
  onSend: (content: string | ContentPart[]) => void;
  onAbort: () => void;
}

const fileToDataUrl = (file: File): Promise<string> => {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = reject;
    reader.readAsDataURL(file);
  });
};

export function AiChatComposer({ isStreaming, disabled, onSend, onAbort }: AiChatComposerProps) {
  const t = useTranslate();
  const [text, setText] = useState("");
  const [pendingImages, setPendingImages] = useState<PendingImage[]>([]);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const handleFiles = async (files: File[]) => {
    for (const file of files) {
      if (!file.type.startsWith("image/")) continue;
      if (file.size > MAX_IMAGE_SIZE) {
        toast.error(t("aiChat.imageTooLarge"));
        continue;
      }
      if (pendingImages.length >= MAX_IMAGES) {
        toast.error(t("aiChat.tooManyImages"));
        break;
      }
      const dataUrl = await fileToDataUrl(file);
      setPendingImages((prev) => [
        ...prev,
        { id: crypto.randomUUID(), dataUrl, name: file.name, size: file.size },
      ]);
    }
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const textContent = text.trim();
    if ((!textContent && pendingImages.length === 0) || isStreaming || disabled) return;

    let content: string | ContentPart[];
    if (pendingImages.length === 0) {
      content = textContent;
    } else {
      const parts: ContentPart[] = [];
      if (textContent) {
        parts.push({ type: "text", text: textContent });
      }
      for (const img of pendingImages) {
        parts.push({ type: "image_url", image_url: { url: img.dataUrl } });
      }
      content = parts;
    }

    onSend(content);
    setText("");
    setPendingImages([]);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e as unknown as React.FormEvent);
    }
  };

  const handlePaste = (e: React.ClipboardEvent) => {
    const items = e.clipboardData?.items;
    if (!items) return;
    const files: File[] = [];
    for (const item of Array.from(items)) {
      if (item.kind === "file" && item.type.startsWith("image/")) {
        const file = item.getAsFile();
        if (file) files.push(file);
      }
    }
    if (files.length > 0) {
      e.preventDefault();
      handleFiles(files);
    }
  };

  const removeImage = (id: string) => {
    setPendingImages((prev) => prev.filter((p) => p.id !== id));
  };

  const canSend = (text.trim().length > 0 || pendingImages.length > 0) && !disabled;

  return (
    <form onSubmit={handleSubmit} className="border-t border-border p-2 flex flex-col gap-2">
      {pendingImages.length > 0 && (
        <div className="flex gap-2 flex-wrap">
          {pendingImages.map((img) => (
            <div key={img.id} className="relative group">
              <img
                src={img.dataUrl}
                alt={img.name}
                className="size-16 object-cover rounded-md border border-border"
              />
              <button
                type="button"
                onClick={() => removeImage(img.id)}
                className="absolute -top-1 -right-1 size-5 rounded-full bg-destructive text-destructive-foreground flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity"
              >
                <XIcon className="size-3" />
              </button>
            </div>
          ))}
        </div>
      )}
      <div className="flex gap-2 items-end">
        <input
          ref={fileInputRef}
          type="file"
          accept="image/*"
          multiple
          className="hidden"
          onChange={(e) => {
            handleFiles(Array.from(e.target.files || []));
            if (fileInputRef.current) fileInputRef.current.value = "";
          }}
        />
        <button
          type="button"
          onClick={() => fileInputRef.current?.click()}
          disabled={disabled}
          className="shrink-0 size-9 rounded-md border border-border flex items-center justify-center hover:bg-muted disabled:opacity-50 disabled:cursor-not-allowed"
          aria-label={t("aiChat.attachImage")}
        >
          <PaperclipIcon className="size-4" />
        </button>
        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={handleKeyDown}
          onPaste={handlePaste}
          placeholder={disabled ? t("aiChat.configureFirst") : t("aiChat.inputPlaceholder")}
          disabled={disabled}
          rows={1}
          className={cn(
            "flex-1 resize-none rounded-md border border-border bg-background px-3 py-2 text-sm",
            "max-h-32 min-h-[36px] focus:outline-none focus:ring-1 focus:ring-primary",
            "disabled:cursor-not-allowed disabled:opacity-50",
          )}
          style={{ height: "auto" }}
        />
        {isStreaming ? (
          <button
            type="button"
            onClick={onAbort}
            className="shrink-0 size-9 rounded-md border border-border flex items-center justify-center hover:bg-muted"
            aria-label="Stop"
          >
            <SquareIcon className="size-4" />
          </button>
        ) : (
          <button
            type="submit"
            disabled={!canSend}
            className="shrink-0 size-9 rounded-md bg-primary text-primary-foreground flex items-center justify-center disabled:opacity-50 disabled:cursor-not-allowed hover:opacity-90"
            aria-label={t("aiChat.send")}
          >
            <SendIcon className="size-4" />
          </button>
        )}
      </div>
    </form>
  );
}
```

- [ ] **Step 2: 验证类型检查**

Run: `npx tsc --noEmit`
Expected: `AiChatComposer.tsx` 无错误。`AiChatMessages.tsx` 可能有错误（content 类型变了），下个 Task 修复。

- [ ] **Step 3: Commit**

```bash
git add src/components/AiChat/AiChatComposer.tsx
git commit -m "feat(aichat): add image button, paste handler, and preview to composer"
```

---

### Task 5: AiChatMessages 渲染用户消息图片

**Files:**
- Modify: `src/components/AiChat/AiChatMessages.tsx:63-64`

- [ ] **Step 1: 添加 ContentPart import 和 renderUserContent 函数**

在 `src/components/AiChat/AiChatMessages.tsx` 中：

1. 修改 import（第 5 行），添加 `ContentPart`：

修改前：
```typescript
import type { ChatMessage } from "./types";
```

修改后：
```typescript
import type { ChatMessage, ContentPart } from "./types";
```

2. 在 `AiChatMessages` 组件内部（`if (messages.length === 0)` 之前）添加渲染函数：

```typescript
  const renderUserContent = (content: string | ContentPart[]) => {
    if (typeof content === "string") {
      return <p className="whitespace-pre-wrap break-words">{content}</p>;
    }
    return (
      <div className="space-y-2">
        {content.map((part, i) => {
          if (part.type === "text") {
            return (
              <p key={i} className="whitespace-pre-wrap break-words">
                {part.text}
              </p>
            );
          }
          return (
            <img
              key={i}
              src={part.image_url.url}
              alt=""
              className="max-w-48 rounded-md"
            />
          );
        })}
      </div>
    );
  };
```

- [ ] **Step 2: 修改用户消息渲染**

将第 63-64 行的用户消息渲染：

修改前：
```tsx
              {isUser ? (
                <p className="whitespace-pre-wrap break-words">{msg.content}</p>
              ) : msg.content ? (
```

修改后：
```tsx
              {isUser ? (
                renderUserContent(msg.content)
              ) : typeof msg.content === "string" && msg.content ? (
```

- [ ] **Step 3: 验证类型检查**

Run: `npx tsc --noEmit`
Expected: 无新增错误（预先存在的 markdown.ts 错误可忽略）

- [ ] **Step 4: Commit**

```bash
git add src/components/AiChat/AiChatMessages.tsx
git commit -m "feat(aichat): render images in user messages"
```

---

### Task 6: 最终验证

**Files:**
- 无新增改动，仅验证

- [ ] **Step 1: 后端编译检查**

Run: `cargo check -p memos-app`
Expected: 编译通过

- [ ] **Step 2: 前端类型检查**

Run: `npx tsc --noEmit`
Expected: 无新增错误

- [ ] **Step 3: 验证完成**

确认所有 Task 已完成，无未提交的改动：

Run: `git status`
Expected: working tree clean（或仅有预先存在的无关改动）
