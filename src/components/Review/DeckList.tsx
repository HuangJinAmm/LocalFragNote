import { Button } from "@/components/ui/button";
import { BookOpenIcon, PlusIcon, RefreshCwIcon, Trash2Icon, PlayIcon } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useGenerateCards, useReviewDecks } from "./hooks";
import type { ReviewDeck } from "./types";
import { useTranslate } from "@/utils/i18n";
import DeckEditor from "./DeckEditor";
import ReviewHeatmap from "./ReviewHeatmap";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  onSelectDeck: (deck: ReviewDeck) => void;
}

const DeckList = ({ onSelectDeck }: Props) => {
  const t = useTranslate();
  const navigate = useNavigate();
  const { decks, loading, refresh } = useReviewDecks();
  const [editorOpen, setEditorOpen] = useState(false);
  const [generatingDeckId, setGeneratingDeckId] = useState<number | null>(null);

  const { generating, progress, result, generate } = useGenerateCards(generatingDeckId ?? 0);

  const handleCreate = async (data: { name: string; tags: string[]; cards_per_memo: number }) => {
    await invoke("review_create_deck", data);
    setEditorOpen(false);
    refresh();
  };

  const handleDelete = async (deckId: number) => {
    if (!confirm(t("review.confirm-delete-deck"))) return;
    await invoke("review_delete_deck", { id: deckId });
    refresh();
  };

  const handleGenerate = async (deckId: number) => {
    setGeneratingDeckId(deckId);
    await generate();
  };

  if (loading) {
    return <div className="text-center text-muted-foreground py-8">{t("common.loading")}</div>;
  }

  if (decks.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-16 gap-4">
        <BookOpenIcon className="size-12 text-muted-foreground" />
        <p className="text-muted-foreground">{t("review.no-decks")}</p>
        <Button onClick={() => setEditorOpen(true)}>
          <PlusIcon className="size-4 mr-2" />
          {t("review.create-deck")}
        </Button>
        <DeckEditor open={editorOpen} onOpenChange={setEditorOpen} onSubmit={handleCreate} />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <ReviewHeatmap />
      <div className="flex justify-end">
        <Button onClick={() => setEditorOpen(true)} size="sm">
          <PlusIcon className="size-4 mr-2" />
          {t("review.create-deck")}
        </Button>
      </div>
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
        {decks.map((deck) => (
          <div
            key={deck.id}
            className="rounded-lg border border-border p-4 flex flex-col gap-3 cursor-pointer hover:border-primary transition-colors"
            onClick={() => onSelectDeck(deck)}
          >
            <div className="flex items-start justify-between">
              <div>
                <h3 className="font-semibold text-foreground">{deck.name}</h3>
                <div className="flex flex-wrap gap-1 mt-1">
                  {deck.tags.map((tag) => (
                    <span key={tag} className="text-xs text-muted-foreground">
                      #{tag}
                    </span>
                  ))}
                </div>
              </div>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  handleDelete(deck.id);
                }}
                className="text-muted-foreground hover:text-destructive"
              >
                <Trash2Icon className="size-4" />
              </button>
            </div>
            <div className="flex gap-2 mt-auto">
              <Button
                size="sm"
                variant="default"
                onClick={(e) => {
                  e.stopPropagation();
                  navigate(`/review/${deck.id}/study`);
                }}
              >
                <PlayIcon className="size-4 mr-1" />
                {t("review.start-review")}
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={(e) => {
                  e.stopPropagation();
                  handleGenerate(deck.id);
                }}
                disabled={generating && generatingDeckId === deck.id}
              >
                <RefreshCwIcon className={`size-4 mr-1 ${generating && generatingDeckId === deck.id ? "animate-spin" : ""}`} />
                {t("review.generate-cards")}
              </Button>
            </div>
          </div>
        ))}
      </div>
      {generatingDeckId !== null && generating && (
        <div className="fixed bottom-4 right-4 max-w-md rounded-lg border border-border bg-background p-4 shadow-lg">
          <div className="text-sm font-medium mb-2">{t("review.generating")}</div>
          <div className="text-xs text-muted-foreground max-h-32 overflow-auto">{progress}</div>
        </div>
      )}
      {result && (
        <div className="fixed bottom-4 right-4 rounded-lg border border-green-500 bg-background p-4 shadow-lg">
          <div className="text-sm font-medium text-green-600">
            {t("review.generated", { count: result.count })}
          </div>
          {result.errors.length > 0 && (
            <div className="text-xs text-destructive mt-1">
              {result.errors.length} {t("review.errors")}
            </div>
          )}
        </div>
      )}
      <DeckEditor open={editorOpen} onOpenChange={setEditorOpen} onSubmit={handleCreate} />
    </div>
  );
};

export default DeckList;
