import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  CheckCircle2Icon,
  CircleDotIcon,
  CopyIcon,
  NetworkIcon,
  PlayIcon,
  SquareIcon,
  ZapIcon,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import toast from "react-hot-toast";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { getErrorMessage } from "@/lib/error";
import { cn } from "@/lib/utils";
import { useTranslate } from "@/utils/i18n";
import SettingGroup from "./SettingGroup";
import { SettingList, SettingListItem } from "./SettingList";
import SettingSection from "./SettingSection";

interface McpConfig {
  enabled: boolean;
  host: string;
  port: number;
  auth_token: string;
  auto_start: boolean;
}

interface McpStatus {
  running: boolean;
  endpoint_url: string;
  started_at: number | null;
  last_error: string | null;
  config: McpConfig;
}

interface McpTestResult {
  ok: boolean;
  status: number;
  body_preview: string;
  error: string | null;
}

const DEFAULT_CONFIG: McpConfig = {
  enabled: false,
  host: "127.0.0.1",
  port: 27100,
  auth_token: "",
  auto_start: false,
};

/** 派生 MCP 端点 URL（与后端 endpoint_url 一致） */
const deriveEndpointUrl = (config: McpConfig): string => {
  const host = config.host.trim() || "127.0.0.1";
  const port = config.port || 27100;
  return `http://${host}:${port}/mcp`;
};

const McpSection = () => {
  const t = useTranslate();
  const [config, setConfig] = useState<McpConfig>(DEFAULT_CONFIG);
  const [original, setOriginal] = useState<McpConfig>(DEFAULT_CONFIG);
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [starting, setStarting] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<McpTestResult | null>(null);
  const [showToken, setShowToken] = useState(false);
  const statusRef = useRef<McpStatus | null>(null);
  statusRef.current = status;

  // 加载配置 + 状态
  useEffect(() => {
    void (async () => {
      try {
        const cfg = await invoke<McpConfig>("mcp_get_config");
        setConfig(cfg);
        setOriginal(cfg);
        const st = await invoke<McpStatus>("mcp_get_status");
        setStatus(st);
      } catch (error) {
        toast.error(getErrorMessage(error, t("setting.mcp.load-failed")));
      } finally {
        setLoading(false);
      }
    })();
  }, [t]);

  // 订阅状态变更事件 + 兜底轮询
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let pollTimer: ReturnType<typeof setInterval> | undefined;
    (async () => {
      unlisten = await listen("mcp:status-changed", async () => {
        try {
          const st = await invoke<McpStatus>("mcp_get_status");
          setStatus(st);
        } catch (e) {
          console.error(e);
        }
      });
    })();
    pollTimer = setInterval(() => {
      invoke<McpStatus>("mcp_get_status")
        .then(setStatus)
        .catch(() => {});
    }, 5000);
    return () => {
      unlisten?.();
      if (pollTimer) clearInterval(pollTimer);
    };
  }, []);

  const update = (partial: Partial<McpConfig>) => {
    setConfig((prev) => ({ ...prev, ...partial }));
  };

  const isDirty = JSON.stringify(config) !== JSON.stringify(original);
  const running = Boolean(status?.running);
  const endpointUrl = running && status?.endpoint_url ? status.endpoint_url : deriveEndpointUrl(config);

  const handleSave = async () => {
    setSaving(true);
    try {
      const saved = await invoke<McpConfig>("mcp_update_config", { req: config });
      setConfig(saved);
      setOriginal(saved);
      toast.success(t("setting.mcp.config-saved"));
    } catch (error) {
      toast.error(getErrorMessage(error, t("setting.mcp.save-failed")));
    } finally {
      setSaving(false);
    }
  };

  const handleStart = async () => {
    setStarting(true);
    setTestResult(null);
    try {
      const st = await invoke<McpStatus>("mcp_start");
      setStatus(st);
      if (st.running) {
        toast.success(t("setting.mcp.started"));
      } else if (st.last_error) {
        toast.error(st.last_error);
      }
    } catch (error) {
      toast.error(getErrorMessage(error, t("setting.mcp.start-failed")));
    } finally {
      setStarting(false);
    }
  };

  const handleStop = async () => {
    setStopping(true);
    setTestResult(null);
    try {
      const st = await invoke<McpStatus>("mcp_stop");
      setStatus(st);
      toast.success(t("setting.mcp.stopped-toast"));
    } catch (error) {
      toast.error(getErrorMessage(error, t("setting.mcp.stop-failed")));
    } finally {
      setStopping(false);
    }
  };

  const handleTestConnection = async () => {
    setTesting(true);
    try {
      const result = await invoke<McpTestResult>("mcp_test_connection");
      setTestResult(result);
      if (result.ok) {
        toast.success(t("setting.mcp.test-ok"));
      } else {
        toast.error(t("setting.mcp.test-fail"));
      }
    } catch (error) {
      toast.error(getErrorMessage(error, t("setting.mcp.test-fail")));
    } finally {
      setTesting(false);
    }
  };

  const handleCopyUrl = async () => {
    try {
      await navigator.clipboard.writeText(endpointUrl);
      toast.success(t("setting.mcp.url-copied"));
    } catch {
      toast.error(t("setting.mcp.copy-failed"));
    }
  };

  const handleCopyClientJson = async () => {
    const token = config.auth_token.trim();
    const clientConfig = {
      mcpServers: {
        "localfragnote": {
          type: "streamableHttp",
          url: endpointUrl,
          ...(token ? { headers: { Authorization: `Bearer ${token}` } } : {}),
        },
      },
    };
    try {
      await navigator.clipboard.writeText(JSON.stringify(clientConfig, null, 2));
      toast.success(t("setting.mcp.client-json-copied"));
    } catch {
      toast.error(t("setting.mcp.copy-failed"));
    }
  };

  if (loading) {
    return (
      <SettingSection title={t("setting.mcp.label")}>
        <div className="px-3 py-3 text-sm text-muted-foreground">…</div>
      </SettingSection>
    );
  }

  return (
    <SettingSection
      title={t("setting.mcp.label")}
      description={t("setting.mcp.description")}
      actions={
        <div className="flex items-center gap-2">
          {running ? (
            <Button variant="destructive" size="sm" onClick={handleStop} disabled={stopping}>
              <SquareIcon className="size-4" />
              {stopping ? t("setting.mcp.stopping") : t("setting.mcp.stop")}
            </Button>
          ) : (
            <Button size="sm" onClick={handleStart} disabled={starting}>
              <PlayIcon className="size-4" />
              {starting ? t("setting.mcp.starting") : t("setting.mcp.start")}
            </Button>
          )}
        </div>
      }
    >
      {/* 运行状态卡片 */}
      <div className="rounded-lg border p-4 space-y-3">
        <div className="flex items-center justify-between gap-3">
          <div className="space-y-1">
            <Label>{t("setting.mcp.status-title")}</Label>
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <span
                className={cn(
                  "size-2 rounded-full",
                  running ? "bg-green-500" : "bg-muted-foreground/40",
                )}
              />
              {running ? t("setting.mcp.running") : t("setting.mcp.stopped")}
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={handleTestConnection}
              disabled={testing || !running}
            >
              <ZapIcon className="size-4" />
              {testing ? t("setting.mcp.testing") : t("setting.mcp.test")}
            </Button>
          </div>
        </div>

        {/* MCP 端点 URL + 复制 */}
        <div className="space-y-2">
          <Label className="text-xs text-muted-foreground">{t("setting.mcp.endpoint-url")}</Label>
          <div className="flex flex-wrap gap-2">
            <code className="flex-1 min-w-0 rounded bg-muted px-2 py-1.5 text-xs font-mono truncate">
              {endpointUrl}
            </code>
            <Button variant="outline" size="sm" onClick={handleCopyUrl}>
              <CopyIcon className="size-3.5" />
              {t("setting.mcp.copy-url")}
            </Button>
            <Button variant="outline" size="sm" onClick={handleCopyClientJson}>
              <NetworkIcon className="size-3.5" />
              {t("setting.mcp.copy-client-json")}
            </Button>
          </div>
        </div>

        {/* 元信息 */}
        <div className="flex flex-wrap gap-x-6 gap-y-1 text-xs text-muted-foreground">
          {status?.started_at != null && (
            <span>
              {t("setting.mcp.started-at")}:{" "}
              <span className="font-mono">
                {new Date(status.started_at * 1000).toLocaleTimeString()}
              </span>
            </span>
          )}
        </div>

        {/* 错误 */}
        {status?.last_error && (
          <div className="rounded border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {status.last_error}
          </div>
        )}

        {/* 测试结果 */}
        {testResult && (
          <div
            className={cn(
              "rounded border px-3 py-2 text-xs",
              testResult.ok
                ? "border-green-500/40 bg-green-500/10 text-green-700 dark:text-green-400"
                : "border-amber-500/40 bg-amber-500/10 text-amber-700 dark:text-amber-400",
            )}
          >
            <div className="flex items-center gap-1.5 font-medium">
              {testResult.ok ? (
                <CheckCircle2Icon className="size-3.5" />
              ) : (
                <CircleDotIcon className="size-3.5" />
              )}
              {testResult.ok
                ? t("setting.mcp.test-ok")
                : t("setting.mcp.test-fail")}
              <span className="font-mono opacity-70">HTTP {testResult.status}</span>
            </div>
            {testResult.error && <div className="mt-1 opacity-80">{testResult.error}</div>}
            {testResult.body_preview && (
              <pre className="mt-1 max-h-24 overflow-auto font-mono opacity-70 whitespace-pre-wrap">
                {testResult.body_preview}
              </pre>
            )}
          </div>
        )}
      </div>

      {/* 网络配置 */}
      <SettingGroup
        title={t("setting.mcp.network-title")}
        description={t("setting.mcp.network-description")}
      >
        <SettingList>
          <SettingListItem
            label={t("setting.mcp.host")}
            description={t("setting.mcp.host-hint")}
          >
            <Input
              className="w-40 font-mono"
              value={config.host}
              onChange={(e) => update({ host: e.target.value })}
              placeholder="127.0.0.1"
            />
          </SettingListItem>
          <SettingListItem label={t("setting.mcp.port")} description={t("setting.mcp.port-hint")}>
            <Input
              className="w-28 font-mono"
              type="number"
              min={1}
              max={65535}
              value={config.port}
              onChange={(e) => update({ port: Math.max(1, Math.min(65535, Number(e.target.value) || 27100)) })}
            />
          </SettingListItem>
        </SettingList>
      </SettingGroup>

      {/* 鉴权 */}
      <SettingGroup
        title={t("setting.mcp.auth-title")}
        description={t("setting.mcp.auth-description")}
        showSeparator
      >
        <SettingList>
          <SettingListItem
            label={t("setting.mcp.auth-token")}
            description={t("setting.mcp.auth-token-hint")}
          >
          <div className="flex items-center gap-2 w-72">
            <Input
              className="flex-1 font-mono"
              type={showToken ? "text" : "password"}
              value={config.auth_token}
              onChange={(e) => update({ auth_token: e.target.value })}
              placeholder={t("setting.mcp.auth-token-placeholder")}
            />
            <Button
              variant="outline"
              size="sm"
              onClick={() => setShowToken((v) => !v)}
              type="button"
            >
              {showToken ? t("setting.mcp.hide-token") : t("setting.mcp.show-token")}
            </Button>
          </div>
          </SettingListItem>
        </SettingList>
      </SettingGroup>

      {/* 工具说明 */}
      <SettingGroup
        title={t("setting.mcp.tools-title")}
        description={t("setting.mcp.tools-description")}
        showSeparator
      >
        <div className="rounded-md border bg-muted/30 p-3 text-xs leading-5 text-muted-foreground">
          <div className="font-medium text-foreground mb-1.5">
            {t("setting.mcp.tools-list-title")}
          </div>
          <ul className="space-y-0.5 font-mono">
            <li>• create_memo — {t("setting.mcp.tool-create-memo")}</li>
            <li>• update_memo — {t("setting.mcp.tool-update-memo")}</li>
            <li>• delete_memo — {t("setting.mcp.tool-delete-memo")}</li>
            <li>• get_memo — {t("setting.mcp.tool-get-memo")}</li>
            <li>• list_memos — {t("setting.mcp.tool-list-memos")}</li>
            <li>• search_memos — {t("setting.mcp.tool-search-memos")}</li>
            <li>• list_tags — {t("setting.mcp.tool-list-tags")}</li>
          </ul>
        </div>
      </SettingGroup>

      {/* 自动启动 */}
      <SettingGroup showSeparator>
        <SettingList>
          <SettingListItem
            label={t("setting.mcp.auto-start")}
            description={t("setting.mcp.auto-start-hint")}
          >
            <Switch
              checked={config.auto_start}
              onCheckedChange={(v) => update({ auto_start: v })}
              aria-label={t("setting.mcp.auto-start")}
            />
          </SettingListItem>
        </SettingList>
      </SettingGroup>

      {/* 保存按钮 */}
      <div className="w-full flex justify-end">
        <Button disabled={!isDirty || saving} onClick={handleSave}>
          {t("common.save")}
        </Button>
      </div>
    </SettingSection>
  );
};

export default McpSection;
