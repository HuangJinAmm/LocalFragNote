import { BotIcon, UserIcon } from "lucide-react";
import { useEffect, useRef } from "react";
import { MemoMarkdownRenderer } from "@/components/MemoContent/MemoMarkdownRenderer";
import { cn } from "@/lib/utils";
import type { ChatMessage, ContentPart } from "./types";

interface AiChatMessagesProps {
  messages: ChatMessage[];
}

export function AiChatMessages({ messages }: AiChatMessagesProps) {
  const scrollRef = useRef<HTMLDivElement>(null);

  // 自动滚动到底部
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages]);

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

  if (messages.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 text-muted-foreground">
        <BotIcon className="size-8" />
        <p className="text-sm">有什么可以帮你的？</p>
      </div>
    );
  }

  return (
    <div ref={scrollRef} className="flex-1 overflow-y-auto px-3 py-2 space-y-3">
      {messages.map((msg) => {
        if (msg.role === "tool") {
          return (
            <div key={msg.id} className="text-xs text-muted-foreground px-2 py-1 rounded bg-muted/50">
              {typeof msg.content === "string" ? msg.content : JSON.stringify(msg.content)}
            </div>
          );
        }
        const isUser = msg.role === "user";
        return (
          <div
            key={msg.id}
            className={cn("flex gap-2", isUser ? "flex-row-reverse" : "flex-row")}
          >
            <div className="shrink-0 mt-0.5">
              {isUser ? (
                <UserIcon className="size-5 text-muted-foreground" />
              ) : (
                <BotIcon className="size-5 text-primary" />
              )}
            </div>
            <div
              className={cn(
                "max-w-[85%] rounded-lg px-3 py-2 text-sm",
                isUser
                  ? "bg-primary text-primary-foreground"
                  : msg.isError
                    ? "bg-destructive/10 text-destructive"
                    : "bg-muted",
              )}
            >
              {isUser ? (
                renderUserContent(msg.content)
              ) : typeof msg.content === "string" && msg.content ? (
                <div className="break-words">
                  <MemoMarkdownRenderer
                    content={msg.content}
                    resolvedMentionUsernames={new Set()}
                  />
                  {msg.streaming && (
                    <span className="inline-block w-1 h-4 ml-0.5 bg-current animate-pulse" />
                  )}
                </div>
              ) : msg.streaming ? (
                <span className="text-muted-foreground text-xs">思考中...</span>
              ) : null}
            </div>
          </div>
        );
      })}
    </div>
  );
}
