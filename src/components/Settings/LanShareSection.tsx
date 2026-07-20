import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { HashIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useLanDiscovery } from "@/components/LanDiscovery/hooks";
import type { AclAccessMode, AclRule, LanStatus } from "@/components/LanDiscovery/types";
import { cn } from "@/lib/utils";
import { useTranslate } from "@/utils/i18n";
import toast from "react-hot-toast";

const LanShareSection = () => {
  const t = useTranslate();
  const { peers } = useLanDiscovery();
  const [displayName, setDisplayName] = useState("");
  const [rules, setRules] = useState<AclRule[]>([]);
  const [status, setStatus] = useState<LanStatus | null>(null);
  const [saving, setSaving] = useState(false);
  const [toggling, setToggling] = useState(false);
  const [availableTags, setAvailableTags] = useState<string[]>([]);

  useEffect(() => {
    invoke<LanStatus>("lan_get_status")
      .then((nextStatus) => {
        setStatus(nextStatus);
        setDisplayName(nextStatus.display_name);
      })
      .catch(console.error);
    invoke<AclRule[]>("lan_get_acl_rules")
      .then(setRules)
      .catch(console.error);
    invoke<Array<{ tag: string; count: number }>>("list_tags")
      .then((tags) => setAvailableTags(tags.map((t) => t.tag).sort((a, b) => a.localeCompare(b))))
      .catch(console.error);
  }, []);

  // 获取某 peer 当前选中的允许标签（restrict-tags 模式下从 allow 规则中取，
  // 排除作为完全拒绝哨兵的 "__none__"）。
  const getPeerAllowedTags = (peerId: string): string[] => {
    const peerAllow = rules.find(
      (r) => r.peer_id === peerId && r.mode === "allow" && !r.tags.includes("__none__"),
    );
    return peerAllow ? peerAllow.tags : [];
  };

  const setPeerAllowedTags = (peerId: string, displayName: string, tags: string[]) => {
    setRules((prev) => {
      const others = prev.filter((r) => r.peer_id !== peerId);
      // 保留空 allow 规则,使下拉框保持 restrict-tags 状态;
      // 后端 filter_memos_for_peer 在 allow_tags 为空时回退到"全部可见",
      // 因此语义上等同 default-open,但 UI 状态更稳定。
      return [
        ...others,
        { peer_id: peerId, display_name: displayName, mode: "allow" as const, tags },
      ];
    });
  };

  const togglePeerTag = (peerId: string, displayName: string, tag: string) => {
    const current = getPeerAllowedTags(peerId);
    const next = current.includes(tag) ? current.filter((t) => t !== tag) : [...current, tag];
    setPeerAllowedTags(peerId, displayName, next);
  };

  const sortedAvailableTags = useMemo(() => availableTags, [availableTags]);

  const handleSaveDisplayName = async () => {
    try {
      await invoke("lan_update_display_name", { req: { name: displayName } });
      setStatus((prev) => (prev ? { ...prev, display_name: displayName } : prev));
      toast.success(t("lan.settings.saved"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleSaveRules = async () => {
    setSaving(true);
    try {
      await invoke("lan_save_acl_rules", { req: { rules } });
      toast.success(t("lan.settings.saved"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleToggleEnabled = async (enabled: boolean) => {
    setToggling(true);
    try {
      const nextStatus = await invoke<LanStatus>("lan_set_enabled", { enabled });
      setStatus(nextStatus);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setToggling(false);
    }
  };

  const getAccessMode = (peerId: string): AclAccessMode => {
    const peerRules = rules.filter((r) => r.peer_id === peerId);
    if (peerRules.length === 0) return "default-open";
    if (peerRules.some((r) => r.mode === "allow" && r.tags.includes("__none__"))) {
      return "completely-blocked";
    }
    return "restrict-tags";
  };

  const setAccessMode = (peerId: string, displayName: string, mode: AclAccessMode) => {
    // 先移除该 peer 现有的全部规则
    let newRules = rules.filter((r) => r.peer_id !== peerId);
    if (mode === "default-open") {
      // 默认开放时不写入规则
    } else if (mode === "completely-blocked") {
      newRules.push({
        peer_id: peerId,
        display_name: displayName,
        mode: "allow",
        tags: ["__none__"],
      });
    } else if (mode === "restrict-tags") {
      // restrict-tags 默认创建空 allow 规则,等待用户通过 chip 选择允许的标签
      // 空选择在 setPeerAllowedTags 中等同默认开放,但保留此处的占位规则
      // 避免下拉切换后立刻回退到 default-open 的视觉抖动
      newRules.push({
        peer_id: peerId,
        display_name: displayName,
        mode: "allow",
        tags: [],
      });
    }
    setRules(newRules);
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-3 rounded-lg border p-4">
        <div className="space-y-1">
          <Label>{t("lan.settings.enabled")}</Label>
          <div className="text-xs text-muted-foreground">
            {status?.running
              ? t("lan.settings.statusRunning")
              : status?.enabled
                ? t("lan.settings.statusError")
                : t("lan.settings.statusStopped")}
          </div>
        </div>
        <Switch
          checked={Boolean(status?.enabled)}
          disabled={toggling}
          onCheckedChange={handleToggleEnabled}
          aria-label={t("lan.settings.enabled")}
        />
      </div>

      {/* 本机身份 */}
      <div className="space-y-2">
        <Label>{t("lan.settings.displayName")}</Label>
        <div className="flex gap-2">
          <Input
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
            placeholder="LocalFragNote"
          />
          <Button onClick={handleSaveDisplayName}>{t("common.save")}</Button>
        </div>
        <div className="text-xs text-muted-foreground">
          {t("lan.settings.peerId")}: {status?.peer_id ? `${status.peer_id.slice(0, 16)}…` : "-"}
        </div>
      </div>

      {/* 运行状态 */}
      <div className="text-sm">
        <span className="inline-flex items-center gap-1">
          <span
            className={`size-2 rounded-full ${
              status?.running ? "bg-green-500" : status?.enabled ? "bg-amber-500" : "bg-muted-foreground/40"
            }`}
          />
          {status?.running
            ? t("lan.settings.statusRunning")
            : status?.enabled
              ? t("lan.settings.statusError")
              : t("lan.settings.statusStopped")}
        </span>
      </div>

      {/* ACL 规则 */}
      <div className="space-y-3">
        <Label>{t("lan.settings.aclRules")}</Label>
        {peers.length === 0 ? (
          <div className="text-sm text-muted-foreground">{t("lan.discovery.empty")}</div>
        ) : (
          <div className="space-y-2">
            {peers.map((peer) => {
              const mode = getAccessMode(peer.peer_id);
              return (
                <div key={peer.peer_id} className="border rounded p-3 space-y-2">
                  <div className="flex items-center justify-between">
                    <span className="font-medium">{peer.display_name}</span>
                    <Select
                      value={mode}
                      onValueChange={(v) =>
                        setAccessMode(peer.peer_id, peer.display_name, v as AclAccessMode)
                      }
                    >
                      <SelectTrigger className="w-40">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="default-open">
                          {t("lan.settings.defaultOpen")}
                        </SelectItem>
                        <SelectItem value="restrict-tags">
                          {t("lan.settings.restrictTags")}
                        </SelectItem>
                        <SelectItem value="completely-blocked">
                          {t("lan.settings.completelyBlocked")}
                        </SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                  <div className="text-xs text-muted-foreground">{peer.peer_id.slice(0, 16)}…</div>
                  {mode === "restrict-tags" && (
                    <div className="space-y-1.5 pt-1">
                      <div className="text-xs text-muted-foreground">
                        {t("lan.settings.restrictTagsHint")}
                      </div>
                      {sortedAvailableTags.length === 0 ? (
                        <div className="text-xs text-muted-foreground italic">
                          {t("lan.settings.noTagsAvailable")}
                        </div>
                      ) : (
                        <div className="flex flex-wrap gap-1.5">
                          {sortedAvailableTags.map((tag) => {
                            const selected = getPeerAllowedTags(peer.peer_id).includes(tag);
                            return (
                              <button
                                key={tag}
                                type="button"
                                onClick={() =>
                                  togglePeerTag(peer.peer_id, peer.display_name, tag)
                                }
                                className={cn(
                                  "inline-flex items-center gap-0.5 text-xs leading-5 px-2 py-0.5 rounded-full border transition-colors select-none",
                                  selected
                                    ? "border-primary bg-primary/10 text-primary"
                                    : "border-border text-muted-foreground hover:text-foreground hover:border-foreground/30",
                                )}
                              >
                                <HashIcon className="w-3 h-3" />
                                {tag}
                              </button>
                            );
                          })}
                        </div>
                      )}
                      {getPeerAllowedTags(peer.peer_id).length === 0 &&
                        sortedAvailableTags.length > 0 && (
                          <div className="text-[11px] text-muted-foreground/70 italic">
                            {t("lan.settings.defaultOpen")}
                          </div>
                        )}
                    </div>
                  )}
                </div>
              );
            })}
            <Button onClick={handleSaveRules} disabled={saving}>
              {t("lan.settings.save")}
            </Button>
          </div>
        )}
      </div>
    </div>
  );
};

export default LanShareSection;
