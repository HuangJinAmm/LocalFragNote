import { useCallback, useMemo } from "react";
import type { MemoFilter } from "@/contexts/MemoFilterContext";
import { useLocalStorage } from "./useLocalStorage";

/// 过滤器预设:任意过滤器组合的命名快照(不仅限于标签)
export interface FilterPreset {
  id: string;
  name: string;
  /// 完整过滤器快照,应用时直接替换当前过滤器
  filters: MemoFilter[];
}

const STORAGE_KEY = "filter-presets";

/// 生成唯一 id
const genId = (): string => {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
};

/// 过滤器预设 hook:基于 localStorage 的增删查
export const useFilterPresets = () => {
  const [presets, setPresets] = useLocalStorage<FilterPreset[]>(STORAGE_KEY, []);

  /// 新增预设
  const addPreset = useCallback(
    (name: string, filters: MemoFilter[]) => {
      if (!name.trim()) return;
      setPresets((prev) => [...prev, { id: genId(), name: name.trim(), filters: [...filters] }]);
    },
    [setPresets],
  );

  /// 更新预设
  const updatePreset = useCallback(
    (id: string, updates: Partial<Pick<FilterPreset, "name" | "filters">>) => {
      setPresets((prev) => prev.map((p) => (p.id === id ? { ...p, ...updates } : p)));
    },
    [setPresets],
  );

  /// 删除预设
  const removePreset = useCallback(
    (id: string) => {
      setPresets((prev) => prev.filter((p) => p.id !== id));
    },
    [setPresets],
  );

  return useMemo(
    () => ({ presets, addPreset, updatePreset, removePreset }),
    [presets, addPreset, updatePreset, removePreset],
  );
};
