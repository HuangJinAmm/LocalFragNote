// 本地应用路由：无守卫，无认证路由
import { lazy } from "react";
import { createHashRouter, Navigate, type RouteObject } from "react-router-dom";
import App from "@/App";
import { ChunkLoadErrorFallback } from "@/components/ErrorBoundary";
import MainLayout from "@/layouts/MainLayout";
import RootLayout from "@/layouts/RootLayout";
import { ROUTES } from "./routes";

function lazyWithReload<T extends React.ComponentType>(factory: () => Promise<{ default: T }>) {
  return lazy(() =>
    factory().catch((error) => {
      const isChunkError =
        error?.message?.includes("Failed to fetch dynamically imported module") ||
        error?.message?.includes("Importing a module script failed");
      const reloadKey = "chunk-reload";
      if (isChunkError && !sessionStorage.getItem(reloadKey)) {
        sessionStorage.setItem(reloadKey, "1");
        window.location.reload();
      }
      throw error;
    }),
  );
}

const About = lazyWithReload(() => import("@/pages/About"));
const Archived = lazyWithReload(() => import("@/pages/Archived"));
const Home = lazyWithReload(() => import("@/pages/Home"));
const MemoDetail = lazyWithReload(() => import("@/pages/MemoDetail"));
const NotFound = lazyWithReload(() => import("@/pages/NotFound"));
const Attachments = lazyWithReload(() => import("@/pages/Attachments"));
const Setting = lazyWithReload(() => import("@/pages/Setting"));

export const Routes = ROUTES;
export { ROUTES };

export const routeConfig: RouteObject[] = [
  {
    path: "/",
    element: <App />,
    errorElement: <ChunkLoadErrorFallback />,
    children: [
      { path: "home", element: <Navigate to={Routes.HOME} replace /> },
      {
        element: <RootLayout />,
        children: [
          {
            element: <MainLayout />,
            children: [
              { index: true, element: <Home /> },
              { path: Routes.ABOUT, element: <About /> },
              { path: Routes.ARCHIVED, element: <Archived /> },
            ],
          },
          { path: "memos/:uid", element: <MemoDetail /> },
          { path: Routes.ATTACHMENTS, element: <Attachments /> },
          { path: Routes.SETTING, element: <Setting /> },
          { path: "404", element: <NotFound /> },
          { path: "*", element: <NotFound /> },
        ],
      },
    ],
  },
];

// Tauri 推荐 hash 路由
const router = createHashRouter(routeConfig);

export default router;
