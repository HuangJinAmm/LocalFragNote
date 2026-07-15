import { useEffect } from "react";
import { Outlet } from "react-router-dom";
import { SearchHighlightProvider } from "./components/MemoContent/SearchHighlightContext";
import { useInstance } from "./contexts/InstanceContext";
import { MemoFilterProvider } from "./contexts/MemoFilterContext";
import { useUserLocale } from "./hooks/useUserLocale";
import { useUserTheme } from "./hooks/useUserTheme";

const App = () => {
  const { generalSetting: instanceGeneralSetting } = useInstance();

  // 响应式应用用户偏好
  useUserLocale();
  useUserTheme();

  // 注入实例自定义样式（本地为空，不会执行）
  useEffect(() => {
    if (instanceGeneralSetting.additionalStyle) {
      const styleEl = document.createElement("style");
      styleEl.innerHTML = instanceGeneralSetting.additionalStyle;
      styleEl.setAttribute("type", "text/css");
      document.body.insertAdjacentElement("beforeend", styleEl);
    }
  }, [instanceGeneralSetting.additionalStyle]);

  // 注入实例自定义脚本（本地为空，不会执行）
  useEffect(() => {
    if (instanceGeneralSetting.additionalScript) {
      const scriptEl = document.createElement("script");
      scriptEl.innerHTML = instanceGeneralSetting.additionalScript;
      document.head.appendChild(scriptEl);
    }
  }, [instanceGeneralSetting.additionalScript]);

  // 动态更新元数据（本地为空，不会执行）
  useEffect(() => {
    if (!instanceGeneralSetting.customProfile) {
      return;
    }

    document.title = instanceGeneralSetting.customProfile.title;
    const link = document.querySelector("link[rel~='icon']") as HTMLLinkElement;
    link.href = instanceGeneralSetting.customProfile.logoUrl || "/logo.webp";
  }, [instanceGeneralSetting.customProfile]);

  return (
    <MemoFilterProvider>
      <SearchHighlightProvider>
        <Outlet />
      </SearchHighlightProvider>
    </MemoFilterProvider>
  );
};

export default App;
