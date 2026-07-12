import { Loader2Icon } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useTranslate } from "@/utils/i18n";

interface TagSuggestionDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  loading: boolean;
  suggestedTags: string[];
  existingTags: string[];
  onConfirm: (selectedTags: string[]) => void;
  onSkip: () => void;
}

const TagSuggestionDialog = ({
  open,
  onOpenChange,
  loading,
  suggestedTags,
  existingTags,
  onConfirm,
  onSkip,
}: TagSuggestionDialogProps) => {
  const t = useTranslate();
  const [selected, setSelected] = useState<Set<string>>(new Set());

  // Reset selection when dialog reopens with new suggestions
  const handleOpenChange = (next: boolean) => {
    if (!next) {
      setSelected(new Set());
    }
    onOpenChange(next);
  };

  const toggleTag = (tag: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(tag)) {
        next.delete(tag);
      } else {
        next.add(tag);
      }
      return next;
    });
  };

  const handleConfirm = () => {
    onConfirm(Array.from(selected));
    setSelected(new Set());
  };

  const handleSkip = () => {
    onSkip();
    setSelected(new Set());
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("editor.auto-tag.dialog-title")}</DialogTitle>
          <DialogDescription>{t("editor.auto-tag.dialog-description")}</DialogDescription>
        </DialogHeader>

        {loading ? (
          <div className="flex items-center justify-center py-8 gap-2 text-muted-foreground">
            <Loader2Icon className="size-4 animate-spin" />
            <span className="text-sm">{t("editor.auto-tag.suggesting")}</span>
          </div>
        ) : suggestedTags.length === 0 ? (
          <p className="py-4 text-sm text-muted-foreground text-center">
            {t("editor.auto-tag.no-suggestions")}
          </p>
        ) : (
          <div className="space-y-4 py-2">
            {existingTags.length > 0 && (
              <div className="space-y-2">
                <p className="text-xs font-medium text-muted-foreground">
                  {t("editor.auto-tag.existing-tags")}
                </p>
                <div className="flex flex-wrap gap-1.5">
                  {existingTags.map((tag) => (
                    <span
                      key={tag}
                      className="inline-flex items-center rounded-md bg-muted px-2 py-0.5 text-xs text-muted-foreground"
                    >
                      #{tag}
                    </span>
                  ))}
                </div>
              </div>
            )}
            <div className="space-y-2">
              <p className="text-xs font-medium text-muted-foreground">
                {t("editor.auto-tag.suggested-tags")}
              </p>
              <div className="space-y-1.5">
                {suggestedTags.map((tag) => (
                  <label
                    key={tag}
                    className="flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-accent cursor-pointer"
                  >
                    <Checkbox
                      checked={selected.has(tag)}
                      onCheckedChange={() => toggleTag(tag)}
                    />
                    <span className="text-sm">#{tag}</span>
                  </label>
                ))}
              </div>
            </div>
          </div>
        )}

        <DialogFooter className="gap-2">
          <DialogClose asChild>
            <Button variant="ghost" onClick={handleSkip}>
              {t("editor.auto-tag.save-without-tags")}
            </Button>
          </DialogClose>
          {!loading && suggestedTags.length > 0 && (
            <Button onClick={handleConfirm} disabled={selected.size === 0}>
              {t("editor.auto-tag.add-and-save")}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

export default TagSuggestionDialog;
