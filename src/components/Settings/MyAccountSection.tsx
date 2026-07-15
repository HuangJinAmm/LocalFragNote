import { timestampDate, type Timestamp } from "@bufbuild/protobuf/wkt";
import HeatMap from "@uiw/react-heat-map";
import { ChevronLeftIcon, ChevronRightIcon, DownloadIcon, PenLineIcon, UploadIcon } from "lucide-react";
import dayjs from "dayjs";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "react-hot-toast";
import { useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import useCurrentUser from "@/hooks/useCurrentUser";
import { useDialog } from "@/hooks/useDialog";
import { useUserStats } from "@/hooks/useUserQueries";
import { useTranslate } from "@/utils/i18n";
import { downloadText } from "@/helpers/utils";
import UpdateAccountDialog from "../UpdateAccountDialog";
import UserAvatar from "../UserAvatar";
import SettingGroup from "./SettingGroup";
import SettingSection from "./SettingSection";

// 本地单用户应用：去除密码/AccessToken/SSO/删除账号等认证相关功能
const MyAccountSection = () => {
  const t = useTranslate();
  const user = useCurrentUser();
  const accountDialog = useDialog();
  const [selectedYear, setSelectedYear] = useState(() => new Date().getFullYear());
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);
  const jsonInputRef = useRef<HTMLInputElement>(null);
  const mdInputRef = useRef<HTMLInputElement>(null);

  // 获取当前用户的 memo 创建时间戳统计，用于热力图
  const { data: userStats, isLoading: isLoadingStats } = useUserStats(user?.name);

  // 生成带时间戳的文件名前缀
  const filePrefix = () => `memos-backup-${dayjs().format("YYYYMMDD-HHmmss")}`;

  const handleExportJson = async () => {
    setExporting(true);
    try {
      const text = await invoke<string>("export_memos_json");
      downloadText(`${filePrefix()}.json`, text, "application/json");
      toast.success(t("setting.account.export-memos"));
    } catch (e) {
      toast.error(t("setting.account.export-failed"));
      console.error(e);
    } finally {
      setExporting(false);
    }
  };

  const handleExportMarkdown = async () => {
    setExporting(true);
    try {
      const text = await invoke<string>("export_memos_markdown");
      downloadText(`${filePrefix()}.md`, text, "text/markdown");
      toast.success(t("setting.account.export-memos"));
    } catch (e) {
      toast.error(t("setting.account.export-failed"));
      console.error(e);
    } finally {
      setExporting(false);
    }
  };

  // 通用导入处理：读取文件文本后调用对应 IPC 命令
  const handleImport = async (file: File, command: "import_memos_json" | "import_memos_markdown") => {
    setImporting(true);
    try {
      const text = await file.text();
      if (!text.trim()) {
        toast.error(t("setting.account.import-empty"));
        return;
      }
      const count = await invoke<number>(command, {
        jsonStr: text,
        markdownStr: text,
      });
      toast.success(t("setting.account.import-success", { count }));
    } catch (e) {
      toast.error(t("setting.account.import-failed"));
      console.error(e);
    } finally {
      setImporting(false);
    }
  };

  const onJsonInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) void handleImport(file, "import_memos_json");
    // 重置 value 允许重复选择同一文件
    e.target.value = "";
  };

  const onMdInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) void handleImport(file, "import_memos_markdown");
    e.target.value = "";
  };

  // 将 proto Timestamp 数组转换为 react-heat-map 所需的 value 数组
  // value: { date: "YYYY/MM/DD", count: number }[]
  // 注意：date 使用 "/" 分隔以兼容 Safari
  const heatMapValue = useMemo(() => {
    const timestamps: Timestamp[] = userStats?.memoCreatedTimestamps ?? [];
    if (timestamps.length === 0) return [];
    const counts: Record<string, number> = {};
    for (const ts of timestamps) {
      const date = ts ? timestampDate(ts) : undefined;
      if (!date) continue;
      const key = dayjs(date).format("YYYY/MM/DD");
      counts[key] = (counts[key] ?? 0) + 1;
    }
    return Object.entries(counts).map(([date, count]) => ({ date, count }));
  }, [userStats]);

  // 当前年份的起止日期
  const startDate = useMemo(() => new Date(`${selectedYear}/01/01`), [selectedYear]);
  const endDate = useMemo(() => new Date(`${selectedYear}/12/31`), [selectedYear]);
  // 一年最多 53 周，每列 rectSize(11) + space(2) = 13px，左侧 weekLabels 占 28px
  // 总宽度需 >= 28 + 53 * 13 = 717px，设 740 以确保完整显示
  const heatMapWidth = 740;

  return (
    <SettingSection title={t("setting.my-account.label")}>
      <SettingGroup title={t("setting.account.title")}>
        <div className="w-full flex flex-row justify-start items-center gap-3">
          <UserAvatar className="shrink-0 w-12 h-12" avatarUrl={user?.avatarUrl} />
          <div className="flex-1 min-w-0 flex flex-col justify-center items-start gap-1">
            <div className="w-full">
              <span className="text-lg font-semibold">{user?.displayName}</span>
              <span className="ml-2 text-sm text-muted-foreground">@{user?.username}</span>
            </div>
            {user?.description && <p className="w-full text-sm text-muted-foreground truncate">{user?.description}</p>}
          </div>
          <div className="flex items-center gap-2 shrink-0">
            <Button variant="outline" size="sm" onClick={accountDialog.open}>
              <PenLineIcon className="w-4 h-4 mr-1.5" />
              {t("common.edit")}
            </Button>
          </div>
        </div>
      </SettingGroup>

      <SettingGroup title={t("common.activity")} description={t("setting.account.activity-description")}>
        <div className="flex items-center gap-2 mb-3 px-1">
          <Button variant="ghost" size="sm" onClick={() => setSelectedYear(selectedYear - 1)} aria-label="Previous year" className="h-7 w-7 p-0">
            <ChevronLeftIcon className="w-4 h-4" />
          </Button>
          <span className="text-lg font-semibold tracking-tight">{selectedYear}</span>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setSelectedYear(selectedYear + 1)}
            aria-label="Next year"
            className="h-7 w-7 p-0"
            disabled={selectedYear >= new Date().getFullYear()}
          >
            <ChevronRightIcon className="w-4 h-4" />
          </Button>
          {selectedYear !== new Date().getFullYear() && (
            <Button variant="ghost" size="sm" onClick={() => setSelectedYear(new Date().getFullYear())} className="h-7 px-2 text-xs">
              {t("common.today")}
            </Button>
          )}
        </div>
        {isLoadingStats ? (
          <div className="w-full py-8 text-center text-sm text-muted-foreground">{t("common.loading")}</div>
        ) : (
          <div className="w-full overflow-x-auto rounded-xl border border-border/30 bg-background p-3">
            <HeatMap
              value={heatMapValue}
              width={heatMapWidth}
              startDate={startDate}
              endDate={endDate}
              weekLabels={["", "Mon", "", "Wed", "", "Fri", ""]}
              panelColors={{
                0: "#ebedf0",
                2: "#c6e48b",
                4: "#7bc96f",
                6: "#239a3b",
                8: "#196127",
              }}
              rectProps={{ rx: 2 }}
              legendCellSize={0}
            />
          </div>
        )}
      </SettingGroup>

      <SettingGroup title={t("setting.account.backup-title")} description={t("setting.account.backup-description")}>
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
          <Button variant="outline" size="sm" onClick={handleExportJson} disabled={exporting || importing}>
            <DownloadIcon className="w-4 h-4 mr-1.5" />
            {t("setting.account.export-json")}
          </Button>
          <Button variant="outline" size="sm" onClick={handleExportMarkdown} disabled={exporting || importing}>
            <DownloadIcon className="w-4 h-4 mr-1.5" />
            {t("setting.account.export-markdown")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => jsonInputRef.current?.click()}
            disabled={exporting || importing}
          >
            <UploadIcon className="w-4 h-4 mr-1.5" />
            {t("setting.account.import-json")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => mdInputRef.current?.click()}
            disabled={exporting || importing}
          >
            <UploadIcon className="w-4 h-4 mr-1.5" />
            {t("setting.account.import-markdown")}
          </Button>
        </div>
        {/* 隐藏的文件输入：JSON 导入 */}
        <input
          ref={jsonInputRef}
          type="file"
          accept="application/json,.json"
          className="hidden"
          onChange={onJsonInputChange}
        />
        {/* 隐藏的文件输入：Markdown 导入 */}
        <input
          ref={mdInputRef}
          type="file"
          accept="text/markdown,.md,text/plain"
          className="hidden"
          onChange={onMdInputChange}
        />
      </SettingGroup>

      <UpdateAccountDialog open={accountDialog.isOpen} onOpenChange={accountDialog.setOpen} />
    </SettingSection>
  );
};

export default MyAccountSection;
