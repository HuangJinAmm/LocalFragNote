import { SearchIcon, SparklesIcon, TypeIcon } from "lucide-react";
import { useRef, useState } from "react";
import { useMemoFilterContext } from "@/contexts/MemoFilterContext";
import { useTranslate } from "@/utils/i18n";
import MemoDisplaySettingMenu from "./MemoDisplaySettingMenu";

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
        if (searchMode === "keyword") {
          // 关键词模式：拆词为多个 contentSearch filter
          const words = trimmedText.split(/\s+/);
          words.forEach((word) => {
            addFilter({
              factor: "contentSearch",
              value: word,
            });
          });
        } else {
          // 语义模式：整句作为单个 semanticSearch filter
          addFilter({
            factor: "semanticSearch",
            value: trimmedText,
          });
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
