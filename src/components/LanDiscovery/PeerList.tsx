import type { FC } from "react";
import type { PeerInfo } from "./types";
import { useTranslate } from "@/utils/i18n";

interface Props {
  peers: PeerInfo[];
  loading: boolean;
  selectedPeerId: string | null;
  onSelect: (peer: PeerInfo) => void;
}

const PeerList: FC<Props> = ({ peers, loading, selectedPeerId, onSelect }) => {
  const t = useTranslate();

  if (loading) {
    return (
      <div className="p-4 space-y-2">
        <div className="bg-muted/70 rounded animate-pulse h-12 w-full" />
        <div className="bg-muted/70 rounded animate-pulse h-12 w-full" />
      </div>
    );
  }

  if (peers.length === 0) {
    return (
      <div className="p-4 text-sm text-muted-foreground">{t("lan.discovery.empty")}</div>
    );
  }

  return (
    <div className="space-y-1">
      {peers.map((peer) => {
        const isSelected = peer.peer_id === selectedPeerId;
        const shortId = peer.peer_id.slice(0, 8);
        return (
          <button
            key={peer.peer_id}
            onClick={() => onSelect(peer)}
            className={`w-full text-left px-3 py-2 rounded-md hover:bg-accent transition-colors ${
              isSelected ? "bg-accent" : ""
            }`}
          >
            <div className="flex items-center gap-2">
              <span className="size-2 rounded-full bg-green-500" />
              <span className="font-medium truncate">{peer.display_name}</span>
            </div>
            <div className="text-xs text-muted-foreground mt-0.5">{shortId}…</div>
          </button>
        );
      })}
    </div>
  );
};

export default PeerList;
