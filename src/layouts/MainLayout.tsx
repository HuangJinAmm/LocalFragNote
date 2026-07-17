import { useMemo } from "react";
import { Outlet, useLocation } from "react-router-dom";
import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react";
import type { MemoExplorerContext } from "@/components/MemoExplorer";
import { MemoExplorer, MemoExplorerDrawer } from "@/components/MemoExplorer";
import MobileHeader from "@/components/MobileHeader";
import useCurrentUser from "@/hooks/useCurrentUser";
import { useFilteredMemoStats } from "@/hooks/useFilteredMemoStats";
import { useLocalStorage } from "@/hooks";
import useMediaQuery from "@/hooks/useMediaQuery";
import { cn } from "@/lib/utils";
import { Routes } from "@/router";
import { AiChatPanel } from "@/components/AiChat";
import { useTranslate } from "@/utils/i18n";

const DESKTOP_EXPLORER_EXPANDED_WIDTH_CLASS = "w-64";
const DESKTOP_EXPLORER_COLLAPSED_WIDTH_CLASS = "w-10";
const MAIN_CONTENT_CLASS_NAME = "w-full min-h-full min-w-0 flex-1";

const MainLayout = () => {
  const t = useTranslate();
  const md = useMediaQuery("md");
  const location = useLocation();
  const currentUser = useCurrentUser();
  const showMemoExplorer = location.pathname !== Routes.ABOUT;
  const [explorerCollapsed, setExplorerCollapsed] = useLocalStorage<boolean>("memo-explorer-collapsed", false);

  // 本地单用户应用：仅 home 和 archived 两种上下文
  const context: MemoExplorerContext = useMemo(() => {
    if (location.pathname === Routes.HOME) return "home";
    if (location.pathname === Routes.ARCHIVED) return "archived";
    return "home";
  }, [location.pathname]);

  // 本地单用户：stats 直接使用当前用户名
  const statsUserName = useMemo(() => {
    if (context === "home") return currentUser?.name;
    return undefined;
  }, [context, currentUser]);

  const { statistics, tags } = useFilteredMemoStats({ userName: statsUserName, context });
  const memoExplorerProps = { context, statisticsData: statistics, tagCount: tags };

  return (
    <section className="@container w-full min-h-full flex flex-col justify-start items-center md:flex-row md:items-start">
      {!md && <MobileHeader>{showMemoExplorer && <MemoExplorerDrawer {...memoExplorerProps} />}</MobileHeader>}
      {md && showMemoExplorer && (
        <div
          className={cn(
            "sticky top-0 h-svh shrink-0 border-r border-border transition-[width] duration-200 overflow-hidden",
            explorerCollapsed ? DESKTOP_EXPLORER_COLLAPSED_WIDTH_CLASS : DESKTOP_EXPLORER_EXPANDED_WIDTH_CLASS,
          )}
        >
          {explorerCollapsed ? (
            <button
              type="button"
              onClick={() => setExplorerCollapsed(false)}
              aria-label={t("common.expand")}
              title={t("common.expand")}
              className="w-full h-full flex flex-col items-center justify-start pt-3 hover:bg-accent transition-colors"
            >
              <ChevronRightIcon className="w-4 h-4 text-muted-foreground" />
            </button>
          ) : (
            <div className="relative w-full h-full">
              <button
                type="button"
                onClick={() => setExplorerCollapsed(true)}
                aria-label={t("common.collapse")}
                title={t("common.collapse")}
                className="absolute top-3 right-1 z-10 w-6 h-6 flex items-center justify-center rounded text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
              >
                <ChevronLeftIcon className="w-4 h-4" />
              </button>
              <MemoExplorer className="px-3 py-6 pr-7" {...memoExplorerProps} />
            </div>
          )}
        </div>
      )}
      <div className={MAIN_CONTENT_CLASS_NAME}>
        <div className={cn("w-full mx-auto px-4 sm:px-6 pt-2 md:pt-6 pb-8")}>
          <Outlet />
        </div>
      </div>
      <AiChatPanel />
    </section>
  );
};

export default MainLayout;
