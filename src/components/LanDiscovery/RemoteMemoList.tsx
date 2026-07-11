import type { FC } from "react";
import { PinIcon, PaperclipIcon } from "lucide-react";
import dayjs from "dayjs";
import { Button } from "@/components/ui/button";
import type { PeerInfo, RemoteMemoSummary } from "./types";
import { useRemoteProfile, useRemoteMemos } from "./hooks";
import { useTranslate } from "@/utils/i18n";

interface Props {
  peer: PeerInfo;
  selectedMemoUid: string | null;
  onSelectMemo: (uid: string) => void;
}

const RemoteMemoList: FC<Props> = ({ peer, selectedMemoUid, onSelectMemo }) => {
  const t = useTranslate();
  const { profile, loading: profileLoading } = useRemoteProfile(peer.peer_id);
  const {
    memos,
    total,
    loading,
    error,
    hasMore,
    loadMore,
    tagFilter,
    setTagFilter,
    retry,
  } = useRemoteMemos(peer.peer_id);

  return (
    <div className="flex flex-col h-full">
      {/* Header: peer 信息 */}
      <div className="p-4 border-b">
        <div className="font-medium text-base">{peer.display_name}</div>
        {profileLoading ? (
          <div className="text-xs text-muted-foreground">…</div>
        ) : profile ? (
          <div className="text-xs text-muted-foreground mt-1">
            {t("lan.peer.publicMemos")}: {profile.public_memo_count} · {profile.tags.join(", ")}
          </div>
        ) : null}
      </div>

      {/* Tag 筛选 */}
      {profile && profile.tags.length > 0 && (
        <div className="px-4 py-2 border-b flex flex-wrap gap-1">
          <button
            onClick={() => setTagFilter(null)}
            className={`px-2 py-0.5 text-xs rounded-full border ${
              tagFilter === null ? "bg-primary text-primary-foreground" : "bg-background"
            }`}
          >
            All
          </button>
          {profile.tags.map((tag) => (
            <button
              key={tag}
              onClick={() => setTagFilter([tag])}
              className={`px-2 py-0.5 text-xs rounded-full border ${
                tagFilter?.includes(tag) ? "bg-primary text-primary-foreground" : "bg-background"
              }`}
            >
              #{tag}
            </button>
          ))}
        </div>
      )}

      {/* 笔记列表 */}
      <div className="flex-1 overflow-auto">
        {error ? (
          <div className="p-4 text-sm text-destructive">
            {t("lan.memo.loadFailed")}: {error}
            <Button variant="ghost" size="sm" onClick={retry} className="ml-2">
              Retry
            </Button>
          </div>
        ) : loading && memos.length === 0 ? (
          <div className="p-4 space-y-2">
            <div className="bg-muted/70 rounded animate-pulse h-16 w-full" />
            <div className="bg-muted/70 rounded animate-pulse h-16 w-full" />
          </div>
        ) : memos.length === 0 ? (
          <div className="p-4 text-sm text-muted-foreground">{t("lan.discovery.empty")}</div>
        ) : (
          <div className="divide-y">
            {memos.map((memo) => (
              <MemoCard
                key={memo.uid}
                memo={memo}
                isSelected={memo.uid === selectedMemoUid}
                onClick={() => onSelectMemo(memo.uid)}
              />
            ))}
            {hasMore && (
              <div className="p-2">
                <Button variant="ghost" size="sm" onClick={loadMore} disabled={loading} className="w-full">
                  {loading ? "…" : "Load more"}
                </Button>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Footer 统计 */}
      <div className="p-2 border-t text-xs text-muted-foreground text-center">
        {memos.length} / {total}
      </div>
    </div>
  );
};

const MemoCard: FC<{
  memo: RemoteMemoSummary;
  isSelected: boolean;
  onClick: () => void;
}> = ({ memo, isSelected, onClick }) => (
  <button
    onClick={onClick}
    className={`w-full text-left px-4 py-3 hover:bg-accent transition-colors ${
      isSelected ? "bg-accent" : ""
    }`}
  >
    <div className="flex items-start gap-2">
      {memo.pinned && <PinIcon className="size-4 text-primary shrink-0 mt-0.5" />}
      <div className="flex-1 min-w-0">
        <div className="text-sm line-clamp-2">{memo.snippet}</div>
        <div className="flex items-center gap-2 mt-1 text-xs text-muted-foreground">
          <span>{dayjs.unix(memo.created_ts).format("YYYY-MM-DD")}</span>
          {memo.tags.length > 0 && <span>· {memo.tags.map((t) => `#${t}`).join(" ")}</span>}
          {memo.has_attachments && <PaperclipIcon className="size-3" />}
        </div>
      </div>
    </div>
  </button>
);

export default RemoteMemoList;
