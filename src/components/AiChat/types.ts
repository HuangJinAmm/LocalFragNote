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

/// 会话摘要（列表项）
export interface ChatSession {
  id: number;
  title: string;
  provider_id: string | null;
  created_ts: number;
  updated_ts: number;
  message_count?: number;
  preview?: string;
}

/// 后端返回的消息记录（已落库的）
export interface ChatMessageRecord {
  id: number;
  session_id: number;
  seq: number;
  role: "user" | "assistant" | "tool";
  /// JSON 字符串：string 或 ContentPart[]
  content: string;
  /// JSON 字符串：assistant 的 tool_calls 数组
  tool_calls: string | null;
  tool_call_id: string | null;
  /// JSON 字符串：tool 执行结果
  tool_result: string | null;
  is_error: boolean;
  created_ts: number;
}

/// AI Provider 配置
export interface ProviderConfig {
  id: string;
  name: string;
  base_url: string;
  api_key: string;
  model: string;
}

/// 聊天消息（前端状态）
export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "tool";
  content: string | ContentPart[];
  /** 助手消息流式中的标记 */
  streaming?: boolean;
  /** tool 消息的展示标记 */
  isToolCall?: boolean;
  /** 错误标记 */
  isError?: boolean;
  /** tool 消息的关联 id（OpenAI tool_call_id） */
  toolCallId?: string;
  /** 助手消息触发的工具调用列表（用于回传给模型） */
  toolCalls?: ToolCallInfo[];
  /** tool 消息携带的工具执行结果（用于回传给模型） */
  toolResult?: unknown;
}

/// 助手消息中的工具调用信息
export interface ToolCallInfo {
  id: string;
  name: string;
  args: unknown;
}

/// 发送给后端的消息格式
export interface WireMessage {
  role: string;
  content: string | ContentPart[];
  tool_calls?: unknown;
  tool_call_id?: string;
}

/// Tauri 事件 payload
export interface ChunkPayload {
  run_id: number;
  text: string;
}

export interface ToolPayload {
  run_id: number;
  name: string;
  args: unknown;
  tool_call_id: string;
  result: unknown;
}

export interface DonePayload {
  run_id: number;
}

export interface ErrorPayload {
  run_id: number;
  message: string;
}

/// Provider 预设模板
export interface ProviderPreset {
  label: string;
  name: string;
  base_url: string;
  model: string;
}

export const PROVIDER_PRESETS: ProviderPreset[] = [
  { label: "OpenAI", name: "OpenAI", base_url: "https://api.openai.com/v1", model: "gpt-4o-mini" },
  { label: "DeepSeek", name: "DeepSeek", base_url: "https://api.deepseek.com/v1", model: "deepseek-chat" },
  { label: "Ollama", name: "本地 Ollama", base_url: "http://localhost:11434/v1", model: "qwen2.5:7b" },
];
