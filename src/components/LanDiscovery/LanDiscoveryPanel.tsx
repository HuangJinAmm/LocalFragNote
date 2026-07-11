import type { FC } from "react";
import { useState } from "react";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import PeerList from "./PeerList";
import RemoteMemoList from "./RemoteMemoList";
import RemoteMemoPreview from "./RemoteMemoPreview";
import { useLanDiscovery } from "./hooks";
import type { PeerInfo } from "./types";
import { useTranslate } from "@/utils/i18n";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const LanDiscoveryPanel: FC<Props> = ({ open, onOpenChange }) => {
  const t = useTranslate();
  const { peers, loading } = useLanDiscovery();
  const [selectedPeer, setSelectedPeer] = useState<PeerInfo | null>(null);
  const [selectedMemoUid, setSelectedMemoUid] = useState<string | null>(null);

  const handleSelectPeer = (peer: PeerInfo) => {
    setSelectedPeer(peer);
    setSelectedMemoUid(null);
  };

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="w-[700px] max-w-full p-0 flex flex-col">
        <SheetHeader className="px-4 py-3 border-b">
          <SheetTitle>{t("lan.discovery.title")}</SheetTitle>
        </SheetHeader>
        <div className="flex-1 flex overflow-hidden">
          {/* 左栏：peer 列表 */}
          <div className="w-56 border-r overflow-auto">
            <PeerList
              peers={peers}
              loading={loading}
              selectedPeerId={selectedPeer?.peer_id ?? null}
              onSelect={handleSelectPeer}
            />
          </div>
          {/* 右栏：笔记列表 / 预览 */}
          <div className="flex-1 overflow-hidden">
            {!selectedPeer ? (
              <div className="flex items-center justify-center h-full text-sm text-muted-foreground">
                {t("lan.discovery.empty")}
              </div>
            ) : selectedMemoUid ? (
              <RemoteMemoPreview
                peerId={selectedPeer.peer_id}
                uid={selectedMemoUid}
                onBack={() => setSelectedMemoUid(null)}
              />
            ) : (
              <RemoteMemoList
                peer={selectedPeer}
                selectedMemoUid={null}
                onSelectMemo={setSelectedMemoUid}
              />
            )}
          </div>
        </div>
      </SheetContent>
    </Sheet>
  );
};

export default LanDiscoveryPanel;
