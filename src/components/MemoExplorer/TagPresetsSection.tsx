import { BookmarkIcon, CheckIcon, PlusIcon, Trash2Icon, XIcon } from "lucide-react";
import { useState } from "react";
import { type MemoFilter, useMemoFilterContext } from "@/contexts/MemoFilterContext";
import { useTagPresets } from "@/hooks";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";

/// 标签预设区：保存当前选中的标签组合、应用预设、删除预设
const TagPresetsSection = () => {
  const { getFiltersByFactor, addFilter, removeFiltersByFactor } = useMemoFilterContext();
  const { presets, addPreset, removePreset } = useTagPresets();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [presetName, setPresetName] = useState("");

  const selectedTags = getFiltersByFactor("tagSearch").map((f: MemoFilter) => f.value);

  /// 保存当前选中标签为预设
  const handleSavePreset = () => {
    if (!presetName.trim() || selectedTags.length === 0) return;
    addPreset(presetName, selectedTags);
    setPresetName("");
    setDialogOpen(false);
  };

  /// 应用预设：清除当前标签过滤，应用预设的所有标签
  const handleApplyPreset = (tags: string[]) => {
    removeFiltersByFactor("tagSearch");
    for (const tag of tags) {
      addFilter({ factor: "tagSearch", value: tag });
    }
  };

  /// 判断预设是否与当前选中标签完全匹配（用于高亮）
  const isPresetActive = (tags: string[]): boolean => {
    if (selectedTags.length !== tags.length) return false;
    return tags.every((t) => selectedTags.includes(t));
  };

  /// 取消保存
  const handleCancelSave = () => {
    setPresetName("");
    setDialogOpen(false);
  };

  return (
    <div className="w-full flex flex-col justify-start items-start mt-3 px-1 h-auto shrink-0 flex-nowrap">
      <div className="flex flex-row justify-between items-center w-full gap-1 mb-1 text-sm leading-6 text-muted-foreground select-none">
        <span className="flex flex-row items-center gap-1">
          <BookmarkIcon className="w-4 h-auto" />
          <span>标签预设</span>
        </span>
        {/* 仅在有选中标签时允许保存为预设 */}
        {selectedTags.length > 0 && (
          <Button
            variant="ghost"
            size="sm"
            className="h-6 px-1.5 text-xs"
            onClick={() => setDialogOpen(true)}
            title="保存当前标签组合为预设"
          >
            <PlusIcon className="w-3.5 h-auto" />
            保存
          </Button>
        )}
      </div>

      {presets.length > 0 ? (
        <div className="w-full flex flex-col justify-start items-start gap-1">
          {presets.map((preset) => {
            const active = isPresetActive(preset.tags);
            return (
              <div
                key={preset.id}
                className={cn(
                  "group w-full flex flex-row justify-between items-center rounded-md px-2 py-1 text-sm cursor-pointer transition-colors select-none",
                  active ? "bg-primary/10 text-primary" : "text-muted-foreground hover:bg-accent hover:text-foreground",
                )}
                onClick={() => handleApplyPreset(preset.tags)}
              >
                <div className="flex flex-row items-center gap-1.5 truncate flex-1 min-w-0">
                  {active && <CheckIcon className="w-3.5 h-auto shrink-0" />}
                  <span className="truncate font-medium">{preset.name}</span>
                </div>
                <div className="flex flex-row items-center gap-1 shrink-0">
                  <span className="text-xs opacity-60">{preset.tags.length}</span>
                  <button
                    type="button"
                    className="opacity-0 group-hover:opacity-100 transition-opacity p-0.5 hover:text-destructive"
                    onClick={(e) => {
                      e.stopPropagation();
                      removePreset(preset.id);
                    }}
                    title="删除预设"
                  >
                    <Trash2Icon className="w-3.5 h-auto" />
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      ) : (
        <div className="p-2 border border-dashed rounded-md flex flex-row justify-start items-start gap-2 text-muted-foreground">
          <BookmarkIcon className="w-4 h-4 shrink-0 mt-0.5" />
          <p className="text-xs leading-snug italic">
            选中多个标签后点击"保存"，可创建标签组合预设，方便快速筛选。
          </p>
        </div>
      )}

      {/* 保存预设对话框 */}
      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>保存标签预设</DialogTitle>
          </DialogHeader>
          <div className="flex flex-col gap-3 py-2">
            <Input
              autoFocus
              placeholder="预设名称"
              value={presetName}
              onChange={(e) => setPresetName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleSavePreset();
                if (e.key === "Escape") handleCancelSave();
              }}
            />
            <div className="flex flex-wrap gap-1.5">
              {selectedTags.map((tag) => (
                <span
                  key={tag}
                  className="inline-flex items-center gap-0.5 rounded bg-secondary text-secondary-foreground px-1.5 py-0.5 text-xs"
                >
                  #{tag}
                </span>
              ))}
            </div>
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={handleCancelSave}>
              <XIcon className="w-4 h-auto mr-1" />
              取消
            </Button>
            <Button onClick={handleSavePreset} disabled={!presetName.trim() || selectedTags.length === 0}>
              <CheckIcon className="w-4 h-auto mr-1" />
              保存
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
};

export default TagPresetsSection;
