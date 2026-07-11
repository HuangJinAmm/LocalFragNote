import { CompassIcon } from "lucide-react";
import MobileHeader from "@/components/MobileHeader";
import LanDiscoveryPanel from "@/components/LanDiscovery";
import { useTranslate } from "@/utils/i18n";

const Discover = () => {
  const t = useTranslate();

  return (
    <section className="@container w-full min-h-full pb-10 sm:pt-3 md:pt-6">
      <MobileHeader />
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 px-4 sm:px-6">
        <div className="flex items-center gap-2 pt-2">
          <CompassIcon className="size-5 text-foreground" />
          <h1 className="text-lg font-semibold text-foreground">{t("lan.discovery.title")}</h1>
        </div>
        <LanDiscoveryPanel />
      </div>
    </section>
  );
};

export default Discover;
