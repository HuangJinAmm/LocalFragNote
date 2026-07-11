# 保存后自动添加标签（方案 A：关键词提取）

## 目标

用户保存 memo 后，后台自动从内容中提取关键词作为 `#tag` 追加到 content 末尾，无需用户手动打标签。

## 架构

- **算法**：TextRank（基于词共现图的无监督关键词提取），纯 Rust 实现，约 150 行，无新依赖
- **触发点**：复用 `create_memo` 已有的 `spawn_blocking` 异步模式，与 embedding 生成并列
- **写入方式**：把 `#tag` 追加到 content 末尾，通过 `update_memo` 写回（mask=`content`）
- **配置开关**：新增 `app_setting` 项 `auto_tag.enabled`（默认开启）+ `auto_tag.max_tags`（默认 3）
- **幂等性**：用 `payload.auto_tagged = true` 标记已处理过的 memo，避免重复追加；用户编辑后清除该标记允许重新生成

## 数据流

```
create_memo
  ├─ 同步：INSERT memo → 返回 Memo 给前端
  └─ spawn_blocking:
      ├─ embed_to_json → INSERT memo_vec   （已有）
      └─ auto_tag(content) → Vec<String>   （新增）
          └─ 若 payload.auto_tagged != true 且设置开启:
              ├─ 追加 "#tag1 #tag2" 到 content
              ├─ UPDATE memo SET content=?, payload=?（payload.auto_tagged=true）
              └─ 重建 embedding + FTS（update_memo 已有逻辑）
```

## 关键设计决策

### 为什么用 TextRank 而非 TF-IDF

- TextRank 考虑词在文档中的位置关系（共现窗口），对短文本（memo 通常 < 500 字）效果优于 TF-IDF
- TextRank 天然带排序（PageRank 迭代后的权重），直接取 Top-N
- 实现复杂度相当，但 TextRank 无需语料库统计

### 为什么追加到 content 而非 payload

- 项目 tag 机制完全基于 content 内联 `#tag`，`extract_tags`、`list_tags`、前端展示都依赖此
- 若存 payload，需改 `toProtoMemo`、`list_tags`、`extract_tags` 等多处，且破坏"tag 即文本"的简洁模型
- 追加到 content 的代价是触发 embedding 二次重建，但 memo 保存是低频操作，可接受

### 幂等性处理

- 首次自动打标签后，在 `payload` 中设置 `auto_tagged: true`
- 若用户后续编辑 content（通过 `update_memo`），清除 `payload.auto_tagged`
- 自动标签任务检查：若 `auto_tagged == true` 则跳过，避免对已处理 memo 重复操作
- 避免自动标签覆盖用户手动添加的 tag：提取已有 tag，只追加不重复的新 tag

### 与 embedding 生成的关系

两者独立运行，无依赖。自动标签用 TextRank（纯文本统计），不依赖 embedding 向量。即使 embedding 模型未下载，自动标签仍可工作。

## 算法细节：TextRank

### 步骤

1. **分词**：按 Unicode 词边界切分（非字母数字作为分隔符），保留长度 > 1 的词
2. **过滤停用词**：内置中英文停用词表（约 100 词），过滤"的、是、the、a、is"等
3. **构建共现图**：窗口大小 K=3，窗口内任意两词连一条边，权重 = 共现次数
4. **PageRank 迭代**：迭代公式 `WS(V_i) = (1-d) + d * Σ_{V_j∈In(V_i)} (w_ji / Σ_{V_k∈Out(V_j)} w_jk) * WS(V_j)`，d=0.85，收敛阈值 0.0001，最大 50 轮
5. **取 Top-N**：按权重降序取前 N 个词作为关键词
6. **规范化为 tag**：转小写，替换空格为 `_`，过滤长度 > MAX_TAG_LENGTH 的词

### 文件结构

```
core/src/text_rank.rs    # TextRank 算法实现（纯函数，可独立测试）
core/src/markdown.rs     # 不改（tag 提取已有）
core/src/lib.rs          # 导出 text_rank 模块
src-tauri/src/commands/memo.rs  # create_memo 集成 auto_tag
src-tauri/src/commands/setting.rs  # auto_tag 配置读写
```

## 配置

存储在 `app_setting` 表，key = `auto_tag`：

```json
{
  "enabled": true,
  "max_tags": 3
}
```

通过现有 `get_app_setting` / `upsert_app_setting` 命令读写（已有，无需新增 IPC）。

## 边界情况

- **空 content**：跳过，不生成 tag
- **极短 content**（< 5 词）：TextRank 退化为直接返回所有词（去停用词后）
- **纯英文/纯中文/混合**：分词器按 Unicode 词边界处理，均支持
- **已有 tag**：`extract_tags` 提取已有，只追加不重复的新 tag
- **TextRank 无结果**：不追加任何 tag，但仍设 `payload.auto_tagged = true` 避免重试
