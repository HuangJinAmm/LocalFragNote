import type { FC } from "react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { MemoMarkdownRenderer } from "@/components/MemoContent/MemoMarkdownRenderer";
import { useRemoteMemoPreview } from "./hooks";
import { useTranslate } from "@/utils/i18n";
import toast from "react-hot-toast";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  peerId: string;
  uid: string;
  onBack: () => void;
}

const RemoteMemoPreview: FC<Props> = ({ peerId, uid, onBack }) => {
  const t = useTranslate();
  const { memo, loading, error, fetchAttachment } = useRemoteMemoPreview(peerId, uid);
  const [copying, setCopying] = useState(false);

  const handleCopy = async () => {
    if (!confirm(t("lan.memo.copyConfirm"))) return;
    setCopying(true);
    try {
      const res = await invoke<{ new_memo_uid: string }>("lan_copy_memo_to_local", {
        req: { peer_id: peerId, uid },
      });
      toast.success(t("lan.memo.copySuccess"));
      onBack();
      void res;
    } catch (e) {
      toast.error(`${t("lan.memo.copyFailed")}: ${e}`);
    } finally {
      setCopying(false);
    }
  };

  if (loading) {
    return (
      <div className="p-4 space-y-2">
        <div className="bg-muted/70 rounded animate-pulse h-6 w-1/3" />
        <div className="bg-muted/70 rounded animate-pulse h-32 w-full" />
      </div>
    );
  }

  if (error || !memo) {
    return (
      <div className="p-4">
        <p className="text-sm text-destructive">{error || t("lan.memo.notFound")}</p>
        <Button variant="ghost" size="sm" onClick={onBack} className="mt-2">
          Back
        </Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 overflow-auto p-4">
        <MemoMarkdownRenderer content={memo.content} resolvedMentionUsernames={new Set()} />
        {memo.attachments.length > 0 && (
          <div className="mt-4">
            <div className="text-xs text-muted-foreground mb-2">
              {memo.attachments.length} attachments (lazy load)
            </div>
            <div className="space-y-1">
              {memo.attachments.map((att) => (
                <AttachmentLazyLoader
                  key={att.uid}
                  filename={att.filename}
                  size={att.size}
                  fetchFn={() => fetchAttachment(att.uid)}
                />
              ))}
            </div>
          </div>
        )}
      </div>
      <div className="p-3 border-t flex gap-2">
        <Button variant="ghost" size="sm" onClick={onBack}>
          Back
        </Button>
        <Button size="sm" onClick={handleCopy} disabled={copying} className="ml-auto">
          {copying ? "…" : t("lan.memo.copyToLocal")}
        </Button>
      </div>
    </div>
  );
};

const AttachmentLazyLoader: FC<{
  filename: string;
  size: number;
  fetchFn: () => Promise<{ content: Uint8Array; mime_type: string } | null>;
}> = ({ filename, size, fetchFn }) => {
  const [blobUrl, setBlobUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [loaded, setLoaded] = useState(false);

  const load = async () => {
    setLoading(true);
    const result = await fetchFn();
    setLoading(false);
    if (result) {
      const blob = new Blob([result.content as unknown as BlobPart], { type: result.mime_type });
      const url = URL.createObjectURL(blob);
      setBlobUrl(url);
      setLoaded(true);
    }
  };

  const isImage = blobUrl && (blobUrl.startsWith("blob:") && loaded);

  return (
    <div className="border rounded p-2 text-xs">
      <div className="flex items-center justify-between">
        <span className="truncate">{filename}</span>
        <span className="text-muted-foreground">{(size / 1024).toFixed(1)} KB</span>
      </div>
      {!loaded && (
        <button onClick={load} disabled={loading} className="text-primary mt-1">
          {loading ? "Loading…" : "Load attachment"}
        </button>
      )}
      {isImage && blobUrl && (
        <img src={blobUrl} alt={filename} className="mt-2 max-w-full rounded" />
      )}
    </div>
  );
};

export default RemoteMemoPreview;
