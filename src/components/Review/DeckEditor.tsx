import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { X, Plus } from "lucide-react";
import { useState } from "react";
import { useTranslate } from "@/utils/i18n";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  initial?: { id: number; name: string; tags: string[]; cards_per_memo: number } | null;
  onSubmit: (data: { name: string; tags: string[]; cards_per_memo: number }) => void;
}

const DeckEditor = ({ open, onOpenChange, initial, onSubmit }: Props) => {
  const t = useTranslate();
  const [name, setName] = useState(initial?.name ?? "");
  const [tags, setTags] = useState<string[]>(initial?.tags ?? []);
  const [tagInput, setTagInput] = useState("");
  const [cardsPerMemo, setCardsPerMemo] = useState(initial?.cards_per_memo ?? 2);

  const addTag = () => {
    const trimmed = tagInput.trim().replace(/^#/, "");
    if (trimmed && !tags.includes(trimmed)) {
      setTags([...tags, trimmed]);
    }
    setTagInput("");
  };

  const removeTag = (tag: string) => {
    setTags(tags.filter((t) => t !== tag));
  };

  const handleSubmit = () => {
    if (!name.trim()) return;
    onSubmit({ name: name.trim(), tags, cards_per_memo: cardsPerMemo });
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{initial ? t("review.edit-deck") : t("review.create-deck")}</DialogTitle>
        </DialogHeader>
        <div className="space-y-4 py-4">
          <div className="space-y-2">
            <Label>{t("review.deck-name")}</Label>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("review.deck-name-placeholder")}
            />
          </div>
          <div className="space-y-2">
            <Label>{t("review.deck-tags")}</Label>
            <div className="flex gap-2">
              <Input
                value={tagInput}
                onChange={(e) => setTagInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    addTag();
                  }
                }}
                placeholder={t("review.deck-tags-placeholder")}
              />
              <Button type="button" variant="outline" onClick={addTag}>
                <Plus className="size-4" />
              </Button>
            </div>
            {tags.length > 0 && (
              <div className="flex flex-wrap gap-2 mt-2">
                {tags.map((tag) => (
                  <span
                    key={tag}
                    className="inline-flex items-center gap-1 rounded-md bg-secondary px-2 py-1 text-sm"
                  >
                    #{tag}
                    <button onClick={() => removeTag(tag)} className="hover:text-destructive">
                      <X className="size-3" />
                    </button>
                  </span>
                ))}
              </div>
            )}
          </div>
          <div className="space-y-2">
            <Label>{t("review.cards-per-memo")}</Label>
            <Input
              type="number"
              min={1}
              max={10}
              value={cardsPerMemo}
              onChange={(e) => setCardsPerMemo(Number(e.target.value) || 1)}
            />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t("common.cancel")}
          </Button>
          <Button onClick={handleSubmit} disabled={!name.trim()}>
            {t("common.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

export default DeckEditor;
