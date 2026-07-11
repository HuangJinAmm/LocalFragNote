import { Button } from "@/components/ui/button";
import { Trash2Icon } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import type { FC } from "react";
import { CARD_STATE_LABELS, CARD_TYPE_LABELS, type ReviewCard } from "./types";
import { useTranslate } from "@/utils/i18n";

interface Props {
  cards: ReviewCard[];
  onRefresh: () => void;
}

const CardTable: FC<Props> = ({ cards, onRefresh }) => {
  const t = useTranslate();

  const handleDelete = async (cardId: number) => {
    if (!confirm(t("review.confirm-delete-card"))) return;
    await invoke("review_delete_card", { cardId });
    onRefresh();
  };

  const formatDate = (ts: number) => {
    return new Date(ts * 1000).toLocaleDateString();
  };

  if (cards.length === 0) {
    return (
      <div className="text-center py-8 text-muted-foreground">{t("review.no-cards")}</div>
    );
  }

  return (
    <div className="rounded-lg border border-border overflow-hidden">
      <table className="w-full text-sm">
        <thead className="bg-muted">
          <tr>
            <th className="text-left p-2">{t("review.front")}</th>
            <th className="text-left p-2">{t("review.card-type")}</th>
            <th className="text-left p-2">{t("review.angle")}</th>
            <th className="text-left p-2">{t("review.due")}</th>
            <th className="text-left p-2">{t("review.state")}</th>
            <th className="text-left p-2">{t("review.reps")}</th>
            <th className="p-2"></th>
          </tr>
        </thead>
        <tbody>
          {cards.map((card) => (
            <tr key={card.id} className="border-t border-border">
              <td className="p-2 max-w-xs truncate">{card.front}</td>
              <td className="p-2">{CARD_TYPE_LABELS[card.card_type] ?? card.card_type}</td>
              <td className="p-2">{card.angle || "-"}</td>
              <td className="p-2">{formatDate(card.due)}</td>
              <td className="p-2">{CARD_STATE_LABELS[card.state] ?? card.state}</td>
              <td className="p-2">{card.reps}</td>
              <td className="p-2">
                <button
                  onClick={() => handleDelete(card.id)}
                  className="text-muted-foreground hover:text-destructive"
                >
                  <Trash2Icon className="size-4" />
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
};

export default CardTable;
