import { Button } from "@/components/ui/button";
import { ArrowLeftIcon, PlayIcon, RefreshCwIcon } from "lucide-react";
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import CardTable from "./CardTable";
import DeckStatsView from "./DeckStats";
import { useDeckStats, useGenerateCards, useReviewCards } from "./hooks";
import type { ReviewDeck } from "./types";
import { useTranslate } from "@/utils/i18n";

interface Props {
  deck: ReviewDeck;
  onBack: () => void;
  onStartReview: () => void;
}

const DeckDetail = ({ deck, onBack, onStartReview }: Props) => {
  const t = useTranslate();
  const { stats, refresh: refreshStats } = useDeckStats(deck.id);
  const { cards, refresh: refreshCards } = useReviewCards(deck.id);
  const { generating, progress, result, generate } = useGenerateCards(deck.id);
  const [showProgress, setShowProgress] = useState(false);

  const handleGenerate = async () => {
    setShowProgress(true);
    await generate();
    refreshCards();
    refreshStats();
  };

  return (
    <div className="space-y-4">
      {/* 顶部 */}
      <div className="flex items-center justify-between">
        <Button variant="ghost" size="sm" onClick={onBack}>
          <ArrowLeftIcon className="size-4 mr-1" />
          {t("common.back")}
        </Button>
      </div>

      {/* Deck 信息 */}
      <div>
        <h1 className="text-2xl font-bold">{deck.name}</h1>
        <div className="flex flex-wrap gap-2 mt-1">
          {deck.tags.map((tag) => (
            <span key={tag} className="text-sm text-muted-foreground">
              #{tag}
            </span>
          ))}
        </div>
      </div>

      {/* 统计 */}
      <DeckStatsView stats={stats} />

      {/* 操作 */}
      <div className="flex gap-2">
        <Button onClick={onStartReview} disabled={stats?.due_count === 0}>
          <PlayIcon className="size-4 mr-1" />
          {t("review.start-review")}
        </Button>
        <Button variant="outline" onClick={handleGenerate} disabled={generating}>
          <RefreshCwIcon className={`size-4 mr-1 ${generating ? "animate-spin" : ""}`} />
          {t("review.generate-cards")}
        </Button>
      </div>

      {/* 生成进度 */}
      {showProgress && generating && (
        <div className="rounded-lg border border-border p-3">
          <div className="text-sm font-medium mb-2">{t("review.generating")}</div>
          <div className="text-xs text-muted-foreground max-h-32 overflow-auto whitespace-pre-wrap">
            {progress}
          </div>
        </div>
      )}
      {result && (
        <div className="rounded-lg border border-green-500 p-3">
          <div className="text-sm text-green-600">
            {t("review.generated", { count: result.count })}
          </div>
        </div>
      )}

      {/* 卡片列表 */}
      <div>
        <h2 className="text-lg font-semibold mb-2">{t("review.cards")}</h2>
        <CardTable cards={cards} onRefresh={refreshCards} />
      </div>
    </div>
  );
};

export default DeckDetail;
