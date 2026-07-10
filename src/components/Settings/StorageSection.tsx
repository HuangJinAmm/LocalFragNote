import { invoke } from "@tauri-apps/api/core";
import { isEqual } from "lodash-es";
import { DatabaseIcon, FolderOpenIcon, ZapIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "react-hot-toast";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { cn } from "@/lib/utils";
import { getErrorMessage } from "@/lib/error";
import { useTranslate } from "@/utils/i18n";
import SettingGroup from "./SettingGroup";
import { SettingList, SettingListItem } from "./SettingList";
import SettingSection from "./SettingSection";

interface StorageConfig {
  storage_type: string;
  local_storage_path: string;
  filepath_template: string;
  auto_threshold: number;
}

const DEFAULT_CONFIG: StorageConfig = {
  storage_type: "AUTO",
  local_storage_path: "attachments",
  filepath_template: "{uid}_{filename}",
  auto_threshold: 1024 * 1024,
};

const STORAGE_TYPES = [
  { value: "AUTO", icon: ZapIcon },
  { value: "DATABASE", icon: DatabaseIcon },
  { value: "LOCAL", icon: FolderOpenIcon },
] as const;

const STORAGE_LABELS: Record<string, "setting.storage.type-auto" | "setting.storage.type-database" | "setting.storage.type-local"> = {
  AUTO: "setting.storage.type-auto",
  DATABASE: "setting.storage.type-database",
  LOCAL: "setting.storage.type-local",
};

const STORAGE_DESCRIPTIONS: Record<string, "setting.storage.auto-description" | "setting.storage.database-description" | "setting.storage.local-description"> = {
  AUTO: "setting.storage.auto-description",
  DATABASE: "setting.storage.database-description",
  LOCAL: "setting.storage.local-description",
};

const StorageSection = () => {
  const t = useTranslate();
  const [config, setConfig] = useState<StorageConfig>(DEFAULT_CONFIG);
  const [original, setOriginal] = useState<StorageConfig>(DEFAULT_CONFIG);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    void (async () => {
      try {
        const cfg = await invoke<StorageConfig>("get_storage_config");
        setConfig(cfg);
        setOriginal(cfg);
      } catch (error) {
        toast.error(getErrorMessage(error, t("setting.storage.load-failed")));
      } finally {
        setLoading(false);
      }
    })();
  }, [t]);

  const update = (partial: Partial<StorageConfig>) => {
    setConfig((prev) => ({ ...prev, ...partial }));
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      const saved = await invoke<StorageConfig>("update_storage_config", { req: config });
      setConfig(saved);
      setOriginal(saved);
      toast.success(t("message.update-succeed"));
    } catch (error) {
      toast.error(getErrorMessage(error, t("setting.storage.save-failed")));
    } finally {
      setSaving(false);
    }
  };

  const isDirty = !isEqual(config, original);
  // auto_threshold 以 KB 为单位显示/编辑
  const thresholdKB = Math.round(config.auto_threshold / 1024);

  if (loading) {
    return (
      <SettingSection title={t("setting.storage.label")}>
        <div className="px-3 py-3 text-sm text-muted-foreground">…</div>
      </SettingSection>
    );
  }

  return (
    <SettingSection title={t("setting.storage.label")} description={t("setting.storage.current-storage-description")}>
      <SettingGroup title={t("setting.storage.current-storage")}>
        <RadioGroup
          value={config.storage_type}
          onValueChange={(value) => update({ storage_type: value })}
          className="gap-2"
        >
          {STORAGE_TYPES.map(({ value, icon: Icon }) => {
            const isSelected = config.storage_type === value;
            return (
              <label
                key={value}
                className={cn(
                  "flex cursor-pointer items-start gap-3 rounded-lg border p-3 transition-colors",
                  isSelected ? "border-accent bg-accent/40" : "border-border hover:bg-muted/30",
                )}
              >
                <RadioGroupItem value={value} className="mt-1" />
                <Icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1">
                  <div className="text-sm font-medium text-foreground">{t(STORAGE_LABELS[value])}</div>
                  <div className="mt-0.5 text-xs leading-5 text-muted-foreground">{t(STORAGE_DESCRIPTIONS[value])}</div>
                </div>
              </label>
            );
          })}
        </RadioGroup>
      </SettingGroup>

      {config.storage_type === "AUTO" && (
        <SettingGroup title={t("setting.storage.auto-threshold")} description={t("setting.storage.auto-threshold-description")} showSeparator>
          <SettingList>
            <SettingListItem label={t("setting.storage.auto-threshold")} description={t("setting.storage.auto-threshold-description")}>
              <div className="flex items-center gap-2">
                <Input
                  className="w-28 font-mono"
                  type="number"
                  min={1}
                  value={thresholdKB}
                  onChange={(event) => {
                    const kb = Math.max(1, Number(event.target.value) || 1);
                    update({ auto_threshold: kb * 1024 });
                  }}
                />
                <span className="text-xs text-muted-foreground">KB</span>
              </div>
            </SettingListItem>
          </SettingList>
        </SettingGroup>
      )}

      <SettingGroup title={t("setting.storage.local-storage-path")} description={t("setting.storage.local-note-path")} showSeparator>
        <SettingList>
          <SettingListItem label={t("setting.storage.local-storage-path")} description={t("setting.storage.local-note-path")}>
            <Input
              className="w-64 font-mono"
              value={config.local_storage_path}
              placeholder="attachments"
              onChange={(event) => update({ local_storage_path: event.target.value })}
            />
          </SettingListItem>
        </SettingList>
      </SettingGroup>

      <SettingGroup title={t("setting.storage.filepath-template")} description={t("setting.storage.filepath-template-description")} showSeparator>
        <SettingList>
          <SettingListItem label={t("setting.storage.filepath-template")} description={t("setting.storage.filepath-template-description")}>
            <Input
              className="w-64 font-mono"
              value={config.filepath_template}
              placeholder="{uid}_{filename}"
              onChange={(event) => update({ filepath_template: event.target.value })}
            />
          </SettingListItem>
        </SettingList>
      </SettingGroup>

      <div className="w-full flex justify-end">
        <Button disabled={!isDirty || saving} onClick={handleSave}>
          {t("common.save")}
        </Button>
      </div>
    </SettingSection>
  );
};

export default StorageSection;
