import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import toast from "react-hot-toast";
import type { ChatMessage, ContentPart, WireMessage } from "./types";

const MAX_MESSAGES_TO_SEND = 20;

interface UseAiChatOptions {
  providerId: string | null;
}

export function useAiChat({ providerId }: UseAiChatOptions) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const currentRunId = useRef<number | null>(null);
  const unlistenersRef = useRef<UnlistenFn[]>([]);

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
        await listen<{ run_id: number; name: string; args: unknown }>("ai:tool", (e) => {
          if (e.payload.run_id !== currentRunId.current) return;
          setMessages((prev) => [
            ...prev,
            {
              id: crypto.randomUUID(),
              role: "tool",
              content: `🔧 ${e.payload.name}(${JSON.stringify(e.payload.args)})`,
              isToolCall: true,
            },
          ]);
        }),
      );

      unlisteners.push(
        await listen<{ run_id: number }>("ai:done", (e) => {
          if (e.payload.run_id !== currentRunId.current) return;
          setMessages((prev) =>
            prev.map((m, i) =>
              i === prev.length - 1 && m.role === "assistant"
                ? { ...m, streaming: false }
                : m,
            ),
          );
          setIsStreaming(false);
          currentRunId.current = null;
        }),
      );

      unlisteners.push(
        await listen<{ run_id: number; message: string }>("ai:error", (e) => {
          if (e.payload.run_id !== currentRunId.current) return;
          toast.error(e.payload.message);
          setMessages((prev) =>
            prev.map((m, i) =>
              i === prev.length - 1 && m.role === "assistant"
                ? { ...m, streaming: false, isError: true }
                : m,
            ),
          );
          setIsStreaming(false);
          currentRunId.current = null;
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

      setMessages((prev) => [...prev, userMsg, assistantMsg]);
      setIsStreaming(true);

      try {
        const runId = await invoke<number>("ai_chat", {
          providerId,
          messages: wireMessages,
        });
        currentRunId.current = runId;
      } catch (e) {
        toast.error(String(e));
        setMessages((prev) =>
          prev.map((m, i) =>
            i === prev.length - 1 && m.role === "assistant"
              ? { ...m, streaming: false, isError: true }
              : m,
          ),
        );
        setIsStreaming(false);
      }
    },
    [providerId, isStreaming, messages],
  );

  const abort = useCallback(async () => {
    if (currentRunId.current !== null) {
      await invoke("ai_abort", { runId: currentRunId.current });
      currentRunId.current = null;
      setIsStreaming(false);
      setMessages((prev) =>
        prev.map((m, i) =>
          i === prev.length - 1 && m.role === "assistant" && m.streaming
            ? { ...m, streaming: false, content: m.content + " [已中断]" }
            : m,
        ),
      );
    }
  }, []);

  const clear = useCallback(() => {
    if (isStreaming) return;
    setMessages([]);
  }, [isStreaming]);

  return { messages, isStreaming, send, abort, clear };
}
