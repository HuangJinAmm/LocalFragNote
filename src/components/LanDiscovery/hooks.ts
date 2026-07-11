import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  PeerInfo,
  RemoteProfile,
  RemoteMemoSummary,
  RemoteMemo,
  RemoteAttachmentResponse,
} from "./types";

// ---------- useLanDiscovery ----------

export function useLanDiscovery() {
  const [peers, setPeers] = useState<PeerInfo[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

    const init = async () => {
      try {
        const result = await invoke<PeerInfo[]>("lan_discover_peers");
        setPeers(result);
      } catch (e) {
        console.error("lan_discover_peers failed", e);
      } finally {
        setLoading(false);
      }

      unlisten = await listen("lan:peers-changed", async () => {
        try {
          const result = await invoke<PeerInfo[]>("lan_discover_peers");
          setPeers(result);
        } catch (e) {
          console.error("lan_discover_peers refresh failed", e);
        }
      });
    };

    init();
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const refresh = useCallback(async () => {
    try {
      const result = await invoke<PeerInfo[]>("lan_discover_peers");
      setPeers(result);
    } catch (e) {
      console.error("lan_discover_peers refresh failed", e);
    }
  }, []);

  return { peers, loading, refresh };
}

// ---------- useRemoteProfile ----------

export function useRemoteProfile(peerId: string | null) {
  const [profile, setProfile] = useState<RemoteProfile | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!peerId) {
      setProfile(null);
      return;
    }
    setLoading(true);
    setError(null);
    invoke<RemoteProfile>("lan_get_remote_profile", { peerId })
      .then((p) => setProfile(p))
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [peerId]);

  return { profile, loading, error };
}

// ---------- useRemoteMemos ----------

export function useRemoteMemos(peerId: string | null) {
  const [memos, setMemos] = useState<RemoteMemoSummary[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tagFilter, setTagFilter] = useState<string[] | null>(null);
  const offsetRef = useRef(0);
  const PAGE_SIZE = 20;

  const loadPage = useCallback(
    async (reset: boolean) => {
      if (!peerId) return;
      setLoading(true);
      setError(null);
      const offset = reset ? 0 : offsetRef.current;
      try {
        const res = await invoke<{
          memos: RemoteMemoSummary[];
          total: number;
        }>("lan_list_remote_memos", {
          req: {
            peer_id: peerId,
            offset,
            limit: PAGE_SIZE,
            tag_filter: tagFilter,
          },
        });
        if (reset) {
          setMemos(res.memos);
          offsetRef.current = res.memos.length;
        } else {
          setMemos((prev) => [...prev, ...res.memos]);
          offsetRef.current += res.memos.length;
        }
        setTotal(res.total);
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    },
    [peerId, tagFilter],
  );

  useEffect(() => {
    offsetRef.current = 0;
    loadPage(true);
  }, [peerId, tagFilter, loadPage]);

  const loadMore = useCallback(() => {
    if (!loading && memos.length < total) {
      loadPage(false);
    }
  }, [loading, memos.length, total, loadPage]);

  const hasMore = memos.length < total;

  return {
    memos,
    total,
    loading,
    error,
    tagFilter,
    setTagFilter,
    loadMore,
    hasMore,
    retry: () => loadPage(true),
  };
}

// ---------- useRemoteMemoPreview ----------

export function useRemoteMemoPreview(peerId: string | null, uid: string | null) {
  const [memo, setMemo] = useState<RemoteMemo | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!peerId || !uid) {
      setMemo(null);
      return;
    }
    setLoading(true);
    setError(null);
    invoke<RemoteMemo>("lan_get_remote_memo", {
      req: { peer_id: peerId, uid },
    })
      .then((m) => setMemo(m))
      .catch((e) => {
        setError(String(e));
        setMemo(null);
      })
      .finally(() => setLoading(false));
  }, [peerId, uid]);

  const fetchAttachment = useCallback(
    async (attUid: string): Promise<RemoteAttachmentResponse | null> => {
      if (!peerId) return null;
      try {
        return await invoke<RemoteAttachmentResponse>("lan_get_remote_attachment", {
          req: { peer_id: peerId, uid: attUid },
        });
      } catch (e) {
        console.error("fetchAttachment failed", e);
        return null;
      }
    },
    [peerId],
  );

  return { memo, loading, error, fetchAttachment };
}
