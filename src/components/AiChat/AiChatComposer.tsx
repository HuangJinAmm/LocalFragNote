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
