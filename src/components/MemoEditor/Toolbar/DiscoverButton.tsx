import { CompassIcon } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import LanDiscoveryPanel from "@/components/LanDiscovery";
import { useTranslate } from "@/utils/i18n";

export const DiscoverButton = () => {
  const t = useTranslate();
  const [open, setOpen] = useState(false);

  return (
    <>
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="icon" onClick={() => setOpen(true)}>
              <CompassIcon className="size-5 text-foreground" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>{t("lan.discover.tooltip")}</TooltipContent>
        </Tooltip>
      </TooltipProvider>
      <LanDiscoveryPanel open={open} onOpenChange={setOpen} />
    </>
  );
};

export default DiscoverButton;
