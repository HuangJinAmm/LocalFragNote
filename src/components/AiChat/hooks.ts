import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import toast from "react-hot-toast";
import {
  appendMessage as persistAppendMessage,
  clearMessages as persistClearMessages,
  createSession,
  generateDefaultTitle,
  listMessages,
  recordToMessage,
} from "./chatSessionService";
import type {
  ChatMessage,
  ContentPart,
  ToolPayload,
  WireMessage,
} from "./types";

/// 保留的最近"对话轮次"数（user/assistant 文本消息），工具消息不计入此限制
const MAX_TURNS_TO_SEND = 20;

interface UseAiChatOptions {
  providerId: string | null;
}

export function useAiChat({ providerId }: UseAiChatOptions) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [currentSessionId, setCurrentSessionIdState] = useState<number | null>(null);
  /// ref 同步镜像 currentSessionId，供 useCallback 闭包读取最新值
  const sessionIdRef = useRef<number | null>(null);
  const setCurrentSessionId = useCallback((id: number | null) => {
    sessionIdRef.current = id;
    setCurrentSessionIdState(id);
  }, []);
  const currentRunId = useRef<number | null>(null);
  /// 当前轮次中已发起工具调用的 assistant 消息 id。
  /// 非 null 表示正在处理同一轮次的工具调用：后续 ai:tool 应追加到同一条 assistant，
  /// 而不是再次拆分出新的 tool_calls assistant。
  const toolCallAssistantId = useRef<string | null>(null);
  const unlistenersRef = useRef<UnlistenFn[]>([]);
  /// 待持久化的消息队列：避免流式过程中频繁写库
  /// 流式完成后一次性落库 assistant 最终内容
  const pendingUserMsgRef = useRef<ChatMessage | null>(null);
  const pendingAssistantMsgRef = useRef<ChatMessage | null>(null);

  // 设置事件监听
  useEffect(() => {
    let mounted = true;
    const unlisteners: UnlistenFn[] = [];

    const setup = async () => {
      unlisteners.push(
        await listen<{ run_id: number; text: string }>("ai:chunk", (e) => {
          if (e.payload.run_id !== currentRunId.current) return;
          setMessages((prev) => {
            const next = [...prev];
            for (let i = next.length - 1; i >= 0; i--) {
              if (next[i].role === "assistant" && next[i].streaming) {
                next[i] = { ...next[i], content: next[i].content + e.payload.text };
                break;
              }
            }
            return next;
          });
        }),
      );

      unlisteners.push(
        await listen<ToolPayload>("ai:tool", (e) => {
          if (e.payload.run_id !== currentRunId.current) return;
          const { name, args, tool_call_id, result } = e.payload;
          setMessages((prev) => {
            const next = [...prev];
            if (toolCallAssistantId.current === null) {
              // 本轮首个工具调用：把当前正在流式输出的 assistant 标记为 tool_calls 发起者
              for (let i = next.length - 1; i >= 0; i--) {
                if (next[i].role === "assistant" && next[i].streaming) {
                  next[i] = {
                    ...next[i],
                    streaming: false,
                    toolCalls: [{ id: tool_call_id, name, args }],
                  };
                  toolCallAssistantId.current = next[i].id;
                  // 落库该 assistant（已经定稿，content 可能非空）
                  void persistCurrent(next[i]);
                  break;
                }
              }
            } else {
              // 同一轮次的后续工具调用：追加到已有的 tool_calls assistant
              const idx = next.findIndex((m) => m.id === toolCallAssistantId.current);
              if (idx >= 0) {
                const prevCalls = next[idx].toolCalls ?? [];
                next[idx] = {
                  ...next[idx],
                  toolCalls: [...prevCalls, { id: tool_call_id, name, args }],
                };
                // 更新已落库的 assistant 记录（追加 tool_calls）
                void persistCurrent(next[idx], true);
              }
            }
            // 添加 tool 消息（携带结果用于下次请求回传）
            const toolMsg: ChatMessage = {
              id: crypto.randomUUID(),
              role: "tool",
              content: `🔧 ${name}(${JSON.stringify(args)})`,
              isToolCall: true,
              toolCallId: tool_call_id,
              toolResult: result,
            };
            next.push(toolMsg);
            // 落库 tool 消息
            void persistCurrent(toolMsg);
            // 确保末尾有一个流式 assistant 用于接收下一轮的文本
            const last = next[next.length - 1];
            if (!last || last.role !== "assistant" || !last.streaming) {
              const newAssistant: ChatMessage = {
                id: crypto.randomUUID(),
                role: "assistant",
                content: "",
                streaming: true,
              };
              next.push(newAssistant);
              pendingAssistantMsgRef.current = newAssistant;
            }
            return next;
          });
        }),
      );

      unlisteners.push(
        await listen<{ run_id: number }>("ai:done", (e) => {
          if (e.payload.run_id !== currentRunId.current) return;
          setMessages((prev) => {
            const next = prev
              .map((m, i) =>
                i === prev.length - 1 && m.role === "assistant"
                  ? { ...m, streaming: false }
                  : m,
              )
              // 丢弃末尾残留的空流式 assistant（工具调用后未产生文本）
              .filter((m, i, arr) => {
                if (i !== arr.length - 1) return true;
                return !(
                  m.role === "assistant" &&
                  !m.streaming &&
                  !m.isError &&
                  (typeof m.content !== "string" || m.content === "") &&
                  !m.toolCalls
                );
              });
            // 落库最终的 assistant 消息
            const finalAssistant = next[next.length - 1];
            if (finalAssistant && finalAssistant.role === "assistant") {
              void persistCurrent(finalAssistant, true);
            }
            return next;
          });
          setIsStreaming(false);
          currentRunId.current = null;
          toolCallAssistantId.current = null;
          pendingUserMsgRef.current = null;
          pendingAssistantMsgRef.current = null;
        }),
      );

      unlisteners.push(
        await listen<{ run_id: number; message: string }>("ai:error", (e) => {
          if (e.payload.run_id !== currentRunId.current) return;
          toast.error(e.payload.message);
          setMessages((prev) =>
            prev.map((m, i) => {
              if (i === prev.length - 1 && m.role === "assistant") {
                const updated = { ...m, streaming: false, isError: true };
                void persistCurrent(updated, true);
                return updated;
              }
              return m;
            }),
          );
          setIsStreaming(false);
          currentRunId.current = null;
          toolCallAssistantId.current = null;
          pendingUserMsgRef.current = null;
          pendingAssistantMsgRef.current = null;
        }),
      );

      if (mounted) {
        unlistenersRef.current = unlisteners;
      } else {
        unlisteners.forEach((fn) => fn());
      }
    };

    setup();

    return () => {
      mounted = false;
      unlistenersRef.current.forEach((fn) => fn());
      unlistenersRef.current = [];
    };
  }, []);

  /// 把一条消息追加到当前 session 落库。
  /// 若 forceUpdate=true，则尝试更新已落库的同 id 消息（用于流式 assistant 增量更新）。
  /// 由于 ChatMessage 用前端 UUID，与后端 id 不直接对应，这里简化为：
  /// - assistant 流式消息：流式过程中不落库，完成时一次性追加
  /// - 其他消息：直接 append
  const persistCurrent = useCallback(
    async (msg: ChatMessage, isFinalAssistant: boolean = false) => {
      const sid = sessionIdRef.current;
      if (sid === null) return;
      try {
        if (isFinalAssistant && pendingAssistantMsgRef.current?.id === msg.id) {
          // 流式 assistant 已定稿：追加最终内容
          await persistAppendMessage(sid, msg);
          pendingAssistantMsgRef.current = null;
        } else if (msg.role === "user") {
          await persistAppendMessage(sid, msg);
        } else if (msg.role === "tool") {
          await persistAppendMessage(sid, msg);
        } else if (msg.role === "assistant" && msg.toolCalls && msg.toolCalls.length > 0) {
          // 带 tool_calls 的 assistant：仅在首次出现时追加，后续更新靠 tool 消息独立记录
          // 这里通过 pendingAssistantMsgRef 已被 null 标记跳过重复追加
          if (pendingAssistantMsgRef.current?.id === msg.id) {
            await persistAppendMessage(sid, msg);
            pendingAssistantMsgRef.current = null;
          }
        }
      } catch (e) {
        // 落库失败不阻塞前端展示，仅 console
        console.error("persist message failed:", e);
      }
    },
    [],
  );

  /// 将前端 ChatMessage[] 构造为符合 OpenAI 工具调用协议的 WireMessage[]：
  /// assistant(tool_calls) → tool(result) → assistant(text) 的顺序保留，
  /// 保证 tool 消息前一定有携带 tool_calls 的 assistant 消息。
  const buildWireMessages = useCallback((src: ChatMessage[]): WireMessage[] => {
    const result: WireMessage[] = [];
    for (const m of src) {
      if (m.role === "tool") {
        // 孤立的 tool 消息（前面的 assistant 被截断）：跳过以避免 API 报错
        const prev = result[result.length - 1];
        if (!prev || prev.role !== "assistant" || !(prev as WireMessage & { tool_calls?: unknown }).tool_calls) {
          continue;
        }
        result.push({
          role: "tool",
          content: JSON.stringify(m.toolResult ?? ""),
          tool_call_id: m.toolCallId ?? "",
        });
      } else if (m.role === "assistant" && m.toolCalls && m.toolCalls.length > 0) {
        // 发起工具调用的 assistant：输出 tool_calls，content 留空
        result.push({
          role: "assistant",
          content: typeof m.content === "string" ? m.content : "",
          tool_calls: m.toolCalls.map((tc) => ({
            id: tc.id,
            type: "function",
            function: {
              name: tc.name,
              arguments: JSON.stringify(tc.args ?? {}),
            },
          })),
        });
      } else {
        result.push({
          role: m.role,
          content: m.content,
        });
      }
    }
    return result;
  }, []);

  /// 切换到指定会话：加载历史消息
  const switchToSession = useCallback(async (sessionId: number | null) => {
    if (isStreaming) return;
    if (sessionId === null) {
      setCurrentSessionId(null);
      setMessages([]);
      return;
    }
    try {
      const records = await listMessages(sessionId);
      const restored = records.map(recordToMessage);
      setCurrentSessionId(sessionId);
      setMessages(restored);
    } catch (e) {
      toast.error(String(e));
    }
  }, [isStreaming]);

  /// 新建会话：如果当前 session 无消息则复用，否则创建新会话
  const newChat = useCallback(async (): Promise<number | null> => {
    if (isStreaming) return null;
    // 当前会话为空，直接复用
    if (sessionIdRef.current !== null && messages.length === 0) {
      return sessionIdRef.current;
    }
    try {
      const session = await createSession(generateDefaultTitle(), providerId);
      setCurrentSessionId(session.id);
      setMessages([]);
      return session.id;
    } catch (e) {
      toast.error(String(e));
      return null;
    }
  }, [isStreaming, messages.length, providerId]);

  const send = useCallback(
    async (content: string | ContentPart[]) => {
      if (!providerId) {
        toast.error("请先选择 Provider");
        return;
      }
      if (isStreaming) return;

      // 确保有 session：首次发送自动创建
      let sid = sessionIdRef.current;
      if (sid === null) {
        try {
          const session = await createSession(generateDefaultTitle(), providerId);
          sid = session.id;
          setCurrentSessionId(session.id);
        } catch (e) {
          toast.error(String(e));
          return;
        }
      }

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
      pendingUserMsgRef.current = userMsg;
      pendingAssistantMsgRef.current = assistantMsg;

      // 保留最近 MAX_TURNS_TO_SEND 轮文本对话（不计 tool 消息），再构造为 OpenAI 格式
      const withUser = [...messages, userMsg];
      let nonToolCount = 0;
      let startIdx = 0;
      for (let i = withUser.length - 1; i >= 0; i--) {
        if (withUser[i].role !== "tool") {
          nonToolCount++;
          if (nonToolCount > MAX_TURNS_TO_SEND) {
            startIdx = i + 1;
            break;
          }
        }
      }
      // 避免从孤立的 tool 消息开始（会破坏 OpenAI tool_calls → tool 的配对）
      while (startIdx < withUser.length && withUser[startIdx].role === "tool") {
        startIdx++;
      }
      const wireMessages = buildWireMessages(withUser.slice(startIdx));

      setMessages((prev) => [...prev, userMsg, assistantMsg]);
      setIsStreaming(true);
      toolCallAssistantId.current = null;

      // 落库 user 消息
      void persistAppendMessage(sid!, userMsg).catch((e) =>
        console.error("persist user msg failed:", e),
      );

      try {
        const runId = await invoke<number>("ai_chat", {
          providerId,
          messages: wireMessages,
        });
        currentRunId.current = runId;
      } catch (e) {
        toast.error(String(e));
        setMessages((prev) =>
          prev.map((m, i) => {
            if (i === prev.length - 1 && m.role === "assistant") {
              const updated = { ...m, streaming: false, isError: true };
              void persistAppendMessage(sid!, updated).catch(console.error);
              return updated;
            }
            return m;
          }),
        );
        setIsStreaming(false);
      }
    },
    [providerId, isStreaming, messages, buildWireMessages],
  );

  const abort = useCallback(async () => {
    if (currentRunId.current !== null) {
      await invoke("ai_abort", { runId: currentRunId.current });
      currentRunId.current = null;
      toolCallAssistantId.current = null;
      setIsStreaming(false);
      setMessages((prev) =>
        prev.map((m, i) => {
          if (i === prev.length - 1 && m.role === "assistant" && m.streaming) {
            const updated = {
              ...m,
              streaming: false,
              content: (typeof m.content === "string" ? m.content : "") + " [已中断]",
            };
            // 落库中断后的 assistant
            const sid = sessionIdRef.current;
            if (sid !== null) {
              void persistAppendMessage(sid, updated).catch(console.error);
            }
            return updated;
          }
          return m;
        }),
      );
    }
  }, []);

  const clear = useCallback(async () => {
    if (isStreaming) return;
    // 清空当前 session 的消息（若已有 session）
    const sid = sessionIdRef.current;
    if (sid !== null) {
      try {
        await persistClearMessages(sid);
      } catch (e) {
        console.error("clear messages failed:", e);
      }
    }
    setMessages([]);
    toolCallAssistantId.current = null;
    // 不创建新 session，让下次 send 时按需创建
    setCurrentSessionId(null);
  }, [isStreaming]);

  return {
    messages,
    isStreaming,
    currentSessionId,
    send,
    abort,
    clear,
    switchToSession,
    newChat,
  };
}
