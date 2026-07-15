import {
  AlignLeftIcon,
  ArchiveIcon,
  ArchiveRestoreIcon,
  BookmarkMinusIcon,
  BookmarkPlusIcon,
  BotIcon,
  CheckCheckIcon,
  CopyIcon,
  Edit3Icon,
  FileTextIcon,
  GraduationCapIcon,
  LanguagesIcon,
  LinkIcon,
  ListChecksIcon,
  ListRestartIcon,
  MoreVerticalIcon,
  PenLineIcon,
  TrashIcon,
} from "lucide-react";
import { useState } from "react";
import { openAiChatWithPrompt } from "@/components/AiChat/aiChatController";
import ConfirmDialog from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { State } from "@/types/proto/api/v1/common_pb";
import { useTranslate } from "@/utils/i18n";
import { countTasks } from "@/utils/markdown-manipulation";
import { useMemoActionHandlers } from "./hooks";
import type { MemoActionMenuProps } from "./types";

const MAX_AI_PROMPT_CONTENT = 2000;

const MemoActionMenu = (props: MemoActionMenuProps) => {
  const { memo, readonly } = props;
  const t = useTranslate();

  // Dialog state
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);

  // Derived state
  const isComment = Boolean(memo.parent);
  const isArchived = memo.state === State.ARCHIVED;
  const taskStats = countTasks(memo.content);
  const canMutateTasks = !readonly && !isArchived && taskStats.total > 0;
  const hasOpenTasks = taskStats.completed < taskStats.total;
  const hasCompletedTasks = taskStats.completed > 0;
  const hasContent = memo.content.trim().length > 0;

  // AI 动作：构造预设 prompt 并打开 AI 聊天面板
  const buildPrompt = (instruction: string) => {
    const content = memo.content.length > MAX_AI_PROMPT_CONTENT
      ? memo.content.slice(0, MAX_AI_PROMPT_CONTENT) + "\n...(内容已截断)"
      : memo.content;
    return `${instruction}\n\n${content}`;
  };

  const handleAiSummarize = () => {
    openAiChatWithPrompt(buildPrompt(t("memo.ai-actions.summarize-prompt")));
  };
  const handleAiTranslate = () => {
    openAiChatWithPrompt(buildPrompt(t("memo.ai-actions.translate-prompt")));
  };
  const handleAiPolish = () => {
    openAiChatWithPrompt(buildPrompt(t("memo.ai-actions.polish-prompt")));
  };
  const handleAiGenerateCards = () => {
    // memo.name 格式为 "memos/{uid}"，提取 uid 供 AI 调用 create_review_cards 工具时使用
    const uid = memo.name.split("/").pop() ?? memo.name;
    const prompt = t("memo.ai-actions.cards-prompt", { uid });
    openAiChatWithPrompt(buildPrompt(prompt));
  };

  // Action handlers
  const {
    handleTogglePinMemoBtnClick,
    handleEditMemoClick,
    handleToggleMemoStatusClick,
    handleCopyLink,
    handleCopyContent,
    handleCheckAllTaskListItemsClick,
    handleUncheckAllTaskListItemsClick,
    handleDeleteMemoClick,
    confirmDeleteMemo,
  } = useMemoActionHandlers({
    memo,
    onEdit: props.onEdit,
    setDeleteDialogOpen,
  });

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon" className="size-4">
          <MoreVerticalIcon className="text-muted-foreground" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" sideOffset={2}>
        {/* Edit actions (non-readonly, non-archived) */}
        {!readonly && !isArchived && (
          <>
            {!isComment && (
              <DropdownMenuItem onClick={handleTogglePinMemoBtnClick}>
                {memo.pinned ? <BookmarkMinusIcon className="w-4 h-auto" /> : <BookmarkPlusIcon className="w-4 h-auto" />}
                {memo.pinned ? t("common.unpin") : t("common.pin")}
              </DropdownMenuItem>
            )}
            <DropdownMenuItem onClick={handleEditMemoClick}>
              <Edit3Icon className="w-4 h-auto" />
              {t("common.edit")}
            </DropdownMenuItem>
          </>
        )}

        {/* Copy submenu (non-archived) */}
        {!isArchived && (
          <DropdownMenuSub>
            <DropdownMenuSubTrigger>
              <CopyIcon className="w-4 h-auto" />
              {t("common.copy")}
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent>
              <DropdownMenuItem onClick={handleCopyLink}>
                <LinkIcon className="w-4 h-auto" />
                {t("memo.copy-link")}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={handleCopyContent}>
                <FileTextIcon className="w-4 h-auto" />
                {t("memo.copy-content")}
              </DropdownMenuItem>
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        )}

        {/* AI submenu (non-archived, has content) */}
        {!isArchived && hasContent && (
          <DropdownMenuSub>
            <DropdownMenuSubTrigger>
              <BotIcon className="w-4 h-auto" />
              {t("memo.ai-actions.title")}
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent>
              <DropdownMenuItem onClick={handleAiSummarize}>
                <AlignLeftIcon className="w-4 h-auto" />
                {t("memo.ai-actions.summarize")}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={handleAiTranslate}>
                <LanguagesIcon className="w-4 h-auto" />
                {t("memo.ai-actions.translate")}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={handleAiPolish}>
                <PenLineIcon className="w-4 h-auto" />
                {t("memo.ai-actions.polish")}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={handleAiGenerateCards}>
                <GraduationCapIcon className="w-4 h-auto" />
                {t("memo.ai-actions.generate-cards")}
              </DropdownMenuItem>
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        )}

        {/* Task submenu (writable task memos) */}
        {canMutateTasks && (
          <DropdownMenuSub>
            <DropdownMenuSubTrigger>
              <ListChecksIcon className="w-4 h-auto" />
              {t("memo.task-actions.title")}
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent>
              <DropdownMenuItem disabled={!hasOpenTasks} onClick={handleCheckAllTaskListItemsClick}>
                <CheckCheckIcon className="w-4 h-auto" />
                {t("memo.task-actions.check-all")}
              </DropdownMenuItem>
              <DropdownMenuItem disabled={!hasCompletedTasks} onClick={handleUncheckAllTaskListItemsClick}>
                <ListRestartIcon className="w-4 h-auto" />
                {t("memo.task-actions.uncheck-all")}
              </DropdownMenuItem>
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        )}

        {/* Write actions (non-readonly) */}
        {!readonly && (
          <>
            {/* Archive/Restore (non-comment) */}
            {!isComment && (
              <DropdownMenuItem onClick={handleToggleMemoStatusClick}>
                {isArchived ? <ArchiveRestoreIcon className="w-4 h-auto" /> : <ArchiveIcon className="w-4 h-auto" />}
                {isArchived ? t("common.restore") : t("common.archive")}
              </DropdownMenuItem>
            )}

            {/* Delete */}
            <DropdownMenuItem onClick={handleDeleteMemoClick}>
              <TrashIcon className="w-4 h-auto" />
              {t("common.delete")}
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>

      {/* Delete confirmation dialog */}
      <ConfirmDialog
        open={deleteDialogOpen}
        onOpenChange={setDeleteDialogOpen}
        title={t("memo.delete-confirm")}
        confirmLabel={t("common.delete")}
        description={t("memo.delete-confirm-description")}
        cancelLabel={t("common.cancel")}
        onConfirm={confirmDeleteMemo}
        confirmVariant="destructive"
      />
    </DropdownMenu>
  );
};

export default MemoActionMenu;
