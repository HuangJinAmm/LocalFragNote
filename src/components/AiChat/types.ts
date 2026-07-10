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
  content: string;
  /** 助手消息流式中的标记 */
  streaming?: boolean;
  /** tool 消息的展示标记 */
  isToolCall?: boolean;
  /** 错误标记 */
  isError?: boolean;
}

/// 发送给后端的消息格式
export interface WireMessage {
  role: string;
  content: string;
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
