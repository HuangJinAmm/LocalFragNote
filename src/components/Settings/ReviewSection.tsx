import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Input } from "@/components/ui/input";
import SettingSection from "./SettingSection";
import SettingGroup from "./SettingGroup";
import { SettingList, SettingListItem } from "./SettingList";
import { useTranslate } from "@/utils/i18n";
import toast from "react-hot-toast";

interface ProviderInfo {
  id: string;
  name: string;
}

const ReviewSection = () => {
  const t = useTranslate();
  const [dailyLimit, setDailyLimit] = useState(20);
  const [cardsPerMemo, setCardsPerMemo] = useState(2);
  const [providerId, setProviderId] = useState("");
  const [providers, setProviders] = useState<ProviderInfo[]>([]);

  useEffect(() => {
    invoke<string | null>("get_app_setting", { key: "review_config" })
      .then((json) => {
        if (json) {
          const config = JSON.parse(json);
          setDailyLimit(config.daily_new_card_limit ?? 20);
          setCardsPerMemo(config.default_cards_per_memo ?? 2);
          setProviderId(config.ai_provider_id ?? "");
        }
      })
      .catch(() => {});
    invoke<ProviderInfo[]>("list_providers")
      .then(setProviders)
      .catch(() => {});
  }, []);

  const saveConfig = async (key: string, value: unknown) => {
    try {
      const current = await invoke<string | null>("get_app_setting", { key: "review_config" }).catch(() => null);
      const config = current ? JSON.parse(current) : {};
      config[key] = value;
      await invoke("upsert_app_setting", {
        req: { key: "review_config", value: JSON.stringify(config) },
      });
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <SettingSection title={t("setting.review.label")}>
      <SettingGroup>
        <SettingList>
          <SettingListItem
            label={t("review.daily-new-card-limit")}
            description={t("review.daily-new-card-limit-desc")}
          >
            <Input
              type="number"
              min={0}
              max={200}
              value={dailyLimit}
              onChange={(e) => {
                const v = Number(e.target.value) || 0;
                setDailyLimit(v);
                saveConfig("daily_new_card_limit", v);
              }}
              className="w-24"
            />
          </SettingListItem>
          <SettingListItem
            label={t("review.default-cards-per-memo")}
            description={t("review.default-cards-per-memo-desc")}
          >
            <Input
              type="number"
              min={1}
              max={10}
              value={cardsPerMemo}
              onChange={(e) => {
                const v = Number(e.target.value) || 1;
                setCardsPerMemo(v);
                saveConfig("default_cards_per_memo", v);
              }}
              className="w-24"
            />
          </SettingListItem>
          <SettingListItem
            label={t("review.ai-provider")}
            description={t("review.ai-provider-desc")}
          >
            <select
              value={providerId}
              onChange={(e) => {
                setProviderId(e.target.value);
                saveConfig("ai_provider_id", e.target.value);
              }}
              className="w-48 rounded-md border border-border px-2 py-1"
            >
              <option value="">{t("review.use-default")}</option>
              {providers.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </SettingListItem>
        </SettingList>
      </SettingGroup>
    </SettingSection>
  );
};

export default ReviewSection;
