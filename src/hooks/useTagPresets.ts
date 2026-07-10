import { useCallback, useMemo } from "react";
import { useLocalStorage } from "./useLocalStorage";

/// 标签预设：一组标签的命名组合
export interface TagPreset {
  id: string;
  name: string;
  tags: string[];
}

const STORAGE_KEY = "tag-presets";

/// 生成唯一 id
const genId = (): string => {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
};

/// 标签预设 hook：基于 localStorage 的增删查
export const useTagPresets = () => {
  const [presets, setPresets] = useLocalStorage<TagPreset[]>(STORAGE_KEY, []);

  /// 新增预设
  const addPreset = useCallback(
    (name: string, tags: string[]) => {
      if (!name.trim() || tags.length === 0) return;
      setPresets((prev) => [...prev, { id: genId(), name: name.trim(), tags: [...tags] }]);
    },
    [setPresets],
  );

  /// 更新预设
  const updatePreset = useCallback(
    (id: string, updates: Partial<Pick<TagPreset, "name" | "tags">>) => {
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
