import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { useTranslate } from "@/utils/i18n";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { ProviderConfig } from "./types";

/// localStorage key for the currently selected AI chat provider.
/// Shared so non-chat features (e.g. suggest_tags) can read the same value
/// to keep their LLM calls in sync with the chat panel's selection.
export const AI_CHAT_ACTIVE_PROVIDER_STORAGE_KEY = "ai_chat.active_provider";

interface AiChatProviderPickerProps {
  onProviderChange: (id: string | null) => void;
  /// 外部递增的刷新信号:值变化时重新加载 provider 列表。
  /// 用于在 AiChatSettings 保存后通知 picker 刷新,而无需关闭面板重开。
  refreshKey?: number;
}

export function AiChatProviderPicker({ onProviderChange, refreshKey }: AiChatProviderPickerProps) {
  const t = useTranslate();
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [selected, setSelected] = useState<string | null>(null);

  // 加载 provider 列表;refreshKey 变化时重新拉取(设置保存后触发)。
  // onProviderChange 用 ref 持有以避免它成为 useEffect 依赖造成循环重载。
  useEffect(() => {
    let cancelled = false;
    invoke<ProviderConfig[]>("list_providers")
      .then((list) => {
        if (cancelled) return;
        setProviders(list);
        // 从 localStorage 恢复选择,若不存在则选第一个
        const saved = localStorage.getItem(AI_CHAT_ACTIVE_PROVIDER_STORAGE_KEY);
        if (saved && list.some((p) => p.id === saved)) {
          setSelected(saved);
          onProviderChange(saved);
        } else if (list.length > 0) {
          setSelected(list[0].id);
          onProviderChange(list[0].id);
        } else {
          setSelected(null);
          onProviderChange(null);
        }
      })
      .catch(() => {
        if (cancelled) return;
        setSelected(null);
        onProviderChange(null);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshKey]);

  const handleChange = (value: string) => {
    setSelected(value);
    localStorage.setItem(AI_CHAT_ACTIVE_PROVIDER_STORAGE_KEY, value);
    onProviderChange(value);
  };

  return (
    <Select value={selected ?? undefined} onValueChange={handleChange}>
      <SelectTrigger size="sm" className="w-full min-w-[120px]">
        <SelectValue placeholder={t("aiChat.selectProvider")} />
      </SelectTrigger>
      <SelectContent>
        {providers.length === 0 && (
          <SelectItem value="__empty__" disabled>
            {t("aiChat.configureFirst")}
          </SelectItem>
        )}
        {providers.map((p) => (
          <SelectItem key={p.id} value={p.id}>
            {p.name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
