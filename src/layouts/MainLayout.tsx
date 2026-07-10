import { useMemo } from "react";
import { Outlet, useLocation } from "react-router-dom";
import type { MemoExplorerContext } from "@/components/MemoExplorer";
import { MemoExplorer, MemoExplorerDrawer } from "@/components/MemoExplorer";
import MobileHeader from "@/components/MobileHeader";
import useCurrentUser from "@/hooks/useCurrentUser";
import { useFilteredMemoStats } from "@/hooks/useFilteredMemoStats";
import useMediaQuery from "@/hooks/useMediaQuery";
import { cn } from "@/lib/utils";
import { Routes } from "@/router";

const DESKTOP_EXPLORER_WIDTH_CLASS = "w-64";
const DESKTOP_EXPLORER_CLASS_NAME = cn("sticky top-0 h-svh shrink-0 border-r border-border transition-all", DESKTOP_EXPLORER_WIDTH_CLASS);
const MAIN_CONTENT_CLASS_NAME = "w-full min-h-full min-w-0 flex-1";

const MainLayout = () => {
  const md = useMediaQuery("md");
  const location = useLocation();
  const currentUser = useCurrentUser();
  const showMemoExplorer = location.pathname !== Routes.ABOUT;

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
        <div className={DESKTOP_EXPLORER_CLASS_NAME}>
          <MemoExplorer className="px-3 py-6" {...memoExplorerProps} />
        </div>
      )}
      <div className={MAIN_CONTENT_CLASS_NAME}>
        <div className={cn("w-full mx-auto px-4 sm:px-6 pt-2 md:pt-6 pb-8")}>
          <Outlet />
        </div>
      </div>
    </section>
  );
};

export default MainLayout;
