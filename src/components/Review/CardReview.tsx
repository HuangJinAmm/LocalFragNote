import { Button } from "@/components/ui/button";
import { ArrowLeftIcon, RotateCcwIcon } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useScoreCard } from "./hooks";
import type { ReviewCard, SessionStats } from "./types";
import { useTranslate } from "@/utils/i18n";

interface Props {
  deckId: number;
  onExit: () => void;
}

const CardReview = ({ deckId, onExit }: Props) => {
  const t = useTranslate();
  const [cards, setCards] = useState<ReviewCard[]>([]);
  const [currentIndex, setCurrentIndex] = useState(0);
  const [flipped, setFlipped] = useState(false);
  const [sessionStats, setSessionStats] = useState<SessionStats | null>(null);
  const [finished, setFinished] = useState(false);
  const { scoring, score } = useScoreCard();

  const loadCards = useCallback(async () => {
    const result = await invoke<ReviewCard[]>("review_list_due_cards", {
      deckId,
      limit: 100,
    });
    setCards(result);
    if (result.length === 0) {
      setFinished(true);
    }
  }, [deckId]);

  useEffect(() => {
    loadCards();
  }, [loadCards]);

  const currentCard = cards[currentIndex];

  const handleScore = async (rating: number) => {
    if (!currentCard || scoring) return;
    const result = await score(currentCard.id, rating, deckId);
    if (result) {
      setSessionStats(result.session_stats);
      setFlipped(false);
      if (currentIndex + 1 >= cards.length) {
        setFinished(true);
      } else {
        setCurrentIndex(currentIndex + 1);
      }
    }
  };

  const handleRegenerate = async () => {
    if (!currentCard) return;
    await invoke("review_regenerate_card", { cardId: currentCard.id });
  };

  // 键盘快捷键
  useEffect(() => {
    if (finished || !currentCard) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === " ") {
        e.preventDefault();
        if (!flipped) setFlipped(true);
      } else if (flipped) {
        if (e.key === "1") handleScore(1);
        else if (e.key === "2") handleScore(2);
        else if (e.key === "3") handleScore(3);
        else if (e.key === "4") handleScore(4);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [flipped, currentCard, finished]);

  if (finished) {
    return (
      <div className="flex flex-col items-center justify-center py-16 gap-6">
        <div className="text-2xl font-bold">{t("review.session-complete")}</div>
        {sessionStats && (
          <div className="grid grid-cols-2 sm:grid-cols-3 gap-4">
            <div className="rounded-lg border p-4 text-center">
              <div className="text-2xl font-bold">{sessionStats.reviewed}</div>
              <div className="text-xs text-muted-foreground">{t("review.reviewed")}</div>
            </div>
            <div className="rounded-lg border p-4 text-center">
              <div className="text-2xl font-bold text-red-600">{sessionStats.again}</div>
              <div className="text-xs text-muted-foreground">{t("review.again")}</div>
            </div>
            <div className="rounded-lg border p-4 text-center">
              <div className="text-2xl font-bold text-green-600">
                {(sessionStats.retention_rate * 100).toFixed(0)}%
              </div>
              <div className="text-xs text-muted-foreground">{t("review.retention")}</div>
            </div>
          </div>
        )}
        <div className="flex gap-2">
          <Button variant="outline" onClick={onExit}>
            {t("review.back-to-decks")}
          </Button>
        </div>
      </div>
    );
  }

  if (!currentCard) {
    return <div className="text-center py-8 text-muted-foreground">{t("common.loading")}</div>;
  }

  return (
    <div className="flex flex-col gap-4">
      {/* 顶部：返回 + 进度 */}
      <div className="flex items-center justify-between">
        <Button variant="ghost" size="sm" onClick={onExit}>
          <ArrowLeftIcon className="size-4 mr-1" />
          {t("common.back")}
        </Button>
        <div className="text-sm text-muted-foreground">
          {currentIndex + 1} / {cards.length}
        </div>
      </div>

      {/* 卡片 */}
      <div
        className="mx-auto w-full max-w-2xl min-h-[300px] rounded-lg border-2 border-border p-8 flex flex-col items-center justify-center cursor-pointer"
        style={{ perspective: "1000px" }}
        onClick={() => !flipped && setFlipped(true)}
      >
        {!flipped ? (
          <>
            <div className="text-xs text-muted-foreground mb-4">
              {t("review.card-type")}: {currentCard.card_type}
            </div>
            <div className="text-lg text-center whitespace-pre-wrap">{currentCard.front}</div>
            <div className="mt-8 text-sm text-muted-foreground">{t("review.click-to-flip")}</div>
          </>
        ) : (
          <>
            <div className="text-xs text-muted-foreground mb-4">{t("review.answer")}</div>
            <div className="text-lg text-center whitespace-pre-wrap">{currentCard.back}</div>
          </>
        )}
      </div>

      {/* 评分按钮 */}
      {flipped && (
        <div className="flex justify-center gap-2">
          <Button variant="destructive" onClick={() => handleScore(1)} disabled={scoring}>
            {t("review.again")} (1)
          </Button>
          <Button variant="outline" onClick={() => handleScore(2)} disabled={scoring}>
            {t("review.hard")} (2)
          </Button>
          <Button variant="default" onClick={() => handleScore(3)} disabled={scoring}>
            {t("review.good")} (3)
          </Button>
          <Button variant="default" onClick={() => handleScore(4)} disabled={scoring}>
            {t("review.easy")} (4)
          </Button>
        </div>
      )}

      {/* 换角度 */}
      {flipped && (
        <div className="flex justify-center">
          <Button variant="ghost" size="sm" onClick={handleRegenerate}>
            <RotateCcwIcon className="size-4 mr-1" />
            {t("review.regenerate-angle")}
          </Button>
        </div>
      )}
    </div>
  );
};

export default CardReview;
