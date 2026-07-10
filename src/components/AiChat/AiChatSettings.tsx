import { invoke } from "@tauri-apps/api/core";
import { PencilIcon, PlusIcon, TrashIcon } from "lucide-react";
import { useEffect, useState } from "react";
import toast from "react-hot-toast";
import { useTranslate } from "@/utils/i18n";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { PROVIDER_PRESETS, type ProviderConfig, type ProviderPreset } from "./types";

interface AiChatSettingsProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: () => void;
}

export function AiChatSettings({ open, onOpenChange, onSaved }: AiChatSettingsProps) {
  const t = useTranslate();
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [editing, setEditing] = useState<ProviderConfig | null>(null);

  useEffect(() => {
    if (open) {
      invoke<ProviderConfig[]>("list_providers").then(setProviders).catch(toast.error);
    }
  }, [open]);

  const handleSave = async (provider: ProviderConfig) => {
    const existing = providers.findIndex((p) => p.id === provider.id);
    const next = existing >= 0
      ? providers.map((p) => (p.id === provider.id ? provider : p))
      : [...providers, provider];
    try {
      await invoke<ProviderConfig[]>("save_providers_cmd", { providers: next });
      setProviders(next);
      setEditing(null);
      onSaved();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleDelete = async (id: string) => {
    const next = providers.filter((p) => p.id !== id);
    try {
      await invoke<ProviderConfig[]>("save_providers_cmd", { providers: next });
      setProviders(next);
      onSaved();
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent size="lg">
        <DialogHeader>
          <DialogTitle>{t("aiChat.settingsTitle")}</DialogTitle>
        </DialogHeader>

        {editing ? (
          <ProviderForm
            provider={editing}
            onSave={handleSave}
            onCancel={() => setEditing(null)}
          />
        ) : (
          <div className="flex flex-col gap-3">
            {providers.length === 0 && (
              <p className="text-sm text-muted-foreground">{t("aiChat.configureFirst")}</p>
            )}
            {providers.map((p) => (
              <div key={p.id} className="flex items-center justify-between rounded-md border p-3">
                <div className="min-w-0 flex-1">
                  <div className="font-medium">{p.name}</div>
                  <div className="truncate text-xs text-muted-foreground">
                    {p.base_url} · {p.model}
                  </div>
                </div>
                <div className="flex gap-1">
                  <Button size="icon" variant="ghost" onClick={() => setEditing(p)}>
                    <PencilIcon className="size-4" />
                  </Button>
                  <Button size="icon" variant="ghost" onClick={() => handleDelete(p.id)}>
                    <TrashIcon className="size-4" />
                  </Button>
                </div>
              </div>
            ))}
            <Button
              variant="outline"
              onClick={() =>
                setEditing({
                  id: crypto.randomUUID(),
                  name: "",
                  base_url: "",
                  api_key: "",
                  model: "",
                })
              }
            >
              <PlusIcon className="size-4 mr-1" />
              {t("aiChat.addProvider")}
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

function ProviderForm({
  provider,
  onSave,
  onCancel,
}: {
  provider: ProviderConfig;
  onSave: (p: ProviderConfig) => void;
  onCancel: () => void;
}) {
  const t = useTranslate();
  const [form, setForm] = useState<ProviderConfig>(provider);

  const applyPreset = (preset: ProviderPreset) => {
    setForm({
      ...form,
      name: preset.name,
      base_url: preset.base_url,
      model: preset.model,
    });
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap gap-2">
        {PROVIDER_PRESETS.map((preset) => (
          <Button key={preset.label} size="sm" variant="outline" onClick={() => applyPreset(preset)}>
            {preset.label}
          </Button>
        ))}
      </div>
      <div className="flex flex-col gap-2">
        <Label>{t("aiChat.name")}</Label>
        <Input
          value={form.name}
          onChange={(e) => setForm({ ...form, name: e.target.value })}
          placeholder="OpenAI"
        />
      </div>
      <div className="flex flex-col gap-2">
        <Label>{t("aiChat.baseUrl")}</Label>
        <Input
          value={form.base_url}
          onChange={(e) => setForm({ ...form, base_url: e.target.value })}
          placeholder="https://api.openai.com/v1"
        />
      </div>
      <div className="flex flex-col gap-2">
        <Label>{t("aiChat.apiKey")}</Label>
        <Input
          type="password"
          value={form.api_key}
          onChange={(e) => setForm({ ...form, api_key: e.target.value })}
          placeholder="sk-..."
        />
      </div>
      <div className="flex flex-col gap-2">
        <Label>{t("aiChat.model")}</Label>
        <Input
          value={form.model}
          onChange={(e) => setForm({ ...form, model: e.target.value })}
          placeholder="gpt-4o-mini"
        />
      </div>
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel}>
          {t("aiChat.cancel")}
        </Button>
        <Button onClick={() => onSave(form)} disabled={!form.name || !form.base_url || !form.model}>
          {t("aiChat.save")}
        </Button>
      </div>
    </div>
  );
}
