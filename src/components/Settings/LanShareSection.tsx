import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useLanDiscovery } from "@/components/LanDiscovery/hooks";
import type { AclAccessMode, AclRule } from "@/components/LanDiscovery/types";
import { useTranslate } from "@/utils/i18n";
import toast from "react-hot-toast";

const LanShareSection = () => {
  const t = useTranslate();
  const { peers } = useLanDiscovery();
  const [peerId, setPeerId] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [rules, setRules] = useState<AclRule[]>([]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    invoke<{ peer_id: string; display_name: string }>("lan_get_local_identity")
      .then((id) => {
        setPeerId(id.peer_id);
        setDisplayName(id.display_name);
      })
      .catch(console.error);
    invoke<AclRule[]>("lan_get_acl_rules")
      .then(setRules)
      .catch(console.error);
  }, []);

  const handleSaveDisplayName = async () => {
    try {
      await invoke("lan_update_display_name", { req: { name: displayName } });
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

  const getAccessMode = (peerId: string): AclAccessMode => {
    const peerRules = rules.filter((r) => r.peer_id === peerId);
    if (peerRules.length === 0) return "default-open";
    if (peerRules.some((r) => r.mode === "allow" && r.tags.includes("__none__"))) {
      return "completely-blocked";
    }
    return "restrict-tags";
  };

  const setAccessMode = (peerId: string, displayName: string, mode: AclAccessMode) => {
    // Remove all rules for this peer
    let newRules = rules.filter((r) => r.peer_id !== peerId);
    if (mode === "default-open") {
      // No rules
    } else if (mode === "completely-blocked") {
      newRules.push({
        peer_id: peerId,
        display_name: displayName,
        mode: "allow",
        tags: ["__none__"],
      });
    }
    // restrict-tags mode requires the user to manually select tags;
    // the actual UI needs a tag multi-select component, simplified here.
    setRules(newRules);
  };

  return (
    <div className="space-y-6">
      {/* Local identity */}
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
          {t("lan.settings.peerId")}: {peerId.slice(0, 16)}…
        </div>
      </div>

      {/* Service status */}
      <div className="text-sm">
        <span className="inline-flex items-center gap-1">
          <span className="size-2 rounded-full bg-green-500" />
          {t("lan.settings.statusRunning")}
        </span>
      </div>

      {/* ACL rules */}
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
                    <div className="text-xs text-muted-foreground">
                      {/* TODO: implement tag multi-select UI, simplified for now */}
                      Tag selection UI TBD
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
