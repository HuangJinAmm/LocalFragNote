import { useMemo } from "react";
import MemoEditor from "@/components/MemoEditor";
import { deriveDefaultCreateTimeFromFilters } from "@/components/MemoEditor/utils/deriveDefaultCreateTime";
import MemoView from "@/components/MemoView";
import PagedMemoList, { getMemoKey } from "@/components/PagedMemoList";
import { useInstance } from "@/contexts/InstanceContext";
import { type MemoFilter, useMemoFilterContext } from "@/contexts/MemoFilterContext";
import { NewMemoProvider } from "@/contexts/NewMemoContext";
import { useMemoFilters, useMemoSorting } from "@/hooks";
import useCurrentUser from "@/hooks/useCurrentUser";
import { cn } from "@/lib/utils";
import { State } from "@/types/proto/api/v1/common_pb";
import { Memo } from "@/types/proto/api/v1/memo_service_pb";
import { useTranslate } from "@/utils/i18n";

const Home = () => {
  const t = useTranslate();
  const user = useCurrentUser();
  const { isInitialized } = useInstance();
  const { filters } = useMemoFilterContext();

  const memoFilter = useMemoFilters({
    creatorName: user?.name,
    includeShortcuts: true,
    includePinned: true,
  });

  const { listSort, orderBy } = useMemoSorting({
    pinnedFirst: true,
    state: State.NORMAL,
  });

  // 编辑器移至页面底部 sticky 容器,这里需自行根据 filters 推导默认创建时间
  // (原逻辑在 PagedMemoList 内部,提取后由 Home 负责)。
  const defaultCreateTime = useMemo(
    () => deriveDefaultCreateTimeFromFilters(filters as MemoFilter[]),
    [filters],
  );

  return (
    <NewMemoProvider>
      {/*
        根容器使用视窗高度单位,不依赖父级 min-h-full 链(该链在 MainLayout
        内层 padding 包裹下无法解析为视窗高度,会导致空列表时 flex-1 无空间
        可撑开、编辑器停在中部)。
        扣除 MainLayout 的顶部内边距(pt-2 md:pt-6),使容器底部对齐视窗底部,
        sticky bottom-0 才能正确吸附到视窗底。MainLayout 底部 pb-8 仍保留为
        可滚动余量,sticky 会在滚动时把编辑器钉在视窗底。
      */}
      <div className="w-full min-h-[calc(100svh-0.5rem)] md:min-h-[calc(100svh-1.5rem)] bg-background text-foreground flex flex-col">
        <div className="flex-1 min-h-0">
          <PagedMemoList
            renderer={(memo: Memo, { compact }) => (
              <MemoView key={getMemoKey(memo)} memo={memo} showVisibility showPinned compact={compact} />
            )}
            listSort={listSort}
            orderBy={orderBy}
            filter={memoFilter}
            enabled={isInitialized}
          />
        </div>
        {/*
          底部笔记输入框:sticky bottom 让它在滚动浏览时始终触手可及。
          - bg-background/95 + backdrop-blur:让下方列表内容滚过时半透明可感知
          - border-t:与列表区视觉分隔
          - px-0:外层已在 MainLayout 的 px-4 sm:px-6 内,这里不再叠加横向内边距,
            保持与列表(max-w-2xl mx-auto)水平对齐
          - MemoEditor 内部 normal mode 已有 max-h-[50vh] 约束,内容过多时自身滚动
        */}
        <div
          className={cn(
            "sticky bottom-0 z-10 bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/80",
            "px-0 pt-2 pb-4 border-t border-border/40",
          )}
        >
          <div className="w-full mx-auto max-w-2xl">
            <MemoEditor
              cacheKey="home-memo-editor"
              placeholder={t("editor.any-thoughts")}
              defaultCreateTime={defaultCreateTime}
            />
          </div>
        </div>
      </div>
    </NewMemoProvider>
  );
};

export default Home;
