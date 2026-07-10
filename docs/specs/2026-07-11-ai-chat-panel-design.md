# AI 聊天面板与 Agent 后端

参考 [assistant-ui modal 示例](https://www.assistant-ui.com/examples/modal)，在界面左下角实现一个浮动 AI 聊天窗。前端支持 provider 配置与切换，后端实现一个支持工具调用的 AI agent。

## 目标

- 左下角浮动按钮 + 弹出式聊天面板，全局可用（不受路由影响）
- 支持配置多个 OpenAI 兼容 provider（OpenAI、DeepSeek、Ollama 本地等），可在面板内切换
- 后端实现 agent 循环：流式响应 + 工具调用，让 AI 能搜索/读取/创建 memo、列出标签
- 流式输出（逐 token 推送），对话历史不持久化（仅内存）

## 范围

- **Provider 协议**：仅 OpenAI 兼容（`/v1/chat/completions` + SSE 流式 + function-calling）。Ollama 通过其内置的 OpenAI 兼容端点（`/v1`）支持，无需单独适配
- **工具**：`list_memos`、`get_memo`、`create_memo`、`list_tags` 共 4 个
- **持久化**：provider 配置持久化到 `app_setting`；聊天历史不持久化，关闭面板/刷新即清空
- **不包含**：多会话管理、历史对话回顾、消息编辑重发、文件上传、多模态（图片输入）

## 架构

```
┌─────────────────────────────────────────────────────┐
│ 前端 (React)                                         │
│                                                      │
│  MainLayout                                          │
│   └── <AiChatPanel />  ← fixed left-4 bottom-4       │
│        ├── 触发按钮 (Bot icon, 圆形)                   │
│        └── 面板 (展开时)                              │
│             ├── Header: 标题 + provider 选择 + 关闭   │
│             ├── MessageList: 用户/助手消息             │
│             │    └── MemoMarkdownRenderer 渲染回复    │
│             └── Composer: 输入框 + 发送               │
│                                                      │
│  events: listen("ai:chunk"/"ai:tool"/"ai:done"/"ai:error") │
│  invoke: ai_chat(provider_id, messages) → u32 run_id │
├─────────────────────────────────────────────────────┤
│ 后端 (Rust/Tauri)                                    │
│                                                      │
│  commands/ai_chat.rs  (新增)                         │
│   ├── ai_chat() #[async command]                     │
│   │   spawn_blocking → agent_loop()                  │
│   │   循环: HTTP POST SSE → 解析 → emit → tool calls │
│   │                                                  │
│   ├── ai/  (新增模块)                                 │
│   │   ├── provider.rs: ProviderConfig, load/save     │
│   │   ├── tools.rs: Tool 定义 + 执行分发              │
│   │   └── sse.rs: SSE 流式解析                        │
│   │                                                  │
│   └── ai_abort(run_id)  # 中断当前 run               │
└─────────────────────────────────────────────────────┘
```

### 关键决策

- **UI 入口**：固定在左下角的 `AiChatPanel`，挂在 `MainLayout` 顶层（全局可用，不受路由影响）
- **后端入口**：单个 `ai_chat` 命令，接收 `provider_id` + `messages`，返回 `run_id` 用于事件匹配
- **流式机制**：Tauri `emit` 事件 `ai:chunk`/`ai:done`/`ai:error`/`ai:tool`，前端用 `run_id` 过滤
- **配置存储**：providers 配置存 `app_setting` 表，key = `ai_providers`（JSON 数组），复用现有 `get_app_setting`/`upsert_app_setting`
- **HTTP 客户端**：用 `ureq`（项目已有依赖）做 SSE 流式请求，`spawn_blocking` 内同步执行
- **工具调用**：工具直接调用 `memos_core::memo::list/get/create` 和 `markdown::extract_tags`，同进程内部调用，不走 IPC 层

## 前端组件设计

### 文件结构

```
src/components/AiChat/
├── index.tsx                    # 导出
├── AiChatPanel.tsx              # 顶层：触发按钮 + 面板切换 state
├── AiChatMessages.tsx           # 消息列表渲染
├── AiChatComposer.tsx           # 输入框 + 发送按钮
├── AiChatProviderPicker.tsx     # provider 下拉选择
├── AiChatSettings.tsx           # provider 配置弹窗（新增/编辑/删除 provider）
├── hooks.ts                     # useAiChat（消息状态、流式监听、发送）
└── types.ts                     # ChatMessage, ProviderConfig 等
```

### AiChatPanel.tsx

- `useState<boolean>(open)` 控制展开/收起
- 折叠时：左下角圆形按钮（`fixed left-4 bottom-4 z-50`，`Bot` icon from lucide-react）
- 展开时：`fixed left-4 bottom-4` 的面板，尺寸 `w-[400px] h-[560px]`，圆角 + 阴影 + 边框
- 面板内布局：`flex flex-col`
  - Header（标题 "AI 助手" + provider picker + 设置齿轮 + 关闭按钮）
  - MessageList（flex-1 overflow-y-auto）
  - Composer（底部固定）

### 消息渲染

- 用户消息：右对齐气泡，纯文本
- 助手消息：左对齐，用 `MemoMarkdownRenderer` 渲染（复用现有组件，支持代码块、列表等）
- 流式光标：助手消息末尾追加一个闪烁的 `▋` span，`ai:done` 时移除
- 工具调用提示：助手调用工具时，在消息流中插入一行灰色小字 "🔧 调用工具: list_memos("...")"，让用户看到 agent 在做什么

### useAiChat hook

```typescript
interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "tool";
  content: string;
  streaming?: boolean;        // 助手消息流式中的标记
  toolCalls?: ToolCallInfo[]; // 展示用
}

function useAiChat() {
  // messages: ChatMessage[]
  // isStreaming: boolean
  // send(text): Promise<void>
  //   1. push 用户消息
  //   2. push 空助手消息 {streaming:true}
  //   3. invoke("ai_chat", { providerId, messages })
  //   4. listen("ai:chunk") → 追加到最后一条助手消息
  //   5. listen("ai:done") → 标记 streaming=false
  //   6. listen("ai:error") → 标记 streaming=false + toast 报错
  // abort(): 停止当前 run（invoke("ai_abort", { runId })）
}
```

### Provider 配置 UI（AiChatSettings）

- 点击齿轮按钮打开 `Dialog`（复用 `ui/dialog.tsx`）
- 列出已配置的 providers，每行：name + base_url + model + 编辑/删除按钮
- "添加 Provider" 表单：name、base_url、api_key（密码框）、model
- 预设模板按钮：点击填入默认值
  - "OpenAI" → `https://api.openai.com/v1` + `gpt-4o-mini`
  - "DeepSeek" → `https://api.deepseek.com/v1` + `deepseek-chat`
  - "Ollama" → `http://localhost:11434/v1` + `qwen2.5:7b`
- 保存到 `app_setting[ai_providers]`，通过现有 `upsert_app_setting` IPC

### Provider 选择器（AiChatProviderPicker）

- `Select`（复用 `ui/select.tsx`）下拉
- 选项 = 已配置的 providers + "配置..." 跳转设置弹窗
- 当前选择存 `localStorage`（`ai_chat.active_provider`），跨会话保留

## 后端 Agent 实现

### 文件结构

```
src-tauri/src/
├── commands/
│   ├── mod.rs              # 新增 pub mod ai_chat;
│   └── ai_chat.rs          # 新增：ai_chat 命令 + agent loop + ai_abort
├── ai/                     # 新增模块
│   ├── mod.rs
│   ├── provider.rs         # ProviderConfig, load/save
│   ├── tools.rs            # Tool 定义 + 执行分发
│   └── sse.rs              # SSE 流式解析
└── main.rs                 # 注册 ai_chat, ai_abort 命令
```

### Provider 配置（`ai/provider.rs`）

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct ProviderConfig {
    pub id: String,          // uuid
    pub name: String,        // 显示名 "OpenAI" / "本地 Ollama"
    pub base_url: String,    // "https://api.openai.com/v1"
    pub api_key: String,     // Ollama 可为空
    pub model: String,       // "gpt-4o-mini"
}

// 存储在 app_setting，key = "ai_providers"
// JSON: [{ id, name, base_url, api_key, model }]
pub fn load_providers(store: &Store) -> Vec<ProviderConfig>
pub fn save_providers(store: &Store, providers: &[ProviderConfig]) -> CoreResult<()>
```

### 工具定义（`ai/tools.rs`）

```rust
/// OpenAI function-calling 格式的工具描述
pub fn tool_definitions() -> Vec<Value> {
    // 返回 4 个工具的 JSON schema:
    // - list_memos: { query: string, limit?: number }
    //     → FTS 全文搜索，limit 默认 10
    // - get_memo: { uid: string }
    //     → 返回单条 memo 详情（content + tags + 时间）
    // - create_memo: { content: string }
    //     → 创建新 memo，返回 uid
    // - list_tags: {}
    //     → 返回所有 tag 及计数
}

/// 执行工具调用，返回结果 JSON
pub fn execute_tool(
    name: &str,
    args: &Value,
    store: &Store,
) -> CoreResult<Value>
```

工具直接调用 `memos_core::memo::list/get/create` 和 `markdown::extract_tags`，不走 IPC 层，因为是同进程内部调用。

### Agent 循环（`commands/ai_chat.rs`）

```rust
#[derive(Deserialize)]
pub struct ChatMessage {
    pub role: String,        // "user" | "assistant" | "tool"
    pub content: String,
    pub tool_calls: Option<Value>,  // 助手消息的 tool_calls
    pub tool_call_id: Option<String>, // tool 角色消息
}

#[tauri::command]
pub async fn ai_chat(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    provider_id: String,
    messages: Vec<ChatMessage>,
) -> IpcResult<u32> {
    let run_id: u32 = next_run_id();  // AtomicU32 静态计数器
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        agent_loop(app_handle, run_id, provider_id, messages)
    });
    Ok(run_id)  // 立即返回，流式通过 events 推送
}

fn agent_loop(app, run_id, provider_id, messages) {
    let state = app.state::<AppState>();
    let store = state.store();
    let providers = load_providers(&store);
    let provider = providers.iter().find(|p| p.id == provider_id)
        .ok_or_else(|| IpcError::BadRequest("Provider 不存在".into()))?;
    let mut msgs = messages;

    loop {  // agent 循环，最多 5 轮防死循环
        // 1. POST {base_url}/chat/completions
        //    body: { model, messages, stream:true, tools: tool_definitions() }
        //    header: Authorization: Bearer {api_key}
        let response = ureq::post(&url).set("Authorization", ...).send(json);

        // 2. SSE 流式解析
        let mut assistant_msg = "".to_string();
        let mut tool_calls: Vec<ToolCall> = vec![];

        for line in response_lines {
            if aborted(run_id) { return; }
            parse "data: {json}":
              - choices[0].delta.content → 追加 assistant_msg
                emit("ai:chunk", { run_id, text: delta })
              - choices[0].delta.tool_calls → 累积 tool_calls
        }

        // 3. 流结束，判断是否有工具调用
        if tool_calls.is_empty() {
            emit("ai:done", { run_id });
            return;
        }

        // 4. 执行工具
        msgs.push(assistant_msg with tool_calls);
        for tc in tool_calls {
            emit("ai:tool", { run_id, name, args });  // 前端展示
            let result = execute_tool(tc.name, tc.args, &store);
            msgs.push(tool message { tool_call_id: tc.id, content: result });
        }

        // 5. 继续循环，让模型基于工具结果生成回复
    }
}
```

### 系统提示词

agent_loop 首轮自动注入 system message（不存入前端 messages）：

```
你是 LocalFragNote 的 AI 助手，帮助用户管理他们的笔记（memo）。
你可以通过工具搜索、读取、创建 memo，以及列出标签。
回答使用用户提问的语言。memo 内容是 Markdown 格式。
创建 memo 前不需要确认，直接创建并告知用户。
```

### 中断机制

```rust
// 全局 abort 标记：run_id → AtomicBool
static ABORTS: Lazy<Mutex<HashMap<u32, Arc<AtomicBool>>>> = ...;

#[tauri::command]
pub fn ai_abort(run_id: u32) -> IpcResult<()> {
    if let Some(flag) = ABORTS.lock().get(&run_id) {
        flag.store(true, Ordering::SeqCst);
    }
    Ok(())
}

// agent_loop 内每读一行 SSE 检查 flag
```

### SSE 解析（`ai/sse.rs`）

`ureq` 的 `Response::into_reader()` 返回字节流，按行读取：
- 跳过空行和 `:` 心跳行
- 解析 `data: {...}` 行为 JSON
- 遇到 `data: [DONE]` 结束
- 累积 `delta.tool_calls`（OpenAI 流式协议中 tool_calls 分多块到达，需按 `index` 拼接）

## 数据流

### 完整数据流示例

```
用户输入 "我写过关于 Rust 的笔记吗？"
  │
  ▼
useAiChat.send(text)
  ├─ push {role:"user", content:text}
  ├─ push {role:"assistant", content:"", streaming:true}
  └─ invoke("ai_chat", { providerId, messages }) → run_id
       │
       ▼ (Rust spawn_blocking)
  agent_loop round 1:
  ├─ POST /chat/completions (stream, tools)
  ├─ SSE: "我帮你查一下" → emit("ai:chunk", {run_id, "我帮你查一下"})
  ├─ SSE: tool_calls=[{list_memos, query:"Rust"}]
  ├─ emit("ai:tool", {run_id, "list_memos", {query:"Rust"}})
  └─ execute_tool → [{uid, snippet, tags}...]
       │
       ▼ (前端：MessageList 出现 "🔧 调用工具: list_memos")
  agent_loop round 2:
  ├─ POST /chat/completions (含 tool 结果)
  ├─ SSE: "你写过 3 条关于 Rust 的笔记：\n1. ..." → emit("ai:chunk", ...)
  └─ 无 tool_calls → emit("ai:done")
       │
       ▼ (前端：流式光标消失，消息完成)
```

### 前端事件监听

```typescript
useEffect(() => {
  const unlisteners: UnlistenFn[] = [];
  
  unlisteners.push(await listen("ai:chunk", (e) => {
    if (e.payload.run_id !== currentRunId) return;  // 过滤其他 run
    setMessages(prev => updateLastAssistant(prev, m => 
      m.content += e.payload.text));
  }));
  
  unlisteners.push(await listen("ai:tool", (e) => {
    if (e.payload.run_id !== currentRunId) return;
    setMessages(prev => [...prev, { 
      role:"tool", content:`🔧 ${e.payload.name}(${JSON.stringify(e.payload.args)})`,
      toolCall: true 
    }]);
  }));
  
  unlisteners.push(await listen("ai:done", (e) => {
    if (e.payload.run_id !== currentRunId) return;
    setMessages(prev => markLastStreamingFalse(prev));
    setIsStreaming(false);
  }));
  
  unlisteners.push(await listen("ai:error", (e) => {
    if (e.payload.run_id !== currentRunId) return;
    toast.error(e.payload.message);
    setIsStreaming(false);
    // 标记最后一条助手消息为错误状态
  }));
  
  return () => unlisteners.forEach(fn => fn());
}, [currentRunId]);
```

## 集成点

| 集成点 | 复用 | 说明 |
|---|---|---|
| Markdown 渲染 | `MemoMarkdownRenderer` | 助手回复渲染，已支持代码块、列表、表格 |
| UI 原语 | `ui/select.tsx`, `ui/dialog.tsx`, `ui/button.tsx` | provider picker、设置弹窗、按钮 |
| 配置存储 | `get_app_setting`/`upsert_app_setting` | key=`ai_providers` 存 provider 列表 |
| Memo 操作 | `memos_core::memo::list/get/create` | 工具直接调 core 层，不走 IPC |
| Tag 提取 | `memos_core::markdown::extract_tags` | `list_tags` 工具复用 |
| 异步模式 | `spawn_blocking` + `emit` | 与 `embed_text` 模式一致 |

### MainLayout 集成

```tsx
// MainLayout.tsx 末尾，Outlet 同级
<AiChatPanel />
```

`AiChatPanel` 用 `fixed` 定位，不参与 flex 布局，不影响现有内容。

## 工具规格

### list_memos
```json
{
  "name": "list_memos",
  "description": "搜索用户的笔记。支持全文搜索（FTS）和列出最近的笔记。",
  "parameters": {
    "type": "object",
    "properties": {
      "query": { "type": "string", "description": "全文搜索关键词，留空则返回最近笔记" },
      "limit": { "type": "number", "description": "返回数量，默认 10，最大 50" }
    }
  }
}
```
返回：`{ memos: [{ uid, snippet, tags, created_ts, updated_ts }] }`（snippet 为前 200 字符摘要，避免 token 浪费）
实现：调 `memo::list`，`fts_query` = query，`limit` 限制，用 `markdown::generate_snippet` 生成摘要

### get_memo
```json
{
  "name": "get_memo",
  "description": "获取单条笔记的完整内容。",
  "parameters": {
    "type": "object",
    "properties": {
      "uid": { "type": "string", "description": "笔记的唯一 ID" }
    },
    "required": ["uid"]
  }
}
```
返回：`{ uid, content, tags, created_ts, updated_ts, visibility, pinned }`
实现：调 `memo::get`，用 `markdown::extract_tags` 提取 tags

### create_memo
```json
{
  "name": "create_memo",
  "description": "创建一条新笔记。",
  "parameters": {
    "type": "object",
    "properties": {
      "content": { "type": "string", "description": "笔记内容，Markdown 格式" }
    },
    "required": ["content"]
  }
}
```
返回：`{ uid, id, created_ts }`
实现：调 `memo::create`，uid 自动生成，visibility=PRIVATE

### list_tags
```json
{
  "name": "list_tags",
  "description": "列出用户所有标签及其使用次数。",
  "parameters": { "type": "object", "properties": {} }
}
```
返回：`{ tags: [{ tag, count }] }`
实现：遍历 NORMAL 状态 memo，用 `markdown::extract_tags` 聚合

## 错误处理

| 场景 | 处理 |
|---|---|
| 无 provider 配置 | picker 显示"请先配置"，点开设置弹窗 |
| API key 错误 (401) | emit `ai:error`，toast "认证失败，请检查 API Key" |
| 网络不通 | emit `ai:error`，toast "无法连接到 {base_url}" |
| 模型不支持 tools | tool_calls 为空，直接返回文本回复（降级为普通聊天） |
| 用户在流式中关闭面板 | `ai_abort` 命令，agent 检测 flag 后停止读 SSE |
| tool 执行报错 | 错误信息作为 tool result 返回模型，不终止 agent |
| 超长对话 | 前端只保留最近 20 条消息发送（截断旧消息） |
| Ollama 无 API key | header 不带 Authorization（`api_key` 为空时跳过） |
| Agent 循环超过 5 轮 | emit `ai:error` "超过最大工具调用轮次" |

## 测试策略

### 后端单元测试（`#[cfg(test)]`）

- `ai/sse.rs`：用固定 SSE 字符串测试解析逻辑（content delta、tool_calls 分块、`[DONE]`）
- `ai/tools.rs`：mock Store（`:memory:` SQLite），测试 4 个工具的输入输出
- `ai/provider.rs`：测试 JSON 序列化/反序列化、load/save 往返

### 后端集成测试（可选，需真实 API key）

- 用环境变量 `AI_TEST_PROVIDER_ID` 跳过或启用
- 测试完整 agent 循环：简单问答 + 工具调用

### 前端测试

- 手动验证：provider 配置 → 对话 → 流式输出 → 工具调用展示
- TypeScript 编译检查（`tsc --noEmit`）

## 依赖变更

- **新增 Rust 依赖**：无（`ureq` 已在 `src-tauri/Cargo.toml`；`run_id` 用 `AtomicU32` 静态计数器生成，无需 `rand` crate）
- **新增 npm 依赖**：无（复用 `lucide-react`、`@radix-ui/*`、`@tauri-apps/api`）

## 需要修改的文件

### 新增

- `src/components/AiChat/` 整个目录（8 个文件）
- `src-tauri/src/ai/` 整个目录（4 个文件）
- `src-tauri/src/commands/ai_chat.rs`

### 修改

- `src-tauri/src/commands/mod.rs` — 新增 `pub mod ai_chat;`
- `src-tauri/src/main.rs` — 注册 `ai_chat`, `ai_abort` 命令
- `src/layouts/MainLayout.tsx` — 引入 `<AiChatPanel />`
- `src/locales/zh-Hans.json` 和 `en.json` — 新增 AI 聊天相关文案
