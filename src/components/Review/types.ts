/** 牌组（笔记集配置） */
export interface ReviewDeck {
  id: number;
  name: string;
  tags: string[];
  cards_per_memo: number;
  created_ts: number;
  last_reviewed_ts: number | null;
  memo_count: number;
}

/** 卡片 */
export interface ReviewCard {
  id: number;
  deck_id: number;
  memo_uid: string;
  card_type: "basic" | "reversed" | "cloze" | "concept" | "compare";
  front: string;
  back: string;
  cloze_answer: string | null;
  angle: string;
  stability: number;
  difficulty: number;
  due: number;
  last_review: number | null;
  reps: number;
  lapses: number;
  state: number; // 0=New 1=Learning 2=Review 3=Relearning
  created_ts: number;
  memo_deleted: boolean;
}

/** 复习记录 */
export interface ReviewRecord {
  id: number;
  card_id: number;
  rating: number; // 1=Again 2=Hard 3=Good 4=Easy
  reviewed_ts: number;
  elapsed_days: number;
  scheduled_days: number;
  state: number;
}

/** 牌组统计 */
export interface DeckStats {
  due_count: number;
  new_count: number;
  total: number;
  learned: number;
  retention_rate: number;
  last_reviewed_ts: number | null;
}

/** 评分结果 */
export interface ScoreResult {
  updated_card: ReviewCard;
  next_card: ReviewCard | null;
  session_stats: SessionStats;
}

/** 会话统计 */
export interface SessionStats {
  reviewed: number;
  again: number;
  hard: number;
  good: number;
  easy: number;
  retention_rate: number;
}

/** 评分等级 */
export type Rating = 1 | 2 | 3 | 4;

/** 卡片类型标签 */
export const CARD_TYPE_LABELS: Record<string, string> = {
  basic: "问答",
  reversed: "翻转",
  cloze: "填空",
  concept: "概念",
  compare: "对比",
};

/** 卡片状态标签 */
export const CARD_STATE_LABELS: Record<number, string> = {
  0: "新卡",
  1: "学习中",
  2: "复习中",
  3: "重学中",
};
