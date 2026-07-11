import type { FC } from "react";
import { useState } from "react";
import PeerList from "./PeerList";
import RemoteMemoList from "./RemoteMemoList";
import RemoteMemoPreview from "./RemoteMemoPreview";
import { useLanDiscovery } from "./hooks";
import type { PeerInfo } from "./types";
import { useTranslate } from "@/utils/i18n";

/**
 * LAN discovery panel content — renders peer list + remote memo list/preview.
 *
 * This is a pure content component (no Sheet/Drawer wrapper).
 * Used by the Discover page which provides the page chrome.
 */
const LanDiscoveryPanel: FC = () => {
  const t = useTranslate();
  const { peers, loading } = useLanDiscovery();
  const [selectedPeer, setSelectedPeer] = useState<PeerInfo | null>(null);
  const [selectedMemoUid, setSelectedMemoUid] = useState<string | null>(null);

  const handleSelectPeer = (peer: PeerInfo) => {
    setSelectedPeer(peer);
    setSelectedMemoUid(null);
  };

  return (
    <div className="flex flex-col rounded-lg border border-border overflow-hidden" style={{ height: "calc(100vh - 180px)", minHeight: "400px" }}>
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
    </div>
  );
};

export default LanDiscoveryPanel;
