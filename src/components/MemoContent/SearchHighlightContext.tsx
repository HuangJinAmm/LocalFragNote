import { createContext, type ReactNode, useContext, useMemo } from "react";
import { useMemoFilterContext } from "@/contexts/MemoFilterContext";

/**
 * 提供当前需要高亮的搜索关键词列表。
 * 从 MemoFilterContext 读取 contentSearch 过滤器值并去重。
 * 在没有 Provider 的地方（如分享图预览）默认空数组，不高亮。
 */
const SearchHighlightContext = createContext<string[]>([]);

export const SearchHighlightProvider = ({ children }: { children: ReactNode }) => {
  const { filters } = useMemoFilterContext();

  const terms = useMemo(() => {
    const seen = new Set<string>();
    const list: string[] = [];
    for (const f of filters) {
      if (f.factor === "contentSearch" && f.value && !seen.has(f.value)) {
        seen.add(f.value);
        list.push(f.value);
      }
    }
    return list;
  }, [filters]);

  return <SearchHighlightContext.Provider value={terms}>{children}</SearchHighlightContext.Provider>;
};

export const useSearchHighlightTerms = () => useContext(SearchHighlightContext);
