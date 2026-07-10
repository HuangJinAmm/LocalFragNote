import { useInstance } from "@/contexts/InstanceContext";
import { cn } from "@/lib/utils";

interface Props {
  className?: string;
  collapsed?: boolean;
}

function MemosLogo(props: Props) {
  const { collapsed } = props;
  const { generalSetting: instanceGeneralSetting } = useInstance();
  const title = instanceGeneralSetting.customProfile?.title || "Memos";

  // 本地应用：不显示 app logo icon，仅保留标题文字
  return (
    <div className={cn("relative w-full h-auto shrink-0", props.className)}>
      <div className={cn("w-auto flex flex-row justify-start items-center text-foreground", collapsed ? "px-1" : "px-3")}>
        {!collapsed && <span className="text-lg font-medium text-foreground shrink truncate">{title}</span>}
      </div>
    </div>
  );
}

export default MemosLogo;
