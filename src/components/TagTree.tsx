import { ChevronRightIcon, HashIcon } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { type MemoFilter, useMemoFilterContext } from "@/contexts/MemoFilterContext";
import { useTranslate } from "@/utils/i18n";

interface Tag {
  key: string;
  text: string;
  amount: number;
  subTags: Tag[];
}

interface Props {
  tagAmounts: [tag: string, amount: number][];
  expandSubTags: boolean;
}

/// 树模式根节点分页大小,避免一次渲染过多根级标签
const TREE_ROOT_PAGE_SIZE = 50;

const TagTree = ({ tagAmounts: rawTagAmounts, expandSubTags }: Props) => {
  const t = useTranslate();
  const [tags, setTags] = useState<Tag[]>([]);
  const [rootLimit, setRootLimit] = useState(TREE_ROOT_PAGE_SIZE);

  useEffect(() => {
    // 用 Map 加速查找,避免 O(n²) 的 .some() 嵌套循环
    const amountMap = new Map<string, number>();
    for (const [tag, amount] of rawTagAmounts) {
      amountMap.set(tag, amount);
    }

    const sortedTagAmounts = Array.from(rawTagAmounts).sort(([a], [b]) => a.localeCompare(b));
    const root: Tag = {
      key: "",
      text: "",
      amount: 0,
      subTags: [],
    };

    // 缓存每个路径节点,避免内层 subTags 数组线性查找
    const nodeByPath = new Map<string, Tag>();
    nodeByPath.set("", root);

    for (const tagAmount of sortedTagAmounts) {
      const subtags = tagAmount[0].split("/");
      let tempObj = root;
      let tagText = "";

      for (let i = 0; i < subtags.length; i++) {
        const key = subtags[i];
        tagText = i === 0 ? key : `${tagText}/${key}`;

        let obj = nodeByPath.get(tagText);
        if (!obj) {
          // 仅当此路径在 amountMap 中存在且数量 > 1 时显示数量
          const storedAmount = amountMap.get(tagText);
          const amount = storedAmount !== undefined && storedAmount > 1 ? tagAmount[1] : 0;
          obj = {
            key,
            text: tagText,
            amount,
            subTags: [],
          };
          nodeByPath.set(tagText, obj);
          tempObj.subTags.push(obj);
        }

        tempObj = obj;
      }
    }

    setTags(root.subTags as Tag[]);
    setRootLimit(TREE_ROOT_PAGE_SIZE);
  }, [rawTagAmounts]);

  const visibleRootTags = tags.slice(0, rootLimit);
  const remainingCount = tags.length - rootLimit;

  return (
    <div className="flex flex-col justify-start items-start relative w-full h-auto flex-nowrap gap-2 mt-1">
      {visibleRootTags.map((t, idx) => (
        <TagItemContainer key={t.text + "-" + idx} tag={t} expandSubTags={expandSubTags} />
      ))}
      {remainingCount > 0 && (
        <button
          type="button"
          onClick={() => setRootLimit((n) => n + TREE_ROOT_PAGE_SIZE)}
          className="w-full text-xs text-muted-foreground hover:text-foreground border border-dashed border-border rounded-md py-1 transition-colors hover:bg-accent"
        >
          {t("common.expand")} (+{Math.min(TREE_ROOT_PAGE_SIZE, remainingCount)} / {remainingCount})
        </button>
      )}
    </div>
  );
};

interface TagItemContainerProps {
  tag: Tag;
  expandSubTags: boolean;
}

const TagItemContainer = (props: TagItemContainerProps) => {
  const { tag, expandSubTags } = props;
  const { getFiltersByFactor, addFilter, removeFilter } = useMemoFilterContext();
  const tagFilters = getFiltersByFactor("tagSearch");
  const isActive = tagFilters.some((f: MemoFilter) => f.value === tag.text);
  const hasSubTags = tag.subTags.length > 0;
  const [showSubTags, setShowSubTags] = useState(false);

  useEffect(() => {
    setShowSubTags(expandSubTags);
  }, [expandSubTags]);

  const handleTagClick = () => {
    if (isActive) {
      removeFilter((f: MemoFilter) => f.factor === "tagSearch" && f.value === tag.text);
    } else {
      // 多选模式：直接添加，不移除已有标签
      addFilter({
        factor: "tagSearch",
        value: tag.text,
      });
    }
  };

  const handleToggleBtnClick = useCallback((event: React.MouseEvent) => {
    event.stopPropagation();
    setShowSubTags((current) => !current);
  }, []);

  return (
    <>
      <div className="relative flex flex-row justify-between items-center w-full leading-6 py-0 mt-px text-sm select-none shrink-0">
        <div
          className={`flex flex-row justify-start items-center truncate shrink leading-5 mr-1 cursor-pointer transition-colors ${
            isActive ? "text-primary" : "text-muted-foreground"
          }`}
          onClick={handleTagClick}
        >
          <HashIcon className="w-4 h-auto shrink-0 mr-1" />
          <span className={`truncate hover:opacity-80 ${isActive ? "font-medium" : ""}`}>
            {tag.key} {tag.amount > 1 && <span className="opacity-60">({tag.amount})</span>}
          </span>
        </div>
        <div className="flex flex-row justify-end items-center">
          {hasSubTags ? (
            <span
              className={`flex flex-row justify-center items-center w-6 h-6 shrink-0 transition-all rotate-0 cursor-pointer ${
                showSubTags && "rotate-90"
              }`}
              onClick={handleToggleBtnClick}
            >
              <ChevronRightIcon className="w-5 h-5 text-muted-foreground hover:text-foreground" />
            </span>
          ) : null}
        </div>
      </div>
      {hasSubTags ? (
        <div
          className={`w-[calc(100%-0.5rem)] flex flex-col justify-start items-start h-auto ml-2 pl-2 border-l-2 border-l-border ${
            !showSubTags && "hidden"
          }`}
        >
          {tag.subTags.map((st, idx) => (
            <TagItemContainer key={st.text + "-" + idx} tag={st} expandSubTags={expandSubTags} />
          ))}
        </div>
      ) : null}
    </>
  );
};

export default TagTree;
