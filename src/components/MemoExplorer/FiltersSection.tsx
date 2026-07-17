import { BookmarkIcon, CheckIcon, PlusIcon, Trash2Icon, XIcon } from "lucide-react";
import { useMemo, useState } from "react";
import { parseCelToFilters } from "@/connect";
import { type FilterFactor, type MemoFilter, getMemoFilterKey, useMemoFilterContext } from "@/contexts/MemoFilterContext";
import { useFilterPresets } from "@/hooks";
import { cn } from "@/lib/utils";
import { useTranslate } from "@/utils/i18n";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

/// 过滤器类型配置:用于快捷表单的选项展示与值输入适配
interface QuickFilterType {
  factor: FilterFactor;
  label: string;
  /// 值输入类型:text 文本、date 日期、none 无值(标志位)、visibility 可见性下拉
  valueType: "text" | "date" | "none" | "visibility";
  placeholder?: string;
}

const QUICK_FILTER_TYPES: QuickFilterType[] = [
  { factor: "tagSearch", label: "标签", valueType: "text", placeholder: "输入标签名,回车添加多个" },
  { factor: "contentSearch", label: "关键词", valueType: "text", placeholder: "搜索关键词" },
  { factor: "semanticSearch", label: "语义搜索", valueType: "text", placeholder: "自然语言查询" },
  { factor: "displayTime", label: "日期", valueType: "date" },
  { factor: "fromDate", label: "起始日期", valueType: "date" },
  { factor: "pinned", label: "置顶", valueType: "none" },
  { factor: "property.hasLink", label: "含链接", valueType: "none" },
  { factor: "property.hasTaskList", label: "含待办", valueType: "none" },
  { factor: "property.hasCode", label: "含代码", valueType: "none" },
  { factor: "visibility", label: "可见性", valueType: "visibility" },
];

const VISIBILITY_OPTIONS = ["PUBLIC", "PROTECTED", "PRIVATE"];

/// 过滤器区:管理过滤器预设 + 添加过滤器弹窗(快捷表单 / CEL 表达式)
const FiltersSection = () => {
  const t = useTranslate();
  const { filters, setFilters, addFilter } = useMemoFilterContext();
  const { presets, addPreset, removePreset } = useFilterPresets();
  const [savePresetDialogOpen, setSavePresetDialogOpen] = useState(false);
  const [addFilterDialogOpen, setAddFilterDialogOpen] = useState(false);
  const [presetName, setPresetName] = useState("");

  /// 保存当前全部过滤器为预设
  const handleSavePreset = () => {
    if (!presetName.trim() || filters.length === 0) return;
    addPreset(presetName, filters);
    setPresetName("");
    setSavePresetDialogOpen(false);
  };

  /// 应用预设:直接替换当前过滤器
  const handleApplyPreset = (presetFilters: MemoFilter[]) => {
    setFilters(presetFilters);
  };

  /// 判断预设是否与当前过滤器完全匹配(用于高亮)
  const isPresetActive = (presetFilters: MemoFilter[]): boolean => {
    if (filters.length !== presetFilters.length) return false;
    const currentKeys = new Set(filters.map(getMemoFilterKey));
    return presetFilters.every((f) => currentKeys.has(getMemoFilterKey(f)));
  };

  const handleCancelSavePreset = () => {
    setPresetName("");
    setSavePresetDialogOpen(false);
  };

  /// 添加过滤器弹窗确认回调:把解析/构建的过滤器一次性加入
  const handleAddFilters = (newFilters: MemoFilter[]) => {
    newFilters.forEach((f) => addFilter(f));
    setAddFilterDialogOpen(false);
  };

  return (
    <div className="w-full flex flex-col justify-start items-start mt-3 px-1 h-auto shrink-0 flex-nowrap">
      <div className="flex flex-row justify-between items-center w-full gap-1 mb-1 text-sm leading-6 text-muted-foreground select-none">
        <span className="flex flex-row items-center gap-1">
          <BookmarkIcon className="w-4 h-auto" />
          <span>{t("memo.filters.label")}</span>
        </span>
        <div className="flex flex-row items-center gap-1">
          {/* 添加过滤器:打开快捷表单 / CEL 弹窗 */}
          <Button
            variant="ghost"
            size="sm"
            className="h-6 px-1.5 text-xs"
            onClick={() => setAddFilterDialogOpen(true)}
            title="添加过滤器"
          >
            <PlusIcon className="w-3.5 h-auto" />
            添加
          </Button>
          {/* 仅在有过滤器时允许保存为预设 */}
          {filters.length > 0 && (
            <Button
              variant="ghost"
              size="sm"
              className="h-6 px-1.5 text-xs"
              onClick={() => setSavePresetDialogOpen(true)}
              title="保存当前过滤器组合为预设"
            >
              <BookmarkIcon className="w-3.5 h-auto" />
              保存
            </Button>
          )}
        </div>
      </div>

      {presets.length > 0 ? (
        <div className="w-full flex flex-col justify-start items-start gap-1">
          {presets.map((preset) => {
            const active = isPresetActive(preset.filters);
            return (
              <div
                key={preset.id}
                className={cn(
                  "group w-full flex flex-row justify-between items-center rounded-md px-2 py-1 text-sm cursor-pointer transition-colors select-none",
                  active ? "bg-primary/10 text-primary" : "text-muted-foreground hover:bg-accent hover:text-foreground",
                )}
                onClick={() => handleApplyPreset(preset.filters)}
              >
                <div className="flex flex-row items-center gap-1.5 truncate flex-1 min-w-0">
                  {active && <CheckIcon className="w-3.5 h-auto shrink-0" />}
                  <span className="truncate font-medium">{preset.name}</span>
                </div>
                <div className="flex flex-row items-center gap-1 shrink-0">
                  <span className="text-xs opacity-60">{preset.filters.length}</span>
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
            点击"添加"创建过滤器,或选中过滤器后"保存"为预设,方便快速筛选。
          </p>
        </div>
      )}

      {/* 添加过滤器对话框 */}
      <AddFilterDialog
        open={addFilterDialogOpen}
        onOpenChange={setAddFilterDialogOpen}
        onConfirm={handleAddFilters}
      />

      {/* 保存预设对话框 */}
      <Dialog open={savePresetDialogOpen} onOpenChange={setSavePresetDialogOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>保存过滤器预设</DialogTitle>
          </DialogHeader>
          <div className="flex flex-col gap-3 py-2">
            <Input
              autoFocus
              placeholder="预设名称"
              value={presetName}
              onChange={(e) => setPresetName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleSavePreset();
                if (e.key === "Escape") handleCancelSavePreset();
              }}
            />
            <div className="flex flex-wrap gap-1.5">
              {filters.map((f) => (
                <span
                  key={getMemoFilterKey(f)}
                  className="inline-flex items-center gap-0.5 rounded bg-secondary text-secondary-foreground px-1.5 py-0.5 text-xs"
                >
                  {f.factor}:{f.value || "✓"}
                </span>
              ))}
            </div>
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={handleCancelSavePreset}>
              <XIcon className="w-4 h-auto mr-1" />
              取消
            </Button>
            <Button onClick={handleSavePreset} disabled={!presetName.trim() || filters.length === 0}>
              <CheckIcon className="w-4 h-auto mr-1" />
              保存
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
};

// ============ 添加过滤器对话框 ============

interface AddFilterDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: (filters: MemoFilter[]) => void;
}

type DialogMode = "quick" | "cel";

const AddFilterDialog = ({ open, onOpenChange, onConfirm }: AddFilterDialogProps) => {
  const t = useTranslate();
  const [mode, setMode] = useState<DialogMode>("quick");

  // 快捷模式状态
  const [selectedFactor, setSelectedFactor] = useState<FilterFactor>("tagSearch");
  const [textValue, setTextValue] = useState("");
  const [dateValue, setDateValue] = useState("");
  const [visibilityValue, setVisibilityValue] = useState("PUBLIC");
  // 多值累积(tagSearch 支持回车添加多个)
  const [tagValues, setTagValues] = useState<string[]>([]);

  // CEL 模式状态
  const [celInput, setCelInput] = useState("");

  const selectedType = QUICK_FILTER_TYPES.find((t) => t.factor === selectedFactor)!;

  /// 快捷模式:根据当前类型构建待添加过滤器
  const quickPreview = useMemo((): MemoFilter[] => {
    if (selectedType.valueType === "none") {
      return [{ factor: selectedFactor, value: "" }];
    }
    if (selectedType.valueType === "visibility") {
      return [{ factor: "visibility", value: visibilityValue }];
    }
    if (selectedType.valueType === "date") {
      return dateValue ? [{ factor: selectedFactor, value: dateValue }] : [];
    }
    // text: tagSearch 支持多个值
    if (selectedFactor === "tagSearch") {
      return tagValues.map((v) => ({ factor: "tagSearch", value: v }));
    }
    return textValue ? [{ factor: selectedFactor, value: textValue }] : [];
  }, [selectedType, selectedFactor, visibilityValue, dateValue, tagValues, textValue]);

  /// CEL 模式:实时解析预览
  const celPreview = useMemo(() => {
    if (!celInput.trim()) return { filters: [] as MemoFilter[], unrecognized: [] as string[] };
    const result = parseCelToFilters(celInput);
    return { filters: result.filters as MemoFilter[], unrecognized: result.unrecognized };
  }, [celInput]);

  /// 重置快捷模式输入(切换类型时)
  const handleFactorChange = (factor: string) => {
    setSelectedFactor(factor as FilterFactor);
    setTextValue("");
    setDateValue("");
    setTagValues([]);
  };

  /// tagSearch 回车添加
  const handleTagKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && selectedFactor === "tagSearch") {
      e.preventDefault();
      const v = textValue.trim();
      if (v && !tagValues.includes(v)) {
        setTagValues((prev) => [...prev, v]);
      }
      setTextValue("");
    }
  };

  const handleConfirm = () => {
    const filtersToAdd = mode === "quick" ? quickPreview : celPreview.filters;
    if (filtersToAdd.length === 0) return;
    onConfirm(filtersToAdd);
    // 重置
    setTextValue("");
    setDateValue("");
    setTagValues([]);
    setCelInput("");
  };

  const handleCancel = () => {
    onOpenChange(false);
    setTextValue("");
    setDateValue("");
    setTagValues([]);
    setCelInput("");
  };

  const canConfirm = mode === "quick" ? quickPreview.length > 0 : celPreview.filters.length > 0;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>添加过滤器</DialogTitle>
        </DialogHeader>

        {/* 模式切换 */}
        <div className="flex gap-1 p-1 bg-muted rounded-md text-sm">
          <button
            type="button"
            className={cn(
              "flex-1 px-3 py-1 rounded transition-colors",
              mode === "quick" ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground",
            )}
            onClick={() => setMode("quick")}
          >
            快捷
          </button>
          <button
            type="button"
            className={cn(
              "flex-1 px-3 py-1 rounded transition-colors",
              mode === "cel" ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground",
            )}
            onClick={() => setMode("cel")}
          >
            CEL 表达式
          </button>
        </div>

        {mode === "quick" ? (
          <div className="flex flex-col gap-3 py-1">
            <div className="flex flex-col gap-1.5">
              <Label>类型</Label>
              <Select value={selectedFactor} onValueChange={handleFactorChange}>
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {QUICK_FILTER_TYPES.map((ft) => (
                    <SelectItem key={ft.factor} value={ft.factor}>
                      {ft.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            {selectedType.valueType === "text" && (
              <div className="flex flex-col gap-1.5">
                <Label>值</Label>
                <Input
                  autoFocus
                  placeholder={selectedType.placeholder}
                  value={textValue}
                  onChange={(e) => setTextValue(e.target.value)}
                  onKeyDown={handleTagKeyDown}
                />
                {selectedFactor === "tagSearch" && tagValues.length > 0 && (
                  <div className="flex flex-wrap gap-1.5 mt-1">
                    {tagValues.map((tag) => (
                      <span
                        key={tag}
                        className="inline-flex items-center gap-1 rounded bg-secondary text-secondary-foreground px-1.5 py-0.5 text-xs"
                      >
                        #{tag}
                        <button
                          type="button"
                          className="hover:text-destructive"
                          onClick={() => setTagValues((prev) => prev.filter((v) => v !== tag))}
                        >
                          <XIcon className="w-3 h-3" />
                        </button>
                      </span>
                    ))}
                  </div>
                )}
                {selectedFactor === "tagSearch" && (
                  <p className="text-xs text-muted-foreground">按回车添加多个标签</p>
                )}
              </div>
            )}

            {selectedType.valueType === "date" && (
              <div className="flex flex-col gap-1.5">
                <Label>日期</Label>
                <Input
                  type="date"
                  autoFocus
                  value={dateValue}
                  onChange={(e) => setDateValue(e.target.value)}
                />
              </div>
            )}

            {selectedType.valueType === "visibility" && (
              <div className="flex flex-col gap-1.5">
                <Label>可见性</Label>
                <Select value={visibilityValue} onValueChange={setVisibilityValue}>
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {VISIBILITY_OPTIONS.map((v) => (
                      <SelectItem key={v} value={v}>
                        {v}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            )}

            {selectedType.valueType === "none" && (
              <p className="text-sm text-muted-foreground">该过滤器无需输入值,直接添加即可。</p>
            )}

            {/* 预览 */}
            {quickPreview.length > 0 && (
              <div className="flex flex-wrap gap-1.5 p-2 bg-muted/50 rounded-md">
                {quickPreview.map((f) => (
                  <span
                    key={getMemoFilterKey(f)}
                    className="inline-flex items-center rounded bg-secondary text-secondary-foreground px-1.5 py-0.5 text-xs"
                  >
                    {f.factor}:{f.value || "✓"}
                  </span>
                ))}
              </div>
            )}
          </div>
        ) : (
          <div className="flex flex-col gap-3 py-1">
            <div className="flex flex-col gap-1.5">
              <Label>CEL 表达式</Label>
              <textarea
                autoFocus
                placeholder={'例如:tag in ["工作", "重要"] && pinned'}
                value={celInput}
                onChange={(e) => setCelInput(e.target.value)}
                className="w-full min-h-[80px] text-sm font-mono bg-transparent border border-border rounded-md p-2 outline-0 focus:border-primary/50 transition-colors resize-y"
              />
            </div>
            <p className="text-xs text-muted-foreground leading-relaxed">
              支持语法:<code>fts.match("关键词")</code>、<code>semantic.search("查询")</code>、
              <code>tag in ["a","b"]</code>、<code>visibility in ["PUBLIC"]</code>、
              <code>created_ts &gt;= timestamp(N)</code>、<code>pinned</code>、<code>has_link</code> 等,用 <code>&amp;&amp;</code> 连接。
            </p>

            {/* 解析预览 */}
            {celPreview.filters.length > 0 && (
              <div className="flex flex-col gap-1.5">
                <span className="text-xs text-muted-foreground">解析结果:</span>
                <div className="flex flex-wrap gap-1.5 p-2 bg-muted/50 rounded-md">
                  {celPreview.filters.map((f, i) => (
                    <span
                      key={`${f.factor}-${f.value}-${i}`}
                      className="inline-flex items-center rounded bg-secondary text-secondary-foreground px-1.5 py-0.5 text-xs"
                    >
                      {f.factor}:{f.value || "✓"}
                    </span>
                  ))}
                </div>
              </div>
            )}
            {celPreview.unrecognized.length > 0 && (
              <p className="text-xs text-warning">
                未识别的条件: {celPreview.unrecognized.join(" | ")
                }
              </p>
            )}
          </div>
        )}

        <DialogFooter>
          <Button variant="ghost" onClick={handleCancel}>
            <XIcon className="w-4 h-auto mr-1" />
            {t("common.cancel")}
          </Button>
          <Button onClick={handleConfirm} disabled={!canConfirm}>
            <CheckIcon className="w-4 h-auto mr-1" />
            {t("common.add")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

export default FiltersSection;
