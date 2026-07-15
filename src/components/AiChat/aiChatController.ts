//! AI 聊天面板的模块级控制器
//! 允许外部组件(如 MemoActionMenu)打开 AI 面板并发送预设消息
//! AiChatPanel 在挂载时注册 open/send 回调，卸载时注销

import type { ContentPart } from "./types";

let openPanelFn: (() => void) | null = null;
let sendFn: ((content: string | ContentPart[]) => void) | null = null;

/** AiChatPanel 调用：注册打开和发送回调，返回注销函数 */
export function registerAiChat(
  open: () => void,
  send: (content: string | ContentPart[]) => void,
): () => void {
  openPanelFn = open;
  sendFn = send;
  return () => {
    openPanelFn = null;
    sendFn = null;
  };
}

/** 外部组件调用：打开 AI 面板并发送预设消息 */
export function openAiChatWithPrompt(content: string | ContentPart[]): void {
  openPanelFn?.();
  // 延迟到下一个 tick，确保面板已展开后再发送
  setTimeout(() => {
    sendFn?.(content);
  }, 0);
}
