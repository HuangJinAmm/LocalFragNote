import {
  BotIcon,
  HistoryIcon,
  PlusIcon,
  SettingsIcon,
  XIcon,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslate } from "@/utils/i18n";
import { cn } from "@/lib/utils";
import { registerAiChat } from "./aiChatController";
import { AiChatComposer } from "./AiChatComposer";
import { AiChatMessages } from "./AiChatMessages";
import { AiChatProviderPicker } from "./AiChatProviderPicker";
import { AiChatSessionSidebar } from "./AiChatSessionSidebar";
import { AiChatSettings } from "./AiChatSettings";
import { useAiChat } from "./hooks";
import type { ChatSession } from "./types";

const BUTTON_SIZE = 44; // size-11
const PANEL_WIDTH = 480;
const PANEL_HEIGHT = 560;
const MARGIN = 16;
const DRAG_THRESHOLD = 5; // px，小于此距离视为点击
const POSITION_STORAGE_KEY = "ai_chat.position";

interface Position {
  x: number;
  y: number;
}

function getDefaultPosition(): Position {
  return { x: MARGIN, y: window.innerHeight - BUTTON_SIZE - MARGIN };
}

function loadPosition(): Position {
  try {
    const saved = localStorage.getItem(POSITION_STORAGE_KEY);
    if (saved) {
      const pos = JSON.parse(saved) as Position;
      if (typeof pos.x === "number" && typeof pos.y === "number") {
        return pos;
      }
    }
  } catch {
    // ignore
  }
  return getDefaultPosition();
}

function clampPosition(pos: Position, width: number, height: number): Position {
  return {
    x: Math.max(MARGIN, Math.min(pos.x, window.innerWidth - width - MARGIN)),
    y: Math.max(MARGIN, Math.min(pos.y, window.innerHeight - height - MARGIN)),
  };
}

export function AiChatPanel() {
  const t = useTranslate();
  const [open, setOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [sessionsOpen, setSessionsOpen] = useState(false);
  const [providerId, setProviderId] = useState<string | null>(null);
  // provider 列表刷新信号:设置保存后递增,触发 picker 重新拉取列表。
  // 解决"添加 provider 后下拉不显示,需关闭面板重开"的问题。
  const [providerRefreshKey, setProviderRefreshKey] = useState(0);
  const [position, setPosition] = useState<Position>(() => loadPosition());
  const {
    messages,
    isStreaming,
    currentSessionId,
    send,
    abort,
    clear,
    switchToSession,
    newChat,
  } = useAiChat({ providerId });

  const handleNewChat = useCallback(async () => {
    // newChat 仅创建/复用 session;若有当前会话且非空,clear 后再创建
    if (messages.length > 0) {
      await clear();
    }
    await newChat();
  }, [messages.length, clear, newChat]);

  const handleSelectSession = useCallback(
    (sessionId: number) => {
      void switchToSession(sessionId);
    },
    [switchToSession],
  );

  const handleDeleted = useCallback(
    (deletedId: number) => {
      if (deletedId === currentSessionId) {
        // 当前会话被删除：清空视图
        void clear();
      }
    },
    [currentSessionId, clear],
  );

  const handleRenamed = useCallback((_session: ChatSession) => {
    // 重命名不影响当前会话视图，无需额外处理
  }, []);

  // 保持 send 的最新引用，供模块级控制器调用
  const sendRef = useRef(send);
  sendRef.current = send;

  // 注册到模块级控制器，允许外部组件(如 MemoActionMenu)打开面板并发送预设消息
  useEffect(() => {
    return registerAiChat(
      () => setOpen(true),
      (content) => sendRef.current(content),
    );
  }, []);

  // 拖拽状态
  const dragState = useRef<{
    dragging: boolean;
    startX: number;
    startY: number;
    startPosX: number;
    startPosY: number;
    moved: boolean;
  }>({
    dragging: false,
    startX: 0,
    startY: 0,
    startPosX: 0,
    startPosY: 0,
    moved: false,
  });

  const beginDrag = useCallback((e: React.MouseEvent, currentPos: Position) => {
    dragState.current = {
      dragging: true,
      startX: e.clientX,
      startY: e.clientY,
      startPosX: currentPos.x,
      startPosY: currentPos.y,
      moved: false,
    };
  }, []);

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!dragState.current.dragging) return;
      const dx = e.clientX - dragState.current.startX;
      const dy = e.clientY - dragState.current.startY;
      if (Math.abs(dx) > DRAG_THRESHOLD || Math.abs(dy) > DRAG_THRESHOLD) {
        dragState.current.moved = true;
      }
      const newPos = {
        x: dragState.current.startPosX + dx,
        y: dragState.current.startPosY + dy,
      };
      const { width, height } = open
        ? { width: PANEL_WIDTH, height: PANEL_HEIGHT }
        : { width: BUTTON_SIZE, height: BUTTON_SIZE };
      setPosition(clampPosition(newPos, width, height));
    };

    const handleMouseUp = () => {
      if (dragState.current.dragging) {
        dragState.current.dragging = false;
        setPosition((pos) => {
          localStorage.setItem(POSITION_STORAGE_KEY, JSON.stringify(pos));
          return pos;
        });
      }
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [open]);

  // 窗口缩放时重新约束位置
  useEffect(() => {
    const handleResize = () => {
      setPosition((pos) => {
        const { width, height } = open
          ? { width: PANEL_WIDTH, height: PANEL_HEIGHT }
          : { width: BUTTON_SIZE, height: BUTTON_SIZE };
        return clampPosition(pos, width, height);
      });
    };
    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, [open]);

  // 折叠按钮点击：若发生过拖拽则不展开
  const handleButtonClick = () => {
    if (dragState.current.moved) {
      dragState.current.moved = false;
      return;
    }
    setOpen(true);
  };

  // 根据当前展开状态约束位置
  const effectivePos = open
    ? clampPosition(position, PANEL_WIDTH, PANEL_HEIGHT)
    : clampPosition(position, BUTTON_SIZE, BUTTON_SIZE);

  return (
    <>
      <div
        className="fixed z-50"
        style={{ left: effectivePos.x, top: effectivePos.y }}
      >
        {open ? (
          <div className="flex flex-col w-[480px] h-[560px] rounded-xl border border-border bg-popover shadow-lg overflow-hidden relative">
            {/* 会话侧栏（绝对定位覆盖在面板上） */}
            <AiChatSessionSidebar
              open={sessionsOpen}
              currentSessionId={currentSessionId}
              onClose={() => setSessionsOpen(false)}
              onSelect={handleSelectSession}
              onNew={() => {
                void handleNewChat();
              }}
              onDeleted={handleDeleted}
              onRenamed={handleRenamed}
            />
            {/* Header — 仅图标+标题区域可拖拽 */}
            <div className="flex items-center gap-2 border-b border-border px-3 py-2">
              <div
                className="flex items-center gap-2 flex-1 cursor-move select-none"
                onMouseDown={(e) => beginDrag(e, effectivePos)}
              >
                <BotIcon className="size-4 text-primary" />
                <span className="font-medium text-sm">{t("aiChat.title")}</span>
              </div>
              <button
                onClick={() => setSessionsOpen(true)}
                disabled={isStreaming}
                className="size-7 rounded-md hover:bg-muted flex items-center justify-center disabled:opacity-40 disabled:hover:bg-transparent"
                aria-label={t("aiChat.sessions")}
                title={t("aiChat.sessions")}
              >
                <HistoryIcon className="size-3.5" />
              </button>
              <button
                onClick={handleNewChat}
                disabled={isStreaming}
                className="size-7 rounded-md hover:bg-muted flex items-center justify-center disabled:opacity-40 disabled:hover:bg-transparent"
                aria-label={t("aiChat.newChat")}
                title={t("aiChat.newChat")}
              >
                <PlusIcon className="size-3.5" />
              </button>
              <button
                onClick={() => setSettingsOpen(true)}
                className="size-7 rounded-md hover:bg-muted flex items-center justify-center"
                aria-label={t("aiChat.settings")}
              >
                <SettingsIcon className="size-3.5" />
              </button>
              <button
                onClick={() => setOpen(false)}
                className="size-7 rounded-md hover:bg-muted flex items-center justify-center"
                aria-label={t("aiChat.close")}
              >
                <XIcon className="size-3.5" />
              </button>
            </div>

            {/* Messages */}
            <AiChatMessages messages={messages} />

            {/* Composer */}
            <AiChatComposer
              isStreaming={isStreaming}
              disabled={!providerId}
              onSend={send}
              onAbort={abort}
              providerSlot={
                <AiChatProviderPicker
                  onProviderChange={setProviderId}
                  refreshKey={providerRefreshKey}
                />
              }
            />
          </div>
        ) : (
          <button
            onMouseDown={(e) => beginDrag(e, effectivePos)}
            onClick={handleButtonClick}
            className={cn(
              "size-11 rounded-full bg-primary text-primary-foreground shadow-lg",
              "flex items-center justify-center cursor-grab active:cursor-grabbing",
              "hover:scale-110 active:scale-90 transition-transform",
            )}
            aria-label={t("aiChat.open")}
          >
            <BotIcon className="size-5" />
          </button>
        )}
      </div>

      <AiChatSettings
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        onSaved={() => {
          // 触发 picker 重新拉取 provider 列表,让新增/编辑/删除立即反映到下拉。
          setProviderRefreshKey((k) => k + 1);
        }}
      />
    </>
  );
}
