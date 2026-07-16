import { useEffect, useState } from "react";
import { useLocation } from "react-router-dom";
import { Button } from "@/components/ui/button";
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetTrigger } from "@/components/ui/sheet";
import { useInstance } from "@/contexts/InstanceContext";
import Navigation from "./Navigation";

const NavigationDrawer = () => {
  const location = useLocation();
  const [open, setOpen] = useState(false);
  const { generalSetting } = useInstance();
  const title = generalSetting.customProfile?.title || "破碎星球";

  useEffect(() => {
    setOpen(false);
  }, [location.key]);

  return (
    <Sheet open={open} onOpenChange={setOpen}>
      <SheetTrigger asChild>
        <Button variant="ghost" className="px-2">
          {/* 本地应用：不显示 app logo icon，仅保留标题文字 */}
          <span className="font-bold text-lg leading-10 text-ellipsis overflow-hidden text-foreground">{title}</span>
        </Button>
      </SheetTrigger>
      <SheetContent side="left" className="w-80 max-w-full overflow-auto px-2 bg-background">
        <SheetHeader>
          <SheetTitle />
        </SheetHeader>
        <Navigation className="pb-4" />
      </SheetContent>
    </Sheet>
  );
};

export default NavigationDrawer;
