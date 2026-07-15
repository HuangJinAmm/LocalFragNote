import dayjs from "dayjs";
import { SearchIcon, SparklesIcon, TypeIcon } from "lucide-react";
import { useRef, useState } from "react";
import { useMemoFilterContext } from "@/contexts/MemoFilterContext";
import type { MemoFilter } from "@/contexts/MemoFilterContext";
import { useTranslate } from "@/utils/i18n";
import MemoDisplaySettingMenu from "./MemoDisplaySettingMenu";

/** 解析 from: 日期表达式，返回 ISO 日期字符串 (YYYY-MM-DD) */
function parseFromDateExpr(expr: string): string | null {
  const lower = expr.toLowerCase();
  const now = dayjs();

  if (lower === "today") return now.format("YYYY-MM-DD");
  if (lower === "yesterday") return now.subtract(1, "day").format("YYYY-MM-DD");

  // Nd = N days ago, Nw = N weeks ago
  const matchD = lower.match(/^(\d+)d$/);
  if (matchD) return now.subtract(parseInt(matchD[1], 10), "day").format("YYYY-MM-DD");

  const matchW = lower.match(/^(\d+)w$/);
  if (matchW) return now.subtract(parseInt(matchW[1], 10), "week").format("YYYY-MM-DD");

  // YYYY-MM-DD 格式
  const parsed = dayjs(expr);
  if (parsed.isValid()) return parsed.format("YYYY-MM-DD");

  return null;
}

/** 解析搜索语法，返回结构化过滤器和剩余文本 */
function parseSearchSyntax(
  text: string,
): { filters: MemoFilter[]; remainingWords: string[] } {
  const tokens = text.split(/\s+/).filter((t) => t.length > 0);
  const filters: MemoFilter[] = [];
  const remainingWords: string[] = [];

  for (const token of tokens) {
    // tag:xxx
    const tagMatch = token.match(/^tag:(.+)$/i);
    if (tagMatch) {
      filters.push({ factor: "tagSearch", value: tagMatch[1] });
      continue;
    }

    // from:DATE
    const fromMatch = token.match(/^from:(.+)$/i);
    if (fromMatch) {
      const dateStr = parseFromDateExpr(fromMatch[1]);
      if (dateStr) {
        filters.push({ factor: "fromDate", value: dateStr });
      }
      continue;
    }

    // has:link|tasklist|code
    const hasMatch = token.match(/^has:(.+)$/i);
    if (hasMatch) {
      const prop = hasMatch[1].toLowerCase();
      if (prop === "link") {
        filters.push({ factor: "property.hasLink", value: "" });
      } else if (prop === "tasklist" || prop === "task") {
        filters.push({ factor: "property.hasTaskList", value: "" });
      } else if (prop === "code") {
        filters.push({ factor: "property.hasCode", value: "" });
      }
      continue;
    }

    // is:pinned
    const isMatch = token.match(/^is:(.+)$/i);
    if (isMatch) {
      const prop = isMatch[1].toLowerCase();
      if (prop === "pinned") {
        filters.push({ factor: "pinned", value: "" });
      }
      continue;
    }

    // 无前缀 → 关键词
    remainingWords.push(token);
  }

  return { filters, remainingWords };
}

const SearchBar = () => {
  const t = useTranslate();
  const { addFilter, removeFiltersByFactor } = useMemoFilterContext();
  const [queryText, setQueryText] = useState("");
  const [searchMode, setSearchMode] = useState<"keyword" | "semantic">("keyword");
  const inputRef = useRef<HTMLInputElement>(null);

  const onTextChange = (event: React.FormEvent<HTMLInputElement>) => {
    setQueryText(event.currentTarget.value);
  };

  const toggleMode = () => {
    setSearchMode((prev) => {
      const next = prev === "keyword" ? "semantic" : "keyword";
      // 切换时清除另一模式的 filter，避免混合
      removeFiltersByFactor(prev === "keyword" ? "contentSearch" : "semanticSearch");
      return next;
    });
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      const trimmedText = queryText.trim();
      if (trimmedText !== "") {
        // 解析搜索语法（tag:/from:/has:/is:）
        const { filters: parsedFilters, remainingWords } = parseSearchSyntax(trimmedText);

        // 添加解析出的过滤器
        parsedFilters.forEach((filter) => addFilter(filter));

        // 处理剩余文本
        if (remainingWords.length > 0) {
          if (searchMode === "keyword") {
            remainingWords.forEach((word) => {
              addFilter({ factor: "contentSearch", value: word });
            });
          } else {
            addFilter({ factor: "semanticSearch", value: remainingWords.join(" ") });
          }
        }
        setQueryText("");
      }
    }
  };

  return (
    <div className="relative w-full h-auto flex flex-row justify-start items-center">
      <SearchIcon className="absolute left-2 w-4 h-auto opacity-40 text-sidebar-foreground" />
      <input
        className="w-full text-sidebar-foreground leading-6 bg-sidebar border border-border text-sm rounded-lg p-1 pl-8 pr-8 outline-0"
        placeholder={searchMode === "keyword" ? t("memo.search-placeholder") : t("memo.search-placeholder-semantic")}
        value={queryText}
        onChange={onTextChange}
        onKeyDown={onKeyDown}
        ref={inputRef}
      />
      <button
        type="button"
        onClick={toggleMode}
        className="absolute right-8 top-1/2 -translate-y-1/2 text-sidebar-foreground opacity-60 hover:opacity-100"
        title={t("memo.search-mode-tooltip")}
      >
        {searchMode === "keyword" ? <TypeIcon className="w-4 h-4" /> : <SparklesIcon className="w-4 h-4" />}
      </button>
      <MemoDisplaySettingMenu className="absolute right-2 top-2 text-sidebar-foreground" />
    </div>
  );
};

export default SearchBar;
