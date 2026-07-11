import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import type { DeckStats, ReviewCard, ReviewDeck, ScoreResult } from "./types";

/** 列出所有 deck */
export function useReviewDecks() {
  const [decks, setDecks] = useState<ReviewDeck[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<ReviewDeck[]>("review_list_decks");
      setDecks(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { decks, loading, error, refresh };
}

/** 获取 deck 统计 */
export function useDeckStats(deckId: number | null) {
  const [stats, setStats] = useState<DeckStats | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (deckId === null) return;
    setLoading(true);
    try {
      const result = await invoke<DeckStats>("review_deck_stats", { deckId });
      setStats(result);
    } catch {
      setStats(null);
    } finally {
      setLoading(false);
    }
  }, [deckId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { stats, loading, refresh };
}

/** 获取到期卡片 */
export function useDueCards(deckId: number | null) {
  const [cards, setCards] = useState<ReviewCard[]>([]);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (deckId === null) return;
    setLoading(true);
    try {
      const result = await invoke<ReviewCard[]>("review_list_due_cards", {
        deckId,
        limit: 100,
      });
      setCards(result);
    } catch {
      setCards([]);
    } finally {
      setLoading(false);
    }
  }, [deckId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { cards, loading, refresh };
}

/** 列出 deck 所有卡片 */
export function useReviewCards(deckId: number | null) {
  const [cards, setCards] = useState<ReviewCard[]>([]);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (deckId === null) return;
    setLoading(true);
    try {
      const result = await invoke<ReviewCard[]>("review_list_cards", { deckId });
      setCards(result);
    } catch {
      setCards([]);
    } finally {
      setLoading(false);
    }
  }, [deckId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { cards, loading, refresh };
}

/** 生成卡片（监听事件流） */
export function useGenerateCards(deckId: number) {
  const [generating, setGenerating] = useState(false);
  const [progress, setProgress] = useState("");
  const [result, setResult] = useState<{ count: number; errors: string[] } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const generate = useCallback(async () => {
    setGenerating(true);
    setProgress("");
    setResult(null);
    setError(null);
    try {
      await invoke("review_generate_cards", { deckId });
    } catch (e) {
      setError(String(e));
      setGenerating(false);
    }
  }, [deckId]);

  useEffect(() => {
    if (!generating) return;
    let unlistenDone: UnlistenFn | null = null;
    let unlistenError: UnlistenFn | null = null;
    let unlistenChunk: UnlistenFn | null = null;

    (async () => {
      unlistenChunk = await listen<{ run_id: number; text: string }>(
        "review:chunk",
        (e) => setProgress((prev) => prev + e.payload.text),
      );
      unlistenDone = await listen<{ deck_id: number; run_id: number; count: number; errors: string[] }>(
        "review:cards-generated",
        (e) => {
          if (e.payload.deck_id === deckId) {
            setResult({ count: e.payload.count, errors: e.payload.errors });
            setGenerating(false);
          }
        },
      );
      unlistenError = await listen<{ deck_id: number; run_id: number; error: string }>(
        "review:generation-error",
        (e) => {
          if (e.payload.deck_id === deckId) {
            setError(e.payload.error);
            setGenerating(false);
          }
        },
      );
    })();

    return () => {
      unlistenDone?.();
      unlistenError?.();
      unlistenChunk?.();
    };
  }, [generating, deckId]);

  return { generating, progress, result, error, generate };
}

/** 评分卡片 */
export function useScoreCard() {
  const [scoring, setScoring] = useState(false);

  const score = useCallback(
    async (cardId: number, rating: number, deckId: number): Promise<ScoreResult | null> => {
      setScoring(true);
      try {
        const result = await invoke<ScoreResult>("review_score_card", {
          cardId,
          rating,
          deckId,
        });
        return result;
      } catch (e) {
        console.error("评分失败:", e);
        return null;
      } finally {
        setScoring(false);
      }
    },
    [],
  );

  return { scoring, score };
}

/** 检查新 memo 数 */
export function useCheckNewMemos(deckId: number) {
  const [newCount, setNewCount] = useState(0);

  const refresh = useCallback(async () => {
    try {
      const count = await invoke<number>("review_check_new_memos", { deckId });
      setNewCount(count);
    } catch {
      setNewCount(0);
    }
  }, [deckId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { newCount, refresh };
}
