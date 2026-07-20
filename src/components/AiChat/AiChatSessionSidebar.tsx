import {
  CheckIcon,
  MessageSquareIcon,
  PlusIcon,
  TrashIcon,
  XIcon,
} from "lucide-react";
import { useEffect, useState } from "react";
import {
  deleteSession,
  listSessions,
  renameSession,
} from "./chatSessionService";
import { useTranslate } from "@/utils/i18n";
import { cn } from "@/lib/utils";
import toast from "react-hot-toast";
import type { ChatSession } from "./types";

interface AiChatSessionSidebarProps {
  open: boolean;
  currentSessionId: number | null;
  onClose: () => void;
  onSelect: (sessionId: number) => void;
  onNew: () => void;
  onDeleted: (sessionId: number) => void;
  onRenamed: (session: ChatSession) => void;
}

export function AiChatSessionSidebar({
  open,
  currentSessionId,
  onClose,
  onSelect,
  onNew,
  onDeleted,
  onRenamed,
}: AiChatSessionSidebarProps) {
  const t = useTranslate();
  const [sessions, setSessions] = useState<ChatSession[]>([]);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editingTitle, setEditingTitle] = useState("");

  // 加载会话列表
  const reload = async () => {
    try {
      const list = await listSessions();
      setSessions(list);
    } catch (e) {
      toast.error(String(e));
    }
  };

  useEffect(() => {
    if (open) {
      void reload();
    }
  }, [open]);

  const startEdit = (s: ChatSession) => {
    setEditingId(s.id);
    setEditingTitle(s.title);
  };

  const commitEdit = async () => {
    if (editingId === null) return;
    const trimmed = editingTitle.trim();
    if (!trimmed) {
      setEditingId(null);
      return;
    }
    try {
      const updated = await renameSession(editingId, trimmed);
      setSessions((prev) =>
        prev.map((s) => (s.id === updated.id ? { ...s, title: updated.title } : s)),
      );
      onRenamed(updated);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setEditingId(null);
      setEditingTitle("");
    }
  };

  const handleDelete = async (id: number) => {
    if (!window.confirm(t("aiChat.confirmDeleteSession"))) return;
    try {
      await deleteSession(id);
      setSessions((prev) => prev.filter((s) => s.id !== id));
      onDeleted(id);
    } catch (e) {
      toast.error(String(e));
    }
  };

  if (!open) return null;

  return (
    <>
      {/* 半透明遮罩 */}
      <div
        className="absolute inset-0 z-10 bg-black/20"
        onClick={onClose}
      />
      <div
        className={cn(
          "absolute left-0 top-0 bottom-0 z-20 w-[280px]",
          "bg-popover border-r border-border flex flex-col",
        )}
      >
        {/* 侧栏头部 */}
        <div className="flex items-center justify-between px-3 py-2 border-b border-border">
          <span className="text-sm font-medium">{t("aiChat.sessions")}</span>
          <div className="flex items-center gap-1">
            <button
              onClick={() => {
                onNew();
                onClose();
              }}
              className="size-7 rounded-md hover:bg-muted flex items-center justify-center"
              aria-label={t("aiChat.newChat")}
              title={t("aiChat.newChat")}
            >
              <PlusIcon className="size-3.5" />
            </button>
            <button
              onClick={onClose}
              className="size-7 rounded-md hover:bg-muted flex items-center justify-center"
              aria-label={t("aiChat.close")}
            >
              <XIcon className="size-3.5" />
            </button>
          </div>
        </div>

        {/* 会话列表 */}
        <div className="flex-1 overflow-y-auto py-1">
          {sessions.length === 0 ? (
            <div className="px-3 py-4 text-xs text-muted-foreground text-center">
              {t("aiChat.noSessions")}
            </div>
          ) : (
            sessions.map((s) => {
              const isActive = s.id === currentSessionId;
              const isEditing = editingId === s.id;
              return (
                <div
                  key={s.id}
                  className={cn(
                    "group flex items-center gap-2 px-3 py-2 cursor-pointer hover:bg-muted/50",
                    isActive && "bg-muted",
                  )}
                  onClick={() => {
                    if (!isEditing) {
                      onSelect(s.id);
                      onClose();
                    }
                  }}
                >
                  <MessageSquareIcon className="size-3.5 shrink-0 text-muted-foreground" />
                  <div className="flex-1 min-w-0">
                    {isEditing ? (
                      <input
                        autoFocus
                        value={editingTitle}
                        onChange={(e) => setEditingTitle(e.target.value)}
                        onBlur={commitEdit}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") {
                            e.preventDefault();
                            void commitEdit();
                          } else if (e.key === "Escape") {
                            setEditingId(null);
                            setEditingTitle("");
                          }
                        }}
                        onClick={(e) => e.stopPropagation()}
                        className="w-full text-sm bg-background border border-primary rounded px-1.5 py-0.5 focus:outline-none"
                      />
                    ) : (
                      <>
                        <div className="text-sm truncate font-medium">
                          {s.title}
                        </div>
                        {s.preview && (
                          <div className="text-xs text-muted-foreground truncate">
                            {s.preview}
                          </div>
                        )}
                        {s.message_count !== undefined && s.message_count > 0 && (
                          <div className="text-[10px] text-muted-foreground/70 mt-0.5">
                            {t("aiChat.messageCount", { count: s.message_count })}
                          </div>
                        )}
                      </>
                    )}
                  </div>
                  {isEditing ? (
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        void commitEdit();
                      }}
                      className="size-6 rounded hover:bg-background flex items-center justify-center shrink-0"
                      aria-label={t("aiChat.save")}
                    >
                      <CheckIcon className="size-3.5" />
                    </button>
                  ) : (
                    <div className="flex items-center opacity-0 group-hover:opacity-100 shrink-0">
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          startEdit(s);
                        }}
                        className="size-6 rounded hover:bg-background flex items-center justify-center text-xs"
                        aria-label={t("aiChat.rename")}
                      >
                        {t("aiChat.edit")}
                      </button>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          void handleDelete(s.id);
                        }}
                        className="size-6 rounded hover:bg-background flex items-center justify-center text-destructive"
                        aria-label={t("aiChat.delete")}
                      >
                        <TrashIcon className="size-3.5" />
                      </button>
                    </div>
                  )}
                </div>
              );
            })
          )}
        </div>
      </div>
    </>
  );
}
