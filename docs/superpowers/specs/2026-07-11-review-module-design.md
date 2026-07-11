# 回顾模块设计（AI 记忆卡 + FSRS 调度）

> **日期**: 2026-07-11
> **状态**: 设计确认中
> **关联**: 基于 [iroh LAN 发现](./2026-07-11-iroh-lan-discovery-design.md) 同期的 LocalFragNote 项目

## 1. 目标与范围

### 目标
为 LocalFragNote 新增"回顾模块"：用户通过单个或多个标签选定笔记集，AI 自动生成 ANKI 风格的记忆卡片并持久化存储；用户按 FSRS 算法调度复习，系统记录每次评分并据此安排下次复习时间。答对的卡片可触发"换角度"重新生成，从不同考核点出题。

### 范围
- **包含**: Deck（笔记集配置）管理、AI 卡片生成、FSRS 调度复习、复习记录与统计、前端回顾页面、设置 section
- **不包含**: FSRS 参数优化器（未来可接入 `fsrs-rs` crate）、多用户协作、卡片导入导出

### 核心场景
1. 用户创建 deck（如"Rust 基础"，tags=`["rust"]`）
2. 点"生成卡片" → AI 读取该 tag 下所有笔记，生成 1-N 张卡片/笔记
3. 点"开始复习" → 展示到期卡片，用户自评（Again/Hard/Good/Easy）
4. FSRS 根据评分更新卡片调度，记录复习历史
5. 答对的卡片可点"换角度" → AI 从新考核点重新生成

## 2. 架构与模块划分

### 方案选择
采用**方案 B：预生成 + 持久化卡片**。卡片由 AI 一次性批量生成存库，复习时直接展示（毫秒级响应），AI 仅在生成/换角度时调用。优势：复习快、可离线、有完整历史记录支撑调度。

### 模块划分

| 层 | 文件 | 职责 |
|---|---|---|
| core 数据层 | `core/src/review.rs` | `ReviewDeck` / `ReviewCard` / `ReviewRecord` 实体 + CRUD + FSRS 调度封装 |
| core 迁移 | `core/migrations/V5__add_review_module.sql` | 建 3 张表 |
| 命令层 | `src-tauri/src/commands/review.rs` | 12 个 Tauri 命令 |
| AI 工具扩展 | `src-tauri/src/ai/tools.rs`（修改） | 新增 `list_memos_by_tag` 工具 |
| AI 卡片生成 | `src-tauri/src/commands/review.rs` 内 `review_generate_cards` | 复用 `agent_loop` + 专用 system prompt |
| 前端页面 | `src/pages/Review.tsx` | 回顾主界面 |
| 前端组件 | `src/components/Review/` | DeckList / DeckEditor / DeckDetail / CardReview / CardTable / GenerationProgress / DeckStats |
| 前端设置 | `src/components/Settings/ReviewSection.tsx` | 默认参数配置 |
| 路由 | `src/router/routes.ts`（修改） | 新增 `REVIEW: "/review"` |

### 关键设计决策
1. **复用现有 AI agent loop**：不新写 HTTP 调用，复用 `ai_chat.rs` 的 `agent_loop`（注入专用 system prompt + `list_memos_by_tag` 工具），通过 Tauri 事件流式推送生成进度
2. **卡片与 memo 弱关联**：`review_card.memo_uid` 仅记录来源，memo 删除时卡片保留但标记 `memo_deleted=1`，回顾时提示
3. **FSRS 算法在 core 层**：通过 `rs-fsrs` crate 实现，`ReviewCard` ↔ `rs_fsrs::Card` 通过 `From`/`Into` 转换
4. **无 SQL 外键约束**：与现有 attachment 模式一致，软关联

## 3. 卡片类型设计

### 5 种卡片类型

| 类型 | code | 正面（front） | 背面（back） | 适用场景 | AI 生成逻辑 |
|---|---|---|---|---|---|
| 基础问答 | `basic` | 问题 | 答案 | 通用知识点 | 直接从笔记提取 Q&A |
| 翻转卡 | `reversed` | 术语/概念 | 定义+解释 | 术语↔定义双向 | 对同一知识点生成正反两张卡 |
| 填空题 | `cloze` | 带 `{{...}}` 占位的句子 | 被删除的词 | 完整定义/公式/列表 | 找关键术语挖空 |
| 概念解释 | `concept` | "请解释：X" | 完整解释 | 理解型知识 | 提取核心概念 |
| 对比题 | `compare` | "对比 A 和 B" | 异同点列表 | 易混淆概念 | 识别笔记中并列/对比的内容 |

### "调整考核点"实现
用户答对后点"换角度" → AI 重新读该 memo，指定生成不同 `angle` 的卡片（如已答对"定义"角度，下次生成"应用场景"角度的 `concept` 卡）。新卡作为独立卡片入库，旧卡保留历史记录。

### AI 生成规则
- AI 收到 memo 内容后自主判断最合适的 `card_type` 和 `angle`
- 一条 memo 可生成 1-N 张卡（默认 1-3 张，deck 配置 `cards_per_memo` 上限）
- 每张卡的 front/back 必须独立完整，不依赖其他卡
- 同一 memo 的不同卡片应覆盖不同 angle

## 4. 数据模型与 SQLite Schema

### V5 迁移

```sql
-- V5__add_review_module.sql

-- 牌组（笔记集配置）
CREATE TABLE review_deck (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',            -- JSON 数组，如 ["rust","ai"]
    cards_per_memo INTEGER NOT NULL DEFAULT 2,  -- 每条 memo 生成卡片上限
    created_ts BIGINT NOT NULL DEFAULT (strftime('%s', 'now')),
    last_reviewed_ts BIGINT,
    memo_count INTEGER NOT NULL DEFAULT 0       -- 上次生成时的 memo 数（检测新增用）
);

-- 卡片
CREATE TABLE review_card (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    deck_id INTEGER NOT NULL,
    memo_uid TEXT NOT NULL,
    card_type TEXT NOT NULL,                    -- basic|reversed|cloze|concept|compare
    front TEXT NOT NULL,
    back TEXT NOT NULL,
    cloze_answer TEXT,                          -- cloze 类型的答案词
    angle TEXT NOT NULL DEFAULT '',             -- 考核点：定义|应用|对比|列举...
    -- FSRS 字段
    stability REAL NOT NULL DEFAULT 0,
    difficulty REAL NOT NULL DEFAULT 0,
    due BIGINT NOT NULL DEFAULT (strftime('%s', 'now')),
    last_review BIGINT,
    reps INTEGER NOT NULL DEFAULT 0,
    lapses INTEGER NOT NULL DEFAULT 0,
    state INTEGER NOT NULL DEFAULT 0,           -- 0=New 1=Learning 2=Review 3=Relearning
    -- 元数据
    created_ts BIGINT NOT NULL DEFAULT (strftime('%s', 'now')),
    memo_deleted INTEGER NOT NULL DEFAULT 0     -- 0/1
);
CREATE INDEX idx_review_card_deck_id ON review_card(deck_id);
CREATE INDEX idx_review_card_due ON review_card(due);
CREATE INDEX idx_review_card_memo_uid ON review_card(memo_uid);

-- 复习记录（FSRS ReviewLog）
CREATE TABLE review_record (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    card_id INTEGER NOT NULL,
    rating INTEGER NOT NULL,                    -- 1=Again 2=Hard 3=Good 4=Easy
    reviewed_ts BIGINT NOT NULL,
    elapsed_days REAL NOT NULL DEFAULT 0,       -- 距上次复习的天数（含小数）
    scheduled_days REAL NOT NULL DEFAULT 0,     -- 上次安排的间隔天数
    state INTEGER NOT NULL                      -- 评分时的卡片状态
);
CREATE INDEX idx_review_record_card_id ON review_record(card_id);
```

### Core 实体

```rust
// core/src/review.rs
use rs_fsrs::{FSRS, Card as FsrsCard, Rating, ReviewLog};
use chrono::{DateTime, Utc};

pub struct ReviewDeck {
    pub id: i32,
    pub name: String,
    pub tags: Vec<String>,           // serde JSON ↔ DB TEXT
    pub cards_per_memo: i32,
    pub created_ts: i64,
    pub last_reviewed_ts: Option<i64>,
    pub memo_count: i32,
}

pub struct ReviewCard {
    pub id: i32,
    pub deck_id: i32,
    pub memo_uid: String,
    pub card_type: String,           // basic|reversed|cloze|concept|compare
    pub front: String,
    pub back: String,
    pub cloze_answer: Option<String>,
    pub angle: String,
    // FSRS 字段
    pub stability: f32,
    pub difficulty: f32,
    pub due: i64,
    pub last_review: Option<i64>,
    pub reps: u32,
    pub lapses: u32,
    pub state: u8,                   // 0=New 1=Learning 2=Review 3=Relearning
    pub created_ts: i64,
    pub memo_deleted: bool,
}

pub struct ReviewRecord {
    pub id: i32,
    pub card_id: i32,
    pub rating: u8,                  // 1-4
    pub reviewed_ts: i64,
    pub elapsed_days: f32,
    pub scheduled_days: f32,
    pub state: u8,
}

// FSRS 转换
impl From<&ReviewCard> for FsrsCard {
    fn from(c: &ReviewCard) -> Self {
        FsrsCard {
            stability: c.stability,
            difficulty: c.difficulty,
            due: DateTime::from_timestamp(c.due, 0).unwrap_or_else(Utc::now),
            last_review: c.last_review
                .and_then(|ts| DateTime::from_timestamp(ts, 0)),
            reps: c.reps,
            lapses: c.lapses,
            state: rs_fsrs::State::try_from(c.state).unwrap_or(rs_fsrs::State::New),
        }
    }
}
```

## 5. AI 卡片生成流程

### 流程

```
用户点"生成卡片"（指定 deck）
  ↓
Tauri 命令 review_generate_cards(deck_id)
  ↓
1. 读取 deck.tags + cards_per_memo
2. 构造 messages = [system_prompt, user_prompt]
3. 调用 ai_chat::agent_loop(provider_id, messages)
   - agent_loop 自动调用 list_memos_by_tag 工具读取笔记
   - AI 返回 JSON 数组格式的卡片
4. 解析 AI 输出 → 批量插入 review_card 表
5. emit("review:cards-generated", { deck_id, count })
```

### AI 工具扩展

`tools.rs` 新增 `list_memos_by_tag` 工具：

```rust
// tool_definitions() 新增
{
    "name": "list_memos_by_tag",
    "description": "List memos that contain ALL specified tags. Returns memo content for card generation.",
    "parameters": {
        "type": "object",
        "properties": {
            "tags": { "type": "array", "items": { "type": "string" } },
            "limit": { "type": "integer", "default": 50 }
        },
        "required": ["tags"]
    }
}

// execute_tool match 分支
"list_memos_by_tag" => {
    let tags: Vec<String> = args["tags"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let limit = args["limit"].as_i64().unwrap_or(50) as i32;
    let find = FindMemo {
        tag_search: tags.clone(),
        row_status: RowStatus::Normal,
        limit: Some(limit),
        ..Default::default()
    };
    let memos = memo::list(conn, &find)?;
    // 返回 [{uid, content, tags, created_ts}]
}
```

### System Prompt

```
你是一个记忆卡片生成专家。根据用户的笔记内容，生成 ANKI 风格的记忆卡片。

## 卡片类型
- basic: 问答卡（正面问题，背面答案）
- reversed: 翻转卡（正面术语，背面定义）
- cloze: 填空卡（front 带 {{答案}} 占位，cloze_answer 存答案词）
- concept: 概念解释卡（"请解释：X" → 完整解释）
- compare: 对比卡（"对比 A 和 B" → 异同点）

## 输出格式
返回 JSON 数组，每个元素：
{
  "memo_uid": "来源 memo 的 uid",
  "card_type": "basic|reversed|cloze|concept|compare",
  "front": "正面内容（Markdown）",
  "back": "背面内容（Markdown）",
  "cloze_answer": "填空答案（仅 cloze 类型）",
  "angle": "考核点，如：定义|应用|对比|列举|原理"
}

## 规则
1. 每条 memo 最多生成 {cards_per_memo} 张卡片
2. 优先生成核心知识点，避免琐碎细节
3. front/back 必须独立完整，不依赖其他卡片
4. 同一 memo 的不同卡片应覆盖不同 angle
5. 只返回 JSON 数组，不要其他文字
```

### User Prompt

```
请为以下标签的笔记生成记忆卡片：
- 标签：{tags}
- 每条笔记最多生成 {cards_per_memo} 张卡片

使用 list_memos_by_tag 工具读取笔记内容。
```

### 进度推送事件

| 事件 | 时机 | payload |
|---|---|---|
| `review:generation-started` | agent_loop 开始 | `{ deck_id, run_id }` |
| `review:chunk` | AI 流式输出 | `{ run_id, text }` |
| `review:tool` | AI 调用工具 | `{ run_id, name, args }` |
| `review:cards-generated` | 解析完成 | `{ deck_id, count, errors? }` |
| `review:generation-error` | 失败 | `{ deck_id, error }` |

### "换角度"重新生成

1. 读取该卡片关联的 `memo_uid`
2. 构造 prompt：`"已存在卡片角度：[定义, 原理]。请为 memo {uid} 生成不同角度的卡片，避免重复已存在的 angle。"`
3. 调用 `agent_loop`，生成新卡插入 DB
4. 旧卡保留（不删除），新卡独立调度

### 错误处理

- **AI 返回非 JSON**：尝试提取 JSON 片段（正则 `\[[\s\S]*\]`），失败则报错
- **memo_uid 不匹配**：跳过该卡片，记录 warning
- **provider 未配置**：返回明确错误提示，引导用户去设置页配置 AI provider

## 6. 复习流程与 FSRS 调度

### 复习流程

```
用户进入 /review → 选择 deck → 点"开始复习"
  ↓
1. 查询 due <= now 的卡片（按 due 升序）
2. 若无到期卡片，提示"今日复习完成"
3. 展示卡片正面
  ↓
用户思考 → 点"显示答案"
  ↓
展示卡片背面 + 4 个评分按钮：Again / Hard / Good / Easy
  ↓
用户评分 → review_score_card(card_id, rating)
  ↓
后端：
  1. card → FsrsCard 转换
  2. fsrs.repeat(card, now)[rating] → 新 Card + ReviewLog
  3. 更新 review_card 表
  4. 插入 review_record 表
  5. 返回下一张卡片
```

### FSRS 调度实现

```rust
pub fn score_card(
    card: &mut ReviewCard,
    rating: u8,
    now: DateTime<Utc>,
    fsrs_params: &[f32],
) -> ReviewRecord {
    let fsrs = if fsrs_params.is_empty() {
        FSRS::default()
    } else {
        FSRS::new(Some(fsrs_params.to_vec()))
    };

    let fsrs_card: FsrsCard = card.into();
    let rating = Rating::try_from(rating).unwrap_or(Rating::Good);
    let record_log = fsrs.repeat(fsrs_card, now);

    let item = &record_log[rating];
    let new_card = &item.card;
    let log = &item.review_log;

    card.stability = new_card.stability;
    card.difficulty = new_card.difficulty;
    card.due = new_card.due.timestamp();
    card.last_review = Some(now.timestamp());
    card.reps = new_card.reps;
    card.lapses = new_card.lapses;
    card.state = new_card.state as u8;

    ReviewRecord {
        card_id: card.id,
        rating: rating as u8,
        reviewed_ts: now.timestamp(),
        elapsed_days: log.elapsed_days,
        scheduled_days: log.scheduled_days,
        state: log.state as u8,
    }
}
```

### 查询逻辑

```rust
// 到期卡片查询
pub fn list_due_cards(conn: &Connection, deck_id: i32, limit: i32) -> Result<Vec<ReviewCard>> {
    let now = Utc::now().timestamp();
    // WHERE deck_id=? AND due<=now AND memo_deleted=0 ORDER BY due ASC LIMIT ?
}

// 统计
pub fn deck_stats(conn: &Connection, deck_id: i32) -> Result<DeckStats> {
    // due_count: due<=now
    // new_count: state=0
    // total: count(*)
    // learned: reps>0
    // retention_rate: 最近7天 (Good+Easy) / 总评分
}
```

### 新卡处理
FSRS `Card::new()` 默认 `state=New`、`due=now`。首次评分：
- `Again` → Learning，几分钟后
- `Good` → Review，1 天后

**每日新卡上限**（默认 20）：`list_due_cards` 先取到期 Review/Learning 卡片，再补充 New 卡至上限。

### 评分按钮语义

| 按钮 | rating | 含义 | 典型效果（Review 状态） |
|---|---|---|---|
| Again | 1 | 完全忘了 | lapses+1，Relearning，10分钟后 |
| Hard | 2 | 勉强记得 | interval × 1.2，难度+0.1 |
| Good | 3 | 正常记得 | interval × stability |
| Easy | 4 | 轻松记得 | interval × 1.3，难度-0.2 |

## 7. 前端界面与交互

### 路由

```
/review                    → Deck 列表页
/review/:deckId            → Deck 详情页
/review/:deckId/study      → 复习界面
```

### 页面布局

**Deck 列表页**：deck 卡片网格，每个 deck 显示名称、tags、今日到期数、总数、掌握率、`[开始复习]` / `[生成卡片]` 按钮，右上角 `[+]` 新建 deck。

**Deck 详情页**：deck 信息 + 统计卡片 + 操作区（生成卡片/开始复习/换角度）+ 卡片管理表格（front/type/angle/due/state/reps，可删除）。

**复习界面**：
```
┌─────────────────────────────────────────┐
│  ← 返回    Rust 基础   3/12             │  ← 进度指示
├─────────────────────────────────────────┤
│         ┌───────────────────┐           │
│         │   什么是所有权？   │           │  ← 卡片正面
│         └───────────────────┘           │
│         [ 显示答案 ]                    │
├─────────────────────────────────────────┤
│  Again   Hard   Good   Easy            │  ← 翻面后显示
└─────────────────────────────────────────┘
```

### 组件树

```
src/pages/Review.tsx
src/components/Review/
├── DeckList.tsx          ← deck 卡片网格
├── DeckEditor.tsx        ← 新建/编辑 deck
├── DeckDetail.tsx        ← deck 详情页
├── DeckStats.tsx         ← 统计卡片
├── CardReview.tsx        ← 复习界面（翻转卡+评分）
├── CardTable.tsx         ← 卡片管理表格
├── GenerationProgress.tsx ← AI 生成进度
└── hooks.ts              ← useReviewDecks / useDueCards / useGenerateCards
```

### 交互细节
- **键盘快捷键**：`Space` 翻面、`1` Again、`2` Hard、`3` Good、`4` Easy
- **翻转动画**：CSS 3D transform（rotateY）
- **进度条**：已复习/总到期
- **完成画面**：本次统计（答对/答错/用时）+ 返回/继续
- **中断恢复**：复习中途离开，下次进入同一 deck 自动继续
- **Markdown 渲染**：复用 `MemoMarkdownRenderer`

### 设置 Section

在 `settingSections.ts` 新增 `review` section：
- 默认每日新卡上限（默认 20）
- 默认每条 memo 卡片数（默认 2）
- FSRS 参数（只读显示"使用默认参数"）
- AI Provider 选择（复用 AI Chat 的 provider 配置）

## 8. 命令接口

### Tauri 命令清单

```rust
// Deck 管理
review_list_decks() -> Vec<ReviewDeck>
review_create_deck(name: String, tags: Vec<String>, cards_per_memo: i32) -> ReviewDeck
review_update_deck(id: i32, name: String, tags: Vec<String>, cards_per_memo: i32) -> ReviewDeck
review_delete_deck(id: i32) -> ()  // 级联删除 card + record

// 卡片管理
review_list_cards(deck_id: i32) -> Vec<ReviewCard>
review_list_due_cards(deck_id: i32, limit: Option<i32>) -> Vec<ReviewCard>
review_delete_card(card_id: i32) -> ()

// 复习操作
review_score_card(card_id: i32, rating: u8) -> ScoreResult
review_regenerate_card(card_id: i32) -> u32  // "换角度"，返回 run_id

// AI 生成
review_generate_cards(deck_id: i32) -> u32  // 返回 run_id，异步
review_check_new_memos(deck_id: i32) -> i32  // 新 memo 数

// 统计
review_deck_stats(deck_id: i32) -> DeckStats
```

### 返回类型

```rust
#[derive(Serialize)]
pub struct ScoreResult {
    pub updated_card: ReviewCard,
    pub next_card: Option<ReviewCard>,
    pub session_stats: SessionStats,
}

#[derive(Serialize)]
pub struct SessionStats {
    pub reviewed: u32,
    pub again: u32,
    pub hard: u32,
    pub good: u32,
    pub easy: u32,
    pub retention_rate: f32,
}

#[derive(Serialize)]
pub struct DeckStats {
    pub due_count: i32,
    pub new_count: i32,
    pub total: i32,
    pub learned: i32,
    pub retention_rate: f32,
    pub last_reviewed_ts: Option<i64>,
}
```

### 错误处理

`IpcError` 新增 `Review(String)` 变体 + `From<ReviewError>` 实现。

| 场景 | 错误信息 | 前端行为 |
|---|---|---|
| AI provider 未配置 | "未配置 AI provider，请先在设置中配置" | toast + 跳转设置页 |
| AI 生成失败 | "AI 生成失败：{detail}" | toast + 保留已生成卡片 |
| AI 返回格式错误 | "AI 输出解析失败，已跳过 N 张无效卡片" | toast warning |
| Deck 不存在 | "牌组不存在" | 返回 deck 列表页 |
| 卡片不存在 | "卡片不存在" | 跳过，显示下一张 |
| FSRS 参数无效 | "FSRS 参数无效，使用默认参数" | fallback + 日志 |

### app_setting 键

```rust
"review_config" → {
    "daily_new_card_limit": 20,
    "default_cards_per_memo": 2,
    "ai_provider_id": ""  // 空=使用 AI Chat 的活跃 provider
}
"fsrs_params" → []  // 空=默认参数
```

## 9. 依赖

### Cargo 依赖

```toml
# src-tauri/Cargo.toml
[dependencies]
rs-fsrs = "1.2.1"  # FSRS 调度算法
# chrono 已在依赖中
```

`core/Cargo.toml` 也需添加 `rs-fsrs`（review.rs 在 core 层）。

## 10. 测试策略

### Core 层单元测试（TDD）

```rust
// 卡片转换
test_review_card_to_fsrs_card_roundtrip

// 评分调度
test_score_card_new_again_enters_learning     // New → Again → Learning
test_score_card_new_good_enters_review        // New → Good → Review, due+1d
test_score_card_review_again_lapses           // Review → Again → Relearning, lapses+1
test_score_card_increments_reps
test_score_card_records_review_log

// 到期查询
test_list_due_cards_excludes_future
test_list_due_cards_excludes_deleted_memo
test_list_due_cards_orders_by_due_asc

// 统计
test_deck_stats_retention_rate
test_deck_stats_new_count

// Deck CRUD
test_create_deck_stores_tags_json
test_delete_deck_cascades_cards_and_records
```

### AI 工具测试

```rust
test_list_memos_by_tag_filters_correctly      // 含所有指定 tag
test_list_memos_by_tag_excludes_archived
test_list_memos_by_tag_respects_limit
```

### 不测试
- FSRS 算法本身（`rs-fsrs` crate 已有测试）
- AI 输出质量
- 前端 UI（手动验证）
