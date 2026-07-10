import { invoke } from "@tauri-apps/api/core";
import { SettingsIcon } from "lucide-react";
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

const STORAGE_KEY = "ai_chat.active_provider";

interface AiChatProviderPickerProps {
  onOpenSettings: () => void;
  onProviderChange: (id: string | null) => void;
}

export function AiChatProviderPicker({ onOpenSettings, onProviderChange }: AiChatProviderPickerProps) {
  const t = useTranslate();
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    invoke<ProviderConfig[]>("list_providers")
      .then((list) => {
        setProviders(list);
        // 从 localStorage 恢复选择，若不存在则选第一个
        const saved = localStorage.getItem(STORAGE_KEY);
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
        setSelected(null);
        onProviderChange(null);
      });
  }, [onProviderChange]);

  const handleChange = (value: string) => {
    if (value === "__settings__") {
      onOpenSettings();
      return;
    }
    setSelected(value);
    localStorage.setItem(STORAGE_KEY, value);
    onProviderChange(value);
  };

  return (
    <Select value={selected ?? undefined} onValueChange={handleChange}>
      <SelectTrigger size="sm" className="min-w-[120px]">
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
        <SelectItem value="__settings__">
          <SettingsIcon className="size-3.5" />
          {t("aiChat.settings")}
        </SelectItem>
      </SelectContent>
    </Select>
  );
}
