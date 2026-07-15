import { BookOpenIcon, CompassIcon, LibraryIcon, PaperclipIcon } from "lucide-react";
import { useEffect, useRef } from "react";
import { NavLink } from "react-router-dom";
import { Badge } from "@/components/ui/badge";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { Routes } from "@/router";
import { useTranslate } from "@/utils/i18n";
import { useTotalDueCount } from "@/components/Review/hooks";
import MemosLogo from "./MemosLogo";
import UserMenu from "./UserMenu";

interface NavLinkItem {
  id: string;
  path: string;
  title: string;
  icon: React.ReactNode;
  badge?: number;
}

interface Props {
  collapsed?: boolean;
  className?: string;
}

const Navigation = (props: Props) => {
  const { collapsed, className } = props;
  const t = useTranslate();
  const { dueCount } = useTotalDueCount();
  const notifiedRef = useRef(false);

  // 应用启动后若有到期卡片，发送一次系统通知
  useEffect(() => {
    if (notifiedRef.current || dueCount === 0) return;
    notifiedRef.current = true;

    if ("Notification" in window && Notification.permission === "granted") {
      new Notification(t("review.due-reminder-title"), {
        body: t("review.due-reminder-body", { count: dueCount }),
      });
    } else if ("Notification" in window && Notification.permission === "default") {
      Notification.requestPermission().then((perm) => {
        if (perm === "granted") {
          new Notification(t("review.due-reminder-title"), {
            body: t("review.due-reminder-body", { count: dueCount }),
          });
        }
      });
    }
  }, [dueCount, t]);

  const homeNavLink: NavLinkItem = {
    id: "header-memos",
    path: Routes.HOME,
    title: t("common.memos"),
    icon: <LibraryIcon className="w-6 h-auto shrink-0" />,
  };
  const attachmentsNavLink: NavLinkItem = {
    id: "header-attachments",
    path: Routes.ATTACHMENTS,
    title: t("common.attachments"),
    icon: <PaperclipIcon className="w-6 h-auto shrink-0" />,
  };
  const discoverNavLink: NavLinkItem = {
    id: "header-discover",
    path: Routes.DISCOVER,
    title: t("lan.discover.button"),
    icon: <CompassIcon className="w-6 h-auto shrink-0" />,
  };

  const reviewNavLink: NavLinkItem = {
    id: "header-review",
    path: Routes.REVIEW,
    title: t("review.nav-title"),
    icon: <BookOpenIcon className="w-6 h-auto shrink-0" />,
    badge: dueCount > 0 ? dueCount : undefined,
  };

  // 本地单用户应用：主导航包含 home、attachments、discover 和 review
  const primaryNavLinks: NavLinkItem[] = [homeNavLink, attachmentsNavLink, discoverNavLink, reviewNavLink];

  return (
    <header className={cn("w-full h-full overflow-auto flex flex-col justify-between items-start gap-4", className)}>
      <div className="w-full px-1 py-1 flex flex-col justify-start items-start space-y-2 overflow-auto overflow-x-hidden shrink">
        <NavLink className="mb-3 cursor-default" to={Routes.HOME}>
          <MemosLogo collapsed={collapsed} />
        </NavLink>
        <TooltipProvider>
          {primaryNavLinks.map((navLink) => (
            <NavLink
              className={({ isActive }) =>
                cn(
                  "px-2 py-2 rounded-2xl border flex flex-row items-center text-lg text-sidebar-foreground transition-colors",
                  collapsed ? "" : "w-full px-4",
                  isActive
                    ? "bg-sidebar-accent text-sidebar-accent-foreground border-sidebar-accent-border drop-shadow"
                    : "border-transparent hover:bg-sidebar-accent hover:text-sidebar-accent-foreground hover:border-sidebar-accent-border opacity-80",
                )
              }
              key={navLink.id}
              to={navLink.path}
              end={navLink.path === Routes.HOME}
              id={navLink.id}
              viewTransition
            >
              {props.collapsed ? (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <div className="relative">
                      {navLink.icon}
                      {navLink.badge !== undefined && (
                        <Badge
                          variant="warning"
                          shape="pill"
                          className="absolute -top-2 -right-2 min-w-4 h-4 px-1 text-[10px] justify-center"
                        >
                          {navLink.badge > 99 ? "99+" : navLink.badge}
                        </Badge>
                      )}
                    </div>
                  </TooltipTrigger>
                  <TooltipContent side="right">
                    <p>{navLink.title}</p>
                  </TooltipContent>
                </Tooltip>
              ) : (
                <div className="relative">
                  {navLink.icon}
                  {navLink.badge !== undefined && (
                    <Badge
                      variant="warning"
                      shape="pill"
                      className="absolute -top-2 -right-2 min-w-4 h-4 px-1 text-[10px] justify-center"
                    >
                      {navLink.badge > 99 ? "99+" : navLink.badge}
                    </Badge>
                  )}
                </div>
              )}
              {!props.collapsed && <span className="ml-3 truncate">{navLink.title}</span>}
            </NavLink>
          ))}
        </TooltipProvider>
      </div>
      <div className={cn("w-full flex flex-col justify-end", props.collapsed ? "items-center" : "items-start pl-3")}>
        <UserMenu collapsed={collapsed} />
      </div>
    </header>
  );
};

export default Navigation;
