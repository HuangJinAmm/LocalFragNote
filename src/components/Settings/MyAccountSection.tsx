import { timestampDate, type Timestamp } from "@bufbuild/protobuf/wkt";
import HeatMap from "@uiw/react-heat-map";
import { ChevronLeftIcon, ChevronRightIcon, PenLineIcon } from "lucide-react";
import dayjs from "dayjs";
import { useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import useCurrentUser from "@/hooks/useCurrentUser";
import { useDialog } from "@/hooks/useDialog";
import { useUserStats } from "@/hooks/useUserQueries";
import { useTranslate } from "@/utils/i18n";
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

  // 获取当前用户的 memo 创建时间戳统计，用于热力图
  const { data: userStats, isLoading: isLoadingStats } = useUserStats(user?.name);

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

      <UpdateAccountDialog open={accountDialog.isOpen} onOpenChange={accountDialog.setOpen} />
    </SettingSection>
  );
};

export default MyAccountSection;
