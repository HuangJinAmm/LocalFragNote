import { useEffect, useMemo, useState } from "react";
import { HashIcon, MoreVerticalIcon, SearchIcon, TagsIcon, XIcon } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { type MemoFilter, useMemoFilterContext } from "@/contexts/MemoFilterContext";
import { useDebouncedEffect, useLocalStorage } from "@/hooks";
import { cn } from "@/lib/utils";
import { useTranslate } from "@/utils/i18n";
import TagTree from "../TagTree";
import { Popover, PopoverContent, PopoverTrigger } from "../ui/popover";

interface Props {
  readonly?: boolean;
  tagCount: Record<string, number>;
}

type TagSortMode = "count" | "alpha";

/// 扁平模式下默认可见标签数量,避免一次渲染过多 DOM 节点
const FLAT_PAGE_SIZE = 50;

const TagsSection = (props: Props) => {
  const t = useTranslate();
  const { getFiltersByFactor, addFilter, removeFilter } = useMemoFilterContext();
  const [treeMode, setTreeMode] = useLocalStorage<boolean>("tag-view-as-tree", false);
  const [treeAutoExpand, setTreeAutoExpand] = useLocalStorage<boolean>("tag-tree-auto-expand", false);
  const [sortMode, setSortMode] = useLocalStorage<TagSortMode>("tag-sort-mode", "count");

  // 标签搜索:输入即时本地过滤
  const [searchInput, setSearchInput] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  useDebouncedEffect(() => {
    setDebouncedQuery(searchInput.trim().toLowerCase());
  }, 200, [searchInput]);

  // 扁平模式下展开显示的标签数量(分页累积)
  const [visibleLimit, setVisibleLimit] = useState(FLAT_PAGE_SIZE);

  // 排序后的全部标签:先按名称字典序,再按数量降序(默认);切换为纯字典序
  const allSortedTags = useMemo(() => {
    const entries = Object.entries(props.tagCount);
    if (sortMode === "alpha") {
      return entries.sort((a, b) => a[0].localeCompare(b[0]));
    }
    return entries.sort((a, b) => a[0].localeCompare(b[0])).sort((a, b) => b[1] - a[1]);
  }, [props.tagCount, sortMode]);

  // 应用搜索过滤
  const filteredTags = useMemo(() => {
    if (!debouncedQuery) return allSortedTags;
    return allSortedTags.filter(([tag]) => tag.toLowerCase().includes(debouncedQuery));
  }, [allSortedTags, debouncedQuery]);

  // 搜索/排序变化时重置分页
  useEffect(() => {
    setVisibleLimit(FLAT_PAGE_SIZE);
  }, [debouncedQuery, sortMode]);

  const handleTagClick = (tag: string) => {
    const isActive = getFiltersByFactor("tagSearch").some((filter: MemoFilter) => filter.value === tag);
    if (isActive) {
      removeFilter((f: MemoFilter) => f.factor === "tagSearch" && f.value === tag);
    } else {
      // 多选模式:直接添加,不移除已有标签
      addFilter({
        factor: "tagSearch",
        value: tag,
      });
    }
  };

  const totalTagCount = allSortedTags.length;
  const hasFilter = debouncedQuery.length > 0;

  return (
    <div className="w-full flex flex-col justify-start items-start mt-3 px-1 h-auto shrink-0 flex-nowrap">
      <div className="flex flex-row justify-between items-center w-full gap-1 mb-1 text-sm leading-6 text-muted-foreground select-none">
        <span className="flex items-center gap-1">
          {t("common.tags")}
          {totalTagCount > 0 && <span className="opacity-60 text-xs">({totalTagCount})</span>}
        </span>
        {totalTagCount > 0 && (
          <Popover>
            <PopoverTrigger>
              <MoreVerticalIcon className="w-4 h-auto shrink-0 text-muted-foreground cursor-pointer hover:text-foreground" />
            </PopoverTrigger>
            <PopoverContent align="end" alignOffset={-12}>
              <div className="w-auto flex flex-row justify-between items-center gap-2 p-1">
                <span className="text-sm shrink-0">{t("common.tree-mode")}</span>
                <Switch checked={treeMode} onCheckedChange={(checked) => setTreeMode(checked)} />
              </div>
              <div className="w-auto flex flex-row justify-between items-center gap-2 p-1">
                <span className="text-sm shrink-0">{t("common.auto-expand")}</span>
                <Switch disabled={!treeMode} checked={treeAutoExpand} onCheckedChange={(checked) => setTreeAutoExpand(checked)} />
              </div>
              <div className="w-auto flex flex-row justify-between items-center gap-2 p-1">
                <span className="text-sm shrink-0">按数量排序</span>
                <Switch
                  checked={sortMode === "count"}
                  onCheckedChange={(checked) => setSortMode(checked ? "count" : "alpha")}
                />
              </div>
            </PopoverContent>
          </Popover>
        )}
      </div>

      {/* 标签搜索框:仅在标签数量较多时显示,减少视觉噪音 */}
      {totalTagCount > FLAT_PAGE_SIZE && (
        <div className="relative w-full mb-1.5">
          <SearchIcon className="absolute left-2 top-1/2 -translate-y-1/2 w-3.5 h-3.5 opacity-40 text-muted-foreground" />
          <input
            type="text"
            value={searchInput}
            onChange={(e) => setSearchInput(e.target.value)}
            placeholder={t("common.search")}
            className="w-full text-sm leading-6 bg-transparent border border-border rounded-md pl-7 pr-7 py-0.5 outline-0 focus:border-primary/50 transition-colors text-foreground placeholder:text-muted-foreground/60"
          />
          {searchInput && (
            <button
              type="button"
              onClick={() => setSearchInput("")}
              aria-label={t("common.clear")}
              className="absolute right-1.5 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
            >
              <XIcon className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
      )}

      {totalTagCount > 0 ? (
        filteredTags.length === 0 ? (
          <div className="w-full text-sm text-muted-foreground italic py-2 text-center">
            {hasFilter ? t("tag.no-tag-found") : null}
          </div>
        ) : treeMode ? (
          <TagTree tagAmounts={filteredTags} expandSubTags={!!treeAutoExpand} />
        ) : (
          <>
            <div className="w-full flex flex-row justify-start items-center relative flex-wrap gap-x-2 gap-y-1.5">
              {filteredTags.slice(0, visibleLimit).map(([tag, amount]) => {
                const isActive = getFiltersByFactor("tagSearch").some((filter: MemoFilter) => filter.value === tag);
                return (
                  <div
                    key={tag}
                    className={cn(
                      "shrink-0 w-auto max-w-full text-sm rounded-md leading-6 flex flex-row justify-start items-center select-none cursor-pointer transition-colors",
                      "hover:opacity-80",
                      isActive ? "text-primary" : "text-muted-foreground",
                    )}
                    onClick={() => handleTagClick(tag)}
                  >
                    <HashIcon className="w-4 h-auto shrink-0" />
                    <div className="inline-flex flex-nowrap ml-0.5 gap-0.5 max-w-[calc(100%-16px)]">
                      <span className={cn("truncate", isActive ? "font-medium" : "")}>{tag}</span>
                      {amount > 1 && <span className="opacity-60 shrink-0">({amount})</span>}
                    </div>
                  </div>
                );
              })}
            </div>
            {filteredTags.length > visibleLimit && (
              <button
                type="button"
                onClick={() => setVisibleLimit((n) => n + FLAT_PAGE_SIZE)}
                className="mt-1.5 w-full text-xs text-muted-foreground hover:text-foreground border border-dashed border-border rounded-md py-1 transition-colors hover:bg-accent"
              >
                {t("common.expand")} (+{Math.min(FLAT_PAGE_SIZE, filteredTags.length - visibleLimit)} / {filteredTags.length - visibleLimit})
              </button>
            )}
            {hasFilter && filteredTags.length > 0 && (
              <div className="mt-1 w-full text-xs text-muted-foreground/70 text-center">
                {filteredTags.length} / {totalTagCount}
              </div>
            )}
          </>
        )
      ) : (
        !props.readonly && (
          <div className="p-2 border border-dashed rounded-md flex flex-row justify-start items-start gap-2 text-muted-foreground">
            <TagsIcon className="w-5 h-5 shrink-0" />
            <p className="text-sm leading-snug italic">{t("tag.create-tags-guide")}</p>
          </div>
        )
      )}
    </div>
  );
};

export default TagsSection;
