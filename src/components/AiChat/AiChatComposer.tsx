import { SendIcon, SquareIcon } from "lucide-react";
import { useState } from "react";
import { useTranslate } from "@/utils/i18n";
import { cn } from "@/lib/utils";

interface AiChatComposerProps {
  isStreaming: boolean;
  disabled: boolean;
  onSend: (text: string) => void;
  onAbort: () => void;
}

export function AiChatComposer({ isStreaming, disabled, onSend, onAbort }: AiChatComposerProps) {
  const t = useTranslate();
  const [text, setText] = useState("");

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = text.trim();
    if (!trimmed || isStreaming || disabled) return;
    onSend(trimmed);
    setText("");
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Enter 发送，Shift+Enter 换行
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e as unknown as React.FormEvent);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="border-t border-border p-2 flex gap-2 items-end">
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={handleKeyDown}
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
          disabled={!text.trim() || disabled}
          className="shrink-0 size-9 rounded-md bg-primary text-primary-foreground flex items-center justify-center disabled:opacity-50 disabled:cursor-not-allowed hover:opacity-90"
          aria-label={t("aiChat.send")}
        >
          <SendIcon className="size-4" />
        </button>
      )}
    </form>
  );
}
