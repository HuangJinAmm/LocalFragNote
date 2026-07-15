import dayjs from "dayjs";
import { useMemo, useState } from "react";
import { YearCalendar } from "@/components/ActivityCalendar";
import type { CalendarData } from "@/components/ActivityCalendar/types";
import { useReviewTimestamps } from "./hooks";
import { useTranslate } from "@/utils/i18n";

/** 复习活动热力图：按年展示每日复习次数 */
const ReviewHeatmap = () => {
  const t = useTranslate();
  const { timestamps, loading } = useReviewTimestamps();
  const [selectedYear, setSelectedYear] = useState(() => new Date().getFullYear());

  // 将时间戳数组聚合为 CalendarData (Record<"YYYY-MM-DD", count>)
  const calendarData = useMemo<CalendarData>(() => {
    const counts: Record<string, number> = {};
    for (const ts of timestamps) {
      const dateStr = dayjs.unix(ts).format("YYYY-MM-DD");
      counts[dateStr] = (counts[dateStr] ?? 0) + 1;
    }
    return counts;
  }, [timestamps]);

  if (loading) {
    return (
      <div className="rounded-xl border border-border/20 bg-muted/5 p-4">
        <div className="text-sm text-muted-foreground animate-pulse">{t("common.loading")}</div>
      </div>
    );
  }

  if (timestamps.length === 0) {
    return null;
  }

  return (
    <div className="rounded-xl border border-border/20 bg-muted/5">
      <div className="px-4 pt-3 pb-1">
        <h3 className="text-sm font-medium text-foreground">{t("review.heatmap-title")}</h3>
        <p className="text-xs text-muted-foreground mt-0.5">{t("review.heatmap-description")}</p>
      </div>
      <YearCalendar
        selectedYear={selectedYear}
        data={calendarData}
        onYearChange={setSelectedYear}
        onDateClick={() => {}}
      />
    </div>
  );
};

export default ReviewHeatmap;
