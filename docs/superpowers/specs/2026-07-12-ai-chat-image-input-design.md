# AI Chat Image Input Design

> **Date**: 2026-07-12
> **Status**: Approved
> **Goal**: 为 AI 聊天框添加图片输入能力，支持文件选择器和剪贴板粘贴，图片以 base64 data URL 内联到消息中发送给 AI API。

## 1. 背景与问题

当前 AI 聊天面板（`AiChatPanel`）仅支持纯文本对话。`ChatMessage.content` 是字符串，无法发送图片。用户希望能在聊天中发送图片给 AI（如截图分析、图片问答）。

现有架构：
- 前端 `AiChatComposer` 用 `<textarea>` 输入，无附件/粘贴/拖拽处理
- 后端 `agent_loop` 构造 OpenAI 兼容的 `/chat/completions` 请求，`content` 直接作为字符串塞入
- 聊天历史不持久化（纯内存），关闭面板即清空
- MemoEditor 已有完整的文件上传/粘贴机制可参考

## 2. 设计决策

### 2.1 图片来源：文件选择器 + 粘贴

- **文件选择器**：Composer 工具栏新增图片按钮（PaperclipIcon），点击触发 `<input type="file" accept="image/*" multiple>`
- **粘贴**：在 Composer 区域监听 `onPaste`，从剪贴板提取图片文件
- 不做拖拽（YAGNI，文件选择器 + 粘贴已覆盖主要场景）

### 2.2 传输方式：Base64 内联

前端将图片转为 base64 data URL，直接嵌入消息的 content array 中。不落盘、不调用 `create_attachment`。

**理由**：聊天历史不持久化，图片随消息存在于内存即可。存储到附件系统会引入不必要的复杂度（清理、生命周期管理）。base64 内联是 OpenAI vision API 的标准格式。

### 2.3 消息格式：OpenAI vision content array

采用 OpenAI vision 的 content array 格式，向后兼容纯字符串：

```typescript
// 无图片（向后兼容）
content: "你好"

// 有图片
content: [
  { type: "text", text: "这张图片是什么？" },
  { type: "image_url", image_url: { url: "data:image/png;base64,iVBOR..." } }
]
```

### 2.4 Provider 兼容性

不新增 `supports_vision` 字段。如果 provider 不支持图片，AI API 会返回错误，前端通过 `ai:error` 事件 toast 提示。用户自行判断 provider 能力。

## 3. 数据结构

### 3.1 前端类型（`src/components/AiChat/types.ts`）

```typescript
// 新增：content 数组中的部分
export type ContentPart =
  | { type: "text"; text: string }
  | { type: "image_url"; image_url: { url: string } };

// 修改：content 从 string 改为联合类型
export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "tool";
  content: string | ContentPart[];  // ← 改这里
  streaming?: boolean;
  isToolCall?: boolean;
  isError?: boolean;
}

// 修改：WireMessage.content 同步
export interface WireMessage {
  role: string;
  content: string | ContentPart[];  // ← 改这里
  tool_calls?: unknown;
  tool_call_id?: string;
}

// 新增：待发送图片
export interface PendingImage {
  id: string;          // 前端生成的唯一 id
  dataUrl: string;     // base64 data URL
  name: string;        // 文件名
  size: number;        // 字节数
}
```

### 3.2 后端类型（`src-tauri/src/commands/ai_chat.rs`）

```rust
// 修改：content 从 String 改为 serde_json::Value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: serde_json::Value,  // ← 改这里：String → Value
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
}
```

用 `serde_json::Value` 而非枚举，因为 OpenAI API 接受字符串或数组两种格式，Value 自动透传无需自定义序列化。

## 4. 前端组件改造

### 4.1 AiChatComposer.tsx

**新增功能**：
1. 图片按钮（PaperclipIcon）— 点击触发隐藏的 `<input type="file" accept="image/*" multiple">`
2. `onPaste` 处理 — 检测剪贴板图片，提取后加入待发送列表
3. 待发送图片预览区 — 在 textarea 上方，缩略图 + 删除按钮（XIcon）
4. 发送时构造 content array
5. 发送后清空待发送图片

**状态管理**：
```typescript
const [pendingImages, setPendingImages] = useState<PendingImage[]>([]);
```

**图片处理流程**：
```typescript
const handleFiles = async (files: File[]) => {
  for (const file of files) {
    if (!file.type.startsWith("image/")) continue;
    if (file.size > 5 * 1024 * 1024) {
      toast.error(t("aiChat.imageTooLarge"));
      continue;
    }
    if (pendingImages.length >= 4) {
      toast.error(t("aiChat.tooManyImages"));
      break;
    }
    const dataUrl = await fileToDataUrl(file);
    setPendingImages(prev => [...prev, {
      id: crypto.randomUUID(),
      dataUrl,
      name: file.name,
      size: file.size,
    }]);
  }
};

const fileToDataUrl = (file: File): Promise<string> => {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = reject;
    reader.readAsDataURL(file);
  });
};
```

**发送逻辑**：
```typescript
const handleSubmit = (e: React.FormEvent) => {
  e.preventDefault();
  const textContent = text.trim();
  if (!textContent && pendingImages.length === 0) return;

  // 构造 content
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
```

**onSend 签名变更**：`onSend: (content: string | ContentPart[]) => void`（原来是 `(text: string) => void`）

**JSX 结构**（在 textarea 上方加预览区）：
```tsx
<form onSubmit={handleSubmit} className="border-t border-border p-2 flex flex-col gap-2">
  {/* 待发送图片预览 */}
  {pendingImages.length > 0 && (
    <div className="flex gap-2 flex-wrap">
      {pendingImages.map(img => (
        <div key={img.id} className="relative group">
          <img src={img.dataUrl} alt={img.name} className="size-16 object-cover rounded-md border border-border" />
          <button
            type="button"
            onClick={() => setPendingImages(prev => prev.filter(p => p.id !== img.id))}
            className="absolute -top-1 -right-1 size-5 rounded-full bg-destructive text-destructive-foreground flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity"
          >
            <XIcon className="size-3" />
          </button>
        </div>
      ))}
    </div>
  )}
  <div className="flex gap-2 items-end">
    {/* 图片按钮 */}
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
    <textarea ... onPaste={handlePaste} />
    {/* 发送/停止按钮 */}
  </div>
</form>
```

**handlePaste**：
```typescript
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
```

### 4.2 hooks.ts

`send` 函数签名从 `send(text: string)` 改为 `send(content: string | ContentPart[])`。构造 userMsg 时直接用传入的 content，不再内部 trim。

### 4.3 AiChatMessages.tsx

用户消息渲染时，检查 content 类型：
```typescript
const renderUserContent = (content: string | ContentPart[]) => {
  if (typeof content === "string") {
    return <p className="whitespace-pre-wrap break-words">{content}</p>;
  }
  return (
    <div className="space-y-2">
      {content.map((part, i) => {
        if (part.type === "text") {
          return <p key={i} className="whitespace-pre-wrap break-words">{part.text}</p>;
        }
        return <img key={i} src={part.image_url.url} alt="" className="max-w-48 rounded-md" />;
      })}
    </div>
  );
};
```

## 5. 后端改造

### 5.1 ai_chat.rs

`ChatMessage.content` 从 `String` 改为 `serde_json::Value`。

`agent_loop` 中构造请求 body 时，原来：
```rust
messages.push(json!({
    "role": m.role,
    "content": m.content,  // String
    ...
}));
```

改为：
```rust
messages.push(json!({
    "role": m.role,
    "content": m.content,  // Value，自动序列化为数组或字符串
    ...
}));
```

`serde_json::Value` 透传，无需额外逻辑。tool_calls 消息的 content 仍为字符串（工具调用结果不含图片）。

### 5.2 系统提示词

系统提示词（`SYSTEM_PROMPT`）仍为字符串，不受影响。

## 6. 图片限制

- 只接受 `image/*` MIME 类型
- 单张最大 5MB（超出 toast 提示 `aiChat.imageTooLarge`）
- 最多 4 张图片/消息（超出 toast 提示 `aiChat.tooManyImages`）
- 不做图片压缩/缩放（YAGNI，5MB 限制已足够）

## 7. i18n 新增键

```json
{
  "aiChat": {
    "attachImage": "添加图片",
    "imageTooLarge": "图片大小不能超过 5MB",
    "tooManyImages": "最多只能添加 4 张图片"
  }
}
```

zh-Hans.json 对应中文翻译。

## 8. 不改动的部分

- **后端 SSE 解析**（`ai/sse.rs`）— 不变，仍解析文本流
- **工具调用**（`ai/tools.rs`）— 不变，工具消息仍为字符串 content
- **Provider 配置**（`ai/provider.rs`）— 不新增字段
- **附件系统**（`create_attachment` 等）— 不使用
- **聊天历史持久化** — 仍不持久化
- **AiChatSettings / AiChatProviderPicker** — 不变

## 9. 测试计划

### 手动验证

1. 文件选择器：点击图片按钮 → 选择 1 张图片 → 预览显示 → 输入文字 → 发送 → 用户消息含图片 + 文字
2. 粘贴：截图后 Ctrl+V → 预览显示 → 发送
3. 多图：连续添加 4 张图片 → 预览显示 4 个缩略图 → 发送
4. 删除预览：添加图片后点 X → 预览消失
5. 超大图片：选择 >5MB 图片 → toast 报错
6. 超出数量：添加第 5 张图片 → toast 报错
7. 纯图片无文字：只添加图片不输入文字 → 发送成功（content array 只有 image_url）
8. 纯文字无图片：不添加图片只输入文字 → 向后兼容，content 为字符串
9. 不支持 vision 的 provider：发送图片 → AI API 返回错误 → toast 显示错误
10. 中断流式：发送含图消息后点停止 → 正常中断

### 编译验证

- `cargo check -p memos-app` 通过
- `npx tsc --noEmit` 无新增错误
