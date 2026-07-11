import { CompassIcon } from "lucide-react";
import { useState } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import LanDiscoveryPanel from "@/components/LanDiscovery";
import { cn } from "@/lib/utils";
import { useTranslate } from "@/utils/i18n";

interface Props {
  /** Collapsed mode: icon-only with tooltip (used in narrow sidebar) */
  collapsed?: boolean;
}

/**
 * Discover button — opens the LAN discovery panel.
 *
 * In collapsed mode, renders as a compact icon button with tooltip
 * (matching Navigation's NavLink style). In expanded mode, renders
 * icon + label.
 */
const DiscoverButton = ({ collapsed = false }: Props) => {
  const t = useTranslate();
  const [open, setOpen] = useState(false);

  const button = (
    <button
      type="button"
      onClick={() => setOpen(true)}
      className={cn(
        "px-2 py-2 rounded-2xl border flex flex-row items-center text-lg text-sidebar-foreground transition-colors cursor-pointer",
        collapsed ? "" : "w-full px-4",
        "border-transparent hover:bg-sidebar-accent hover:text-sidebar-accent-foreground hover:border-sidebar-accent-border opacity-80",
      )}
    >
      <CompassIcon className="w-6 h-auto shrink-0" />
      {!collapsed && <span className="ml-3 truncate">{t("lan.discover.button")}</span>}
    </button>
  );

  return (
    <>
      {collapsed ? (
        <Tooltip>
          <TooltipTrigger asChild>{button}</TooltipTrigger>
          <TooltipContent side="right">
            <p>{t("lan.discover.tooltip")}</p>
          </TooltipContent>
        </Tooltip>
      ) : (
        button
      )}
      <LanDiscoveryPanel open={open} onOpenChange={setOpen} />
    </>
  );
};

export default DiscoverButton;
