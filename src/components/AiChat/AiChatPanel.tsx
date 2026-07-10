import { BotIcon, SettingsIcon, XIcon } from "lucide-react";
import { useState } from "react";
import { useTranslate } from "@/utils/i18n";
import { cn } from "@/lib/utils";
import { AiChatComposer } from "./AiChatComposer";
import { AiChatMessages } from "./AiChatMessages";
import { AiChatProviderPicker } from "./AiChatProviderPicker";
import { AiChatSettings } from "./AiChatSettings";
import { useAiChat } from "./hooks";

export function AiChatPanel() {
  const t = useTranslate();
  const [open, setOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [providerId, setProviderId] = useState<string | null>(null);
  const { messages, isStreaming, send, abort } = useAiChat({ providerId });

  return (
    <>
      <div className="fixed left-4 bottom-4 z-50">
        {open ? (
          <div className="flex flex-col w-[400px] h-[560px] rounded-xl border border-border bg-popover shadow-lg overflow-hidden">
            {/* Header */}
            <div className="flex items-center gap-2 border-b border-border px-3 py-2">
              <BotIcon className="size-4 text-primary" />
              <span className="font-medium text-sm flex-1">{t("aiChat.title")}</span>
              <AiChatProviderPicker
                onOpenSettings={() => setSettingsOpen(true)}
                onProviderChange={setProviderId}
              />
              <button
                onClick={() => setSettingsOpen(true)}
                className="size-7 rounded-md hover:bg-muted flex items-center justify-center"
                aria-label={t("aiChat.settings")}
              >
                <SettingsIcon className="size-3.5" />
              </button>
              <button
                onClick={() => setOpen(false)}
                className="size-7 rounded-md hover:bg-muted flex items-center justify-center"
                aria-label={t("aiChat.close")}
              >
                <XIcon className="size-3.5" />
              </button>
            </div>

            {/* Messages */}
            <AiChatMessages messages={messages} />

            {/* Composer */}
            <AiChatComposer
              isStreaming={isStreaming}
              disabled={!providerId}
              onSend={send}
              onAbort={abort}
            />
          </div>
        ) : (
          <button
            onClick={() => setOpen(true)}
            className={cn(
              "size-11 rounded-full bg-primary text-primary-foreground shadow-lg",
              "flex items-center justify-center hover:scale-110 active:scale-90 transition-transform",
            )}
            aria-label={t("aiChat.open")}
          >
            <BotIcon className="size-5" />
          </button>
        )}
      </div>

      <AiChatSettings
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        onSaved={() => {
          // 触发 picker 重新加载（通过 key 重新挂载）
          // 简单方案：关闭设置后重新打开 picker 会因 useEffect 重新加载
        }}
      />
    </>
  );
}
