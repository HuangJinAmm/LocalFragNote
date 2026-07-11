import type { FC } from "react";
import type { DeckStats } from "./types";

interface Props {
  stats: DeckStats | null;
  loading?: boolean;
}

const DeckStatsView: FC<Props> = ({ stats, loading }) => {
  if (loading || !stats) {
    return (
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
        {[...Array(4)].map((_, i) => (
          <div key={i} className="h-20 rounded-lg bg-muted animate-pulse" />
        ))}
      </div>
    );
  }

  const items = [
    { label: "今日到期", value: stats.due_count, color: "text-orange-600" },
    { label: "新卡", value: stats.new_count, color: "text-blue-600" },
    { label: "总卡片", value: stats.total, color: "text-foreground" },
    {
      label: "掌握率",
      value: `${(stats.retention_rate * 100).toFixed(0)}%`,
      color: "text-green-600",
    },
  ];

  return (
    <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
      {items.map((item) => (
        <div key={item.label} className="rounded-lg border border-border p-3">
          <div className={`text-2xl font-bold ${item.color}`}>{item.value}</div>
          <div className="text-xs text-muted-foreground mt-1">{item.label}</div>
        </div>
      ))}
    </div>
  );
};

export default DeckStatsView;
