// IPC 适配层：proto API ↔ Rust IPC 命令的参数/返回值转换
import { invoke } from "@tauri-apps/api/core";

// ============ 工具函数 ============

// "memos/abc123" → "abc123"
function extractUid(name: string): string {
  const parts = name.split("/");
  return parts[parts.length - 1] || "";
}

// proto State 枚举：0=UNSPECIFIED, 1=NORMAL, 2=ARCHIVED
// Rust row_status: "NORMAL" | "ARCHIVED"
function rowStatusToState(rowStatus: string): number {
  return rowStatus === "ARCHIVED" ? 2 : 1;
}

// proto State → Rust row_status 字符串
// state=0 (UNSPECIFIED) 时不应该调用此函数（调用方需处理）
function stateToRowStatus(state: number): string {
  return state === 2 ? "ARCHIVED" : "NORMAL";
}

// Rust visibility 字符串 → proto Visibility 数字
// proto: 0=UNSPECIFIED, 1=PRIVATE, 2=PROTECTED, 3=PUBLIC
function rustVisToProto(vis: string): number {
  switch (vis) {
    case "PUBLIC": return 3;
    case "PROTECTED": return 2;
    default: return 1;
  }
}

// proto Visibility 数字 → Rust visibility 字符串
function protoVisToRust(vis: number | undefined): string {
  switch (vis) {
    case 3: return "PUBLIC";
    case 2: return "PROTECTED";
    default: return "PRIVATE";
  }
}

// epoch seconds → proto Timestamp { seconds: bigint, nanos: 0 }
function ts(seconds: number): { seconds: bigint; nanos: number } {
  return { seconds: BigInt(seconds), nanos: 0 };
}

// 从 markdown 内容提取 tags
function extractTags(content: string): string[] {
  const tags = new Set<string>();
  const regex = /(?:^|\s)#([\w\u4e00-\u9fa5-]+)/g;
  let m;
  while ((m = regex.exec(content)) !== null) {
    tags.add(m[1]);
  }
  return Array.from(tags);
}

// 生成纯文本摘要
function snippet(content: string, max = 200): string {
  return content.replace(/[#*`>\-[\]()!]/g, "").replace(/\s+/g, " ").trim().slice(0, max);
}

// ============ Rust Memo → Proto Memo ============

// Rust MemoRelationType 字符串 → proto 数字（0=UNSPECIFIED, 1=REFERENCE, 2=COMMENT）
function rustRelTypeToProto(t: string): number {
  if (t === "REFERENCE") return 1;
  if (t === "COMMENT") return 2;
  return 0;
}

// proto 数字 → Rust MemoRelationType 字符串
function protoRelTypeToRust(t: number): string {
  if (t === 1) return "REFERENCE";
  if (t === 2) return "COMMENT";
  return "REFERENCE";
}

// Rust MemoRelation → proto MemoRelation
// idToUid：memo_id → uid 映射，用于把数字 id 转为 proto name "memos/{uid}"
function toProtoRelation(r: any, idToUid?: Map<number, string>): any {
  const memoName = idToUid && idToUid.has(r.memo_id)
    ? `memos/${idToUid.get(r.memo_id)}`
    : `memos/${r.memo_id}`;
  const relatedName = idToUid && idToUid.has(r.related_memo_id)
    ? `memos/${idToUid.get(r.related_memo_id)}`
    : `memos/${r.related_memo_id}`;
  return {
    type: rustRelTypeToProto(r.type ?? r["r#type"] ?? "REFERENCE"),
    memo: { name: memoName },
    relatedMemo: { name: relatedName },
  };
}

/// 为 relations 建立 id→uid 映射：已知 memo 加入映射，缺失的 memo 批量查询补充
async function buildIdToUidMap(
  knownMemos: { id: number; uid: string }[],
  rawRels: any[],
): Promise<Map<number, string>> {
  const idToUid = new Map<number, string>();
  for (const m of knownMemos) {
    idToUid.set(m.id, m.uid);
  }
  const missingIds = new Set<number>();
  for (const raw of rawRels) {
    if (!idToUid.has(raw.memo_id)) missingIds.add(raw.memo_id);
    if (!idToUid.has(raw.related_memo_id)) missingIds.add(raw.related_memo_id);
  }
  if (missingIds.size > 0) {
    const missingMemos = await invoke<{ memos: any[] }>("list_memos", {
      req: { id_list: [...missingIds], limit: missingIds.size, exclude_content: true },
    });
    for (const m of missingMemos.memos ?? []) {
      idToUid.set(m.id, m.uid);
    }
  }
  return idToUid;
}

function toProtoMemo(m: any, attachments: any[] = [], relations: any[] = []): any {
  return {
    name: `memos/${m.uid}`,
    state: rowStatusToState(m.row_status),
    creator: "users/local",
    createTime: ts(m.created_ts),
    updateTime: ts(m.updated_ts),
    content: m.content || "",
    visibility: rustVisToProto(m.visibility),
    tags: extractTags(m.content || ""),
    pinned: m.pinned ?? false,
    attachments,
    relations,
    reactions: [],
    property: { hasLink: false, hasTaskList: false, hasCode: false, hasIncompleteTasks: false, title: "" },
    snippet: snippet(m.content || ""),
    location: m.location
      ? {
          placeholder: m.location.placeholder ?? "",
          latitude: m.location.latitude ?? 0,
          longitude: m.location.longitude ?? 0,
        }
      : undefined,
  };
}

// ============ Rust Attachment → Proto Attachment ============

function toProtoAttachment(a: any): any {
  return {
    name: `attachments/${a.uid}`,
    uid: a.uid,
    createTime: ts(a.created_ts),
    updateTime: ts(a.updated_ts),
    filename: a.filename,
    content: new Uint8Array(), // 列表/详情不返回 content，避免大对象传输
    externalLink: "",
    type: a["type"] || a.type,
    size: a.size != null ? BigInt(a.size) : 0n,
    // proto memo 格式 "memos/{uid}"，Rust memo_id 是 i32
    // 用数字 id 占位（前端若需 uid 可另行查询）
    memo: a.memo_id ? `memos/${a.memo_id}` : undefined,
  };
}

// ============ CEL filter 解析 ============

function parseFilter(filter: string): Record<string, unknown> {
  const r: Record<string, unknown> = {};
  if (!filter) return r;

  // content.contains("xxx") — 保留向后兼容（若旧 filter 残留）
  const cm = filter.match(/content\.contains\("((?:[^"\\]|\\.)*)"\)/);
  if (cm) r.content_contains = cm[1].replace(/\\"/g, '"');

  // fts.match("xxx") — FTS5 全文搜索
  const fm = filter.match(/fts\.match\("((?:[^"\\]|\\.)*)"\)/);
  if (fm) {
    const raw = fm[1].replace(/\\"/g, '"');
    const words = raw.split(/\s+/).filter((w) => w.length > 0);
    // trigram 分词器要求 token >= 3 字符；短词 fallback 到 LIKE 查询
    const hasShortWord = words.some((w) => w.length < 3);
    if (hasShortWord) {
      r.content_contains = raw;
    } else {
      // FTS5 AND 查询：每个词用 phrase 包裹（转义特殊字符），空格连接 = 隐式 AND
      r.fts_query = words.map((w) => `"${w.replace(/"/g, '""')}"`).join(" ");
    }
  }

  // semantic.search("xxx") — 语义搜索（需后续调 embed_text）
  const sm = filter.match(/semantic\.search\("((?:[^"\\]|\\.)*)"\)/);
  if (sm) r.semantic_query = sm[1].replace(/\\"/g, '"');

  // tag in ["xxx", "yyy"]
  const tm = filter.match(/tag\s+in\s+\[([^\]]+)\]/);
  if (tm) {
    r.tag_search = (tm[1].match(/"([^"]+)"/g) || []).map((s) => s.slice(1, -1));
  }

  // created_ts >= timestamp(123)
  const ta = filter.match(/created_ts\s*>=\s*timestamp\((\d+)\)/);
  if (ta) r.created_ts_after = parseInt(ta[1], 10);

  // created_ts < timestamp(456)
  const tb = filter.match(/created_ts\s*<\s*timestamp\((\d+)\)/);
  if (tb) r.created_ts_before = parseInt(tb[1], 10);

  // visibility in ["PUBLIC", "PROTECTED"]
  const vm = filter.match(/visibility\s+in\s+\[([^\]]+)\]/);
  if (vm) {
    r.visibility_list = (vm[1].match(/"([^"]+)"/g) || []).map((s) => s.slice(1, -1));
  }

  return r;
}

function parseOrderBy(orderBy: string): Record<string, unknown> {
  const r: Record<string, unknown> = {};
  if (!orderBy) return r;
  if (orderBy.includes("pinned")) r.order_by_pinned = true;
  if (orderBy.includes("update_time")) {
    r.order_by_updated_ts = true;
    r.order_by_time_asc = orderBy.includes("asc");
  } else {
    r.order_by_time_asc = orderBy.includes("asc");
  }
  return r;
}

// ============ Memo Service 适配 ============

export const memoServiceClient = {
  async listMemos(req: any): Promise<any> {
    const limit = req.pageSize || 50;
    const offset = req.pageToken ? parseInt(req.pageToken, 10) : 0;
    const rustReq: any = {
      limit,
      offset,
      exclude_content: false,
      ...parseFilter(req.filter || ""),
      ...parseOrderBy(req.orderBy || ""),
    };
    if (req.state !== undefined && req.state !== 0) {
      rustReq.row_status = stateToRowStatus(req.state);
    }

    // 语义搜索：先获取 embedding，再传给 list_memos
    // 注意：KNN 返回的是固定 top_k 候选集，对它们施加 OFFSET 会直接跳过结果，
    // 因此语义搜索不使用 offset 分页——一次性取足 top_k，前端按需切片。
    if (rustReq.semantic_query) {
      const embedding = await invoke<string>("embed_text", { text: rustReq.semantic_query });
      rustReq.vector_embedding = embedding;
      // 取候选集上限：单页 limit 的 4 倍，给后续过滤（可见性/时间/tag 等）留余量
      const candidateK = Math.max(limit * 4, 50);
      rustReq.vector_top_k = candidateK;
      // 语义搜索禁用 offset：KNN 结果固定，offset 会跳过全部
      rustReq.offset = 0;
      // limit 保留为前端请求的 pageSize，让 Rust 层再切片到本页大小
      delete rustReq.semantic_query;
    }

    const res = await invoke<{ memos: any[]; total: number }>("list_memos", { req: rustReq });
    const memoIds: number[] = res.memos.map((m: any) => m.id);

    // 批量查询 attachments 和 relations，避免 N+1
    let rawAtts: any[] = [];
    let rawRels: any[] = [];
    if (memoIds.length > 0) {
      const [attRes, relRes] = await Promise.all([
        invoke<any[]>("list_attachments", { req: { memo_id_list: memoIds } }),
        invoke<any[]>("list_memo_relations", { req: { memo_id_list: memoIds } }),
      ]);
      rawAtts = attRes;
      rawRels = relRes;
    }

    // 按原始 memo_id 分组 attachments（AttachmentWithBlob flatten 后 memo_id 在顶层）
    const attsByMemoId = new Map<number, any[]>();
    for (const raw of rawAtts) {
      const mid: number | undefined = raw.memo_id;
      if (mid != null) {
        if (!attsByMemoId.has(mid)) attsByMemoId.set(mid, []);
        attsByMemoId.get(mid)!.push(toProtoAttachment(raw));
      }
    }

    // 建立 memo_id → uid 映射，用于 toProtoRelation 生成正确的 proto name
    const idToUid = await buildIdToUidMap(res.memos, rawRels);

    // 按原始 memo_id 分组 relations（每条 relation 分配给 memo_id 和 related_memo_id 两侧）
    const relsByMemoId = new Map<number, any[]>();
    for (const raw of rawRels) {
      const mid = raw.memo_id;
      const rid = raw.related_memo_id;
      if (!relsByMemoId.has(mid)) relsByMemoId.set(mid, []);
      relsByMemoId.get(mid)!.push(toProtoRelation(raw, idToUid));
      if (rid !== mid && memoIds.includes(rid)) {
        if (!relsByMemoId.has(rid)) relsByMemoId.set(rid, []);
        relsByMemoId.get(rid)!.push(toProtoRelation(raw, idToUid));
      }
    }

    const memos = res.memos.map((m: any) =>
      toProtoMemo(m, attsByMemoId.get(m.id) ?? [], relsByMemoId.get(m.id) ?? []),
    );
    const nextOffset = offset + memos.length;
    // 语义搜索：候选集一次性返回，无下一页
    const isSemanticSearch = rustReq.vector_embedding !== undefined;
    return {
      memos,
      nextPageToken: isSemanticSearch ? "" : (nextOffset < res.total ? String(nextOffset) : ""),
    };
  },

  async getMemo(req: any): Promise<any> {
    const uid = extractUid(req.name);
    const memo = await invoke<any | null>("get_memo", { uid, id: null });
    if (!memo) throw new Error(`Memo not found: ${req.name}`);

    // 双向查询 relations：memo_id_list 会同时匹配 memo_id 和 related_memo_id
    const [attRes, relRes] = await Promise.all([
      invoke<any[]>("list_attachments", { req: { memo_id: memo.id } }),
      invoke<any[]>("list_memo_relations", { req: { memo_id_list: [memo.id] } }),
    ]);
    const attachments = attRes.map((a: any) => toProtoAttachment(a));

    // 建立 id→uid 映射，用于 toProtoRelation 生成正确的 proto name
    const idToUid = await buildIdToUidMap([memo], relRes);
    const relations = relRes.map((r: any) => toProtoRelation(r, idToUid));
    return toProtoMemo(memo, attachments, relations);
  },

  async createMemo(req: any): Promise<any> {
    const m = req.memo || {};
    const uid = m.uid || crypto.randomUUID().replace(/-/g, "").slice(0, 16);
    const rustReq: any = {
      uid,
      content: m.content || "",
      visibility: protoVisToRust(m.visibility),
      pinned: m.pinned ?? false,
      payload: m.payload || {},
    };
    if (m.location) {
      rustReq.location = {
        placeholder: m.location.placeholder ?? "",
        latitude: m.location.latitude ?? 0,
        longitude: m.location.longitude ?? 0,
      };
    }
    const created = await invoke<any>("create_memo", { req: rustReq });

    // 关联附件：把新创建 memo 的 id 设置到每个 attachment
    const attachments = m.attachments || [];
    if (attachments.length > 0) {
      for (const att of attachments) {
        if (att.name) {
          const attUid = extractUid(att.name);
          const existingAtt = await invoke<any | null>("get_attachment", { id: null, uid: attUid, get_blob: false });
          if (existingAtt) {
            await invoke("update_attachment", { req: { id: existingAtt.id, memo_id: created.id } });
          }
        }
      }
    }

    // 创建引用关系
    const relations = m.relations || [];
    if (relations.length > 0) {
      for (const rel of relations) {
        const relatedUid = extractUid(rel.relatedMemo?.name || "");
        if (relatedUid) {
          const relatedMemo = await invoke<any | null>("get_memo", { id: null, uid: relatedUid });
          if (relatedMemo) {
            await invoke("upsert_memo_relation", {
              req: {
                memo_id: created.id,
                related_memo_id: relatedMemo.id,
                type: protoRelTypeToRust(rel.type ?? 1),
              },
            });
          }
        }
      }
    }

    // 查询关联后的 attachments 和 relations 返回
    const [attRes, relRes] = await Promise.all([
      invoke<any[]>("list_attachments", { req: { memo_id: created.id } }),
      invoke<any[]>("list_memo_relations", { req: { memo_id: created.id } }),
    ]);
    const idToUid = await buildIdToUidMap([created], relRes);
    return toProtoMemo(
      created,
      attRes.map((a: any) => toProtoAttachment(a)),
      relRes.map((r: any) => toProtoRelation(r, idToUid)),
    );
  },

  async updateMemo(req: any): Promise<any> {
    const uid = extractUid(req.memo.name);
    const existing = await invoke<any | null>("get_memo", { uid, id: null });
    if (!existing) throw new Error(`Memo not found: ${req.memo.name}`);

    const mask: string[] = req.updateMask?.paths || [];
    const rustReq: any = { id: existing.id };
    if (mask.includes("content")) rustReq.content = req.memo.content;
    if (mask.includes("pinned")) rustReq.pinned = req.memo.pinned;
    if (mask.includes("visibility")) rustReq.visibility = protoVisToRust(req.memo.visibility);
    if (mask.includes("state")) rustReq.row_status = stateToRowStatus(req.memo.state);

    // 处理 location：Some(Some(loc))=设置，Some(None)=清除，None=不更新
    if (mask.includes("location")) {
      if (req.memo.location) {
        rustReq.location = {
          placeholder: req.memo.location.placeholder ?? "",
          latitude: req.memo.location.latitude ?? 0,
          longitude: req.memo.location.longitude ?? 0,
        };
      } else {
        rustReq.location = null;
      }
    }

    const updated = await invoke<any>("update_memo", { req: rustReq });

    // 同步 attachments：把传入的附件关联到此 memo
    if (mask.includes("attachments")) {
      const attachments = req.memo.attachments || [];
      for (const att of attachments) {
        if (att.name) {
          const attUid = extractUid(att.name);
          const existingAtt = await invoke<any | null>("get_attachment", { id: null, uid: attUid, get_blob: false });
          if (existingAtt && existingAtt.memo_id !== existing.id) {
            await invoke("update_attachment", { req: { id: existingAtt.id, memo_id: existing.id } });
          }
        }
      }
    }

    // 同步 relations：upsert 所有传入的 relations
    if (mask.includes("relations")) {
      const relations = req.memo.relations || [];
      for (const rel of relations) {
        const relatedUid = extractUid(rel.relatedMemo?.name || "");
        if (relatedUid) {
          const relatedMemo = await invoke<any | null>("get_memo", { id: null, uid: relatedUid });
          if (relatedMemo) {
            await invoke("upsert_memo_relation", {
              req: {
                memo_id: existing.id,
                related_memo_id: relatedMemo.id,
                type: protoRelTypeToRust(rel.type ?? 1),
              },
            });
          }
        }
      }
    }

    // 查询关联后的 attachments 和 relations 返回
    const [attRes, relRes] = await Promise.all([
      invoke<any[]>("list_attachments", { req: { memo_id: existing.id } }),
      invoke<any[]>("list_memo_relations", { req: { memo_id: existing.id } }),
    ]);
    const idToUid = await buildIdToUidMap([updated], relRes);
    return toProtoMemo(
      updated,
      attRes.map((a: any) => toProtoAttachment(a)),
      relRes.map((r: any) => toProtoRelation(r, idToUid)),
    );
  },

  async deleteMemo(req: any): Promise<void> {
    const uid = extractUid(req.name);
    const existing = await invoke<any | null>("get_memo", { uid, id: null });
    if (!existing) throw new Error(`Memo not found: ${req.name}`);
    await invoke("delete_memo", { id: existing.id });
  },

  async listMemoComments(_req: any): Promise<any> {
    return { memos: [], nextPageToken: "" };
  },

  async createMemoComment(_req: any): Promise<any> {
    throw new Error("Comments not supported in local mode");
  },

  async listMemoRelations(_req: any): Promise<any> {
    return { relations: [], nextPageToken: "" };
  },

  async setMemoRelations(_req: any): Promise<any> {
    return {};
  },

  async listMemoReactions(_req: any): Promise<any> {
    return { reactions: [], nextPageToken: "" };
  },

  async upsertMemoReaction(_req: any): Promise<any> {
    throw new Error("Reactions not supported in local mode");
  },

  async deleteMemoReaction(_req: any): Promise<void> {},

  async getLinkMetadata(_req: any): Promise<any> {
    return { url: "", title: "", description: "", image: "" };
  },

  async listMemoShares(_req: any): Promise<any> {
    return { memoShares: [] };
  },

  async createMemoShare(_req: any): Promise<any> {
    throw new Error("Shares not supported in local mode");
  },

  async deleteMemoShare(_req: any): Promise<void> {},

  async getMemoByShare(_req: any): Promise<any> {
    throw new Error("Shares not supported in local mode");
  },
};

// ============ User Service 适配 ============

const LOCAL_USER = {
  name: "users/local",
  username: "local",
  displayName: "Local",
  role: 2,
  state: 1,
  createTime: ts(Math.floor(Date.now() / 1000)),
  updateTime: ts(Math.floor(Date.now() / 1000)),
};

export const userServiceClient = {
  async getUser(_req: any): Promise<any> {
    return LOCAL_USER;
  },
  async listUsers(_req: any): Promise<any> {
    return { users: [LOCAL_USER] };
  },
  async batchGetUsers(_req: any): Promise<any> {
    return { users: [LOCAL_USER] };
  },
  async getUserStats(_req: any): Promise<any> {
    // 并行获取 tag 计数和 memo 时间戳
    const [tags, timestamps] = await Promise.all([
      invoke<Array<{ tag: string; count: number }>>("list_tags"),
      invoke<{ created_timestamps: number[]; updated_timestamps: number[] }>("list_memo_timestamps"),
    ]);
    const tagCount: Record<string, number> = {};
    for (const { tag, count } of tags) {
      tagCount[tag] = count;
    }
    // proto Timestamp 对象 { seconds: bigint, nanos: 0 }
    const toTs = (secs: number[]) => secs.map((s) => ({ seconds: BigInt(s), nanos: 0 }));
    return {
      tagCount,
      memoCreatedTimestamps: toTs(timestamps.created_timestamps),
      memoUpdatedTimestamps: toTs(timestamps.updated_timestamps),
    };
  },
  async listAllUserStats(_req: any): Promise<any> {
    const [tags, timestamps] = await Promise.all([
      invoke<Array<{ tag: string; count: number }>>("list_tags"),
      invoke<{ created_timestamps: number[]; updated_timestamps: number[] }>("list_memo_timestamps"),
    ]);
    const tagCount: Record<string, number> = {};
    for (const { tag, count } of tags) {
      tagCount[tag] = count;
    }
    const toTs = (secs: number[]) => secs.map((s) => ({ seconds: BigInt(s), nanos: 0 }));
    return {
      stats: [
        {
          tagCount,
          memoCreatedTimestamps: toTs(timestamps.created_timestamps),
          memoUpdatedTimestamps: toTs(timestamps.updated_timestamps),
        },
      ],
    };
  },
  async updateUser(req: any): Promise<any> {
    return { ...LOCAL_USER, ...req.user };
  },
  async deleteUser(_req: any): Promise<void> {},
  async listUserSettings(_req: any): Promise<any> {
    // 从 app_setting 读取所有 user_setting:* 开头的设置
    // 由于 Rust 端没有 list 命令，我们预定义已知 keys
    const knownKeys = ["GENERAL", "TAGS"];
    const settings: any[] = [];
    for (const key of knownKeys) {
      const name = `users/local/settings/${key}`;
      const json = await invoke<string | null>("get_app_setting", { key: `user_setting:${name}` });
      if (json) {
        try {
          settings.push(JSON.parse(json));
        } catch {
          // 忽略损坏的 JSON
        }
      }
    }
    return { settings };
  },
  async updateUserSetting(req: any): Promise<any> {
    // req.setting 是 proto UserSetting 对象，包含 name 和 value（oneof）
    const setting = req.setting;
    if (!setting?.name) return {};
    // 序列化为 JSON 存储（value 是 oneof 对象，包含 generalSetting 或 tagsSetting）
    const storable = {
      name: setting.name,
      value: setting.value,
    };
    await invoke("upsert_app_setting", {
      req: { key: `user_setting:${setting.name}`, value: JSON.stringify(storable) },
    });
    return storable;
  },
  async listUserNotifications(_req: any): Promise<any> {
    return { notifications: [] };
  },
};

// ============ Attachment Service 适配 ============

export const attachmentServiceClient = {
  async listAttachments(req: any): Promise<any> {
    const rustReq: any = {};
    if (req.pageSize) rustReq.limit = req.pageSize;
    if (req.pageToken) rustReq.offset = parseInt(req.pageToken, 10);
    // 解析简单 filter：memo_id == null
    if (req.filter && req.filter.includes("memo_id == null")) {
      rustReq.memo_id_is_null = true;
    }
    const list = await invoke<any[]>("list_attachments", { req: rustReq });
    return { attachments: list.map(toProtoAttachment), nextPageToken: "" };
  },

  async getAttachment(req: any): Promise<any> {
    const uid = extractUid(req.name);
    const a = await invoke<any | null>("get_attachment", { id: null, uid, get_blob: false });
    if (!a) throw new Error(`Attachment not found: ${req.name}`);
    return toProtoAttachment(a);
  },

  async createAttachment(req: any): Promise<any> {
    const a = req.attachment || {};
    // proto CreateAttachmentRequest.attachmentId 是客户端指定的 uid
    const uid = req.attachmentId || a.uid || crypto.randomUUID().replace(/-/g, "").slice(0, 16);
    // proto Attachment.content (Uint8Array) → Rust blob (Vec<u8>)
    // proto Attachment.memo ("memos/{uid}") → Rust memo_id (i32)
    // memo_id 需要通过 uid 查询 memo 的数字 id
    let memo_id: number | null = null;
    if (a.memo) {
      const memoUid = extractUid(a.memo);
      const existingMemo = await invoke<any | null>("get_memo", { id: null, uid: memoUid });
      if (existingMemo) memo_id = existingMemo.id;
    }
    const rustReq = {
      uid,
      filename: a.filename || "untitled",
      blob: a.content || new Uint8Array(),
      type: a.type || "application/octet-stream",
      memo_id,
    };
    const created = await invoke<any>("create_attachment", { req: rustReq });
    return toProtoAttachment(created);
  },

  async updateAttachment(req: any): Promise<any> {
    const uid = extractUid(req.attachment.name);
    const existing = await invoke<any | null>("get_attachment", { id: null, uid, get_blob: false });
    if (!existing) throw new Error(`Attachment not found: ${req.attachment.name}`);
    const rustReq: any = { id: existing.id };
    if (req.updateMask?.paths?.includes("filename")) rustReq.filename = req.attachment.filename;
    const updated = await invoke<any>("update_attachment", { req: rustReq });
    return toProtoAttachment(updated);
  },

  async deleteAttachment(req: any): Promise<void> {
    const uid = extractUid(req.name);
    const existing = await invoke<any | null>("get_attachment", { id: null, uid, get_blob: false });
    if (!existing) throw new Error(`Attachment not found: ${req.name}`);
    await invoke("delete_attachment", { id: existing.id });
  },

  async batchDeleteAttachments(req: any): Promise<void> {
    for (const name of req.names || []) {
      const uid = extractUid(name);
      const existing = await invoke<any | null>("get_attachment", { id: null, uid, get_blob: false });
      if (existing) await invoke("delete_attachment", { id: existing.id });
    }
  },
};

// ============ 空 client（已删除功能）============

function createEmptyClient(): any {
  return new Proxy({}, {
    get: (_t, method: string) => {
      if (typeof method !== "string") return undefined;
      return async () => {
        if (method.startsWith("list") || method.startsWith("batch")) return {};
        return {};
      };
    },
  });
}

// ============ Instance Service 适配 ============
// 本地应用：instance settings 存储在 app_setting 中，key 格式 instance_setting:{name}

const LOCAL_PROFILE = {
  name: "instances/local",
  version: "0.1.0-local",
  instanceUrl: "",
  demo: false,
  needsSetup: false,
  mode: "NORMAL",
  disallowUserRegistration: false,
  disallowPasswordAuth: false,
  additionalScript: "",
  additionalStyle: "",
  customProfile: { title: "Memos", description: "", logoUrl: "", locale: "en", appearance: "SYSTEM" },
};

export const instanceServiceClient = {
  async getInstanceProfile(_req: any): Promise<any> {
    return LOCAL_PROFILE;
  },

  async getInstanceStats(_req: any): Promise<any> {
    const stats = await invoke<{
      generated_time: { seconds: number; nanos: number };
      database: { driver: string; size_bytes: number };
      local_storage_bytes: number;
    }>("get_instance_stats");
    return {
      generatedTime: { seconds: BigInt(stats.generated_time.seconds), nanos: stats.generated_time.nanos },
      database: {
        driver: stats.database.driver,
        sizeBytes: BigInt(stats.database.size_bytes),
      },
      localStorageBytes: BigInt(stats.local_storage_bytes),
    };
  },

  async getInstanceSetting(req: any): Promise<any> {
    // req.name 格式 "instance/settings/{KEY}"
    const json = await invoke<string | null>("get_app_setting", { key: `instance_setting:${req.name}` });
    if (!json) {
      // 返回一个空 setting，避免前端 undefined 报错
      return { name: req.name, value: { case: undefined, value: undefined } };
    }
    try {
      return JSON.parse(json);
    } catch {
      return { name: req.name, value: { case: undefined, value: undefined } };
    }
  },

  async batchGetInstanceSettings(req: any): Promise<any> {
    const names: string[] = req.names || [];
    const settings: any[] = [];
    for (const name of names) {
      const s = await this.getInstanceSetting({ name });
      settings.push(s);
    }
    return { settings };
  },

  async updateInstanceSetting(req: any): Promise<any> {
    const setting = req.setting;
    if (!setting?.name) return {};
    const storable = {
      name: setting.name,
      value: setting.value,
    };
    await invoke("upsert_app_setting", {
      req: { key: `instance_setting:${setting.name}`, value: JSON.stringify(storable) },
    });
    return storable;
  },
};

export const aiServiceClient = createEmptyClient();
export const shortcutServiceClient = createEmptyClient();

// 本地应用无 token 刷新
export async function refreshAccessToken(): Promise<void> {}
export async function getRequestToken(): Promise<string | null> { return null; }
