import { useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "react-hot-toast";
import { useAuth } from "@/contexts/AuthContext";
import { useInstance } from "@/contexts/InstanceContext";
import { useNewMemo } from "@/contexts/NewMemoContext";
import { useLocalStorage } from "@/hooks";
import useCurrentUser from "@/hooks/useCurrentUser";
import { memoKeys } from "@/hooks/useMemoQueries";
import { userKeys } from "@/hooks/useUserQueries";
import { handleError } from "@/lib/error";
import { cn } from "@/lib/utils";
import { InstanceSetting_Key } from "@/types/proto/api/v1/instance_service_pb";
import { useTranslate } from "@/utils/i18n";
import { convertVisibilityFromString } from "@/utils/memo";
import { AudioRecorderPanel, EditorContent, EditorMetadata, FocusModeOverlay, TagSuggestionDialog, TimestampPopover } from "./components";
import { AUTO_TAG_STORAGE_KEY, FOCUS_MODE_STYLES, FORMATTING_TOOLBAR_STORAGE_KEY, SUMMARY_STORAGE_KEY } from "./constants";
import { useAudioRecorder, useAutoSave, useFocusMode, useMemoInit } from "./hooks";
import { documentSummaryService, errorService, isSummarizable, memoService, transcriptionService, validationService } from "./services";
import { EditorProvider, useEditorContext, useEditorSelector } from "./state";
import { EditorToolbar, FormattingToolbar } from "./Toolbar";
import type { MemoEditorProps } from "./types";
import type { LocalFile } from "./types/attachment";
import type { EditorController } from "./types/editorController";

const MemoEditor = (props: MemoEditorProps) => (
  <EditorProvider>
    <MemoEditorImpl {...props} />
  </EditorProvider>
);

const MemoEditorImpl: React.FC<MemoEditorProps> = ({
  className,
  cacheKey,
  memo,
  parentMemoName,
  autoFocus,
  placeholder,
  defaultCreateTime,
  onConfirm,
  onCancel,
}) => {
  const t = useTranslate();
  const queryClient = useQueryClient();
  const currentUser = useCurrentUser();
  const editorRef = useRef<EditorController>(null);
  const { actions, dispatch, getState } = useEditorContext();
  // Subscribe only to the low-frequency slices this component renders from, so
  // typing (which changes content) does not re-render the editor shell and its
  // toolbar/metadata children.
  const isFocusMode = useEditorSelector((s) => s.ui.isFocusMode);
  const hasTimestamp = useEditorSelector((s) => Boolean(s.timestamps.createTime));
  const { userGeneralSetting } = useAuth();
  const { aiSetting, fetchSetting } = useInstance();
  const { markNewMemo } = useNewMemo();
  const [isAudioRecorderOpen, setIsAudioRecorderOpen] = useState(false);
  const [isTranscribingAudio, setIsTranscribingAudio] = useState(false);
  // Persisted preference: also show the formatting toolbar in normal mode. Focus
  // mode always shows it regardless; this only governs the non-focus layout.
  const [isFormattingToolbarVisible, setFormattingToolbarVisible] = useLocalStorage(FORMATTING_TOOLBAR_STORAGE_KEY, false);
  // Persisted preference: auto-extract tags on save via AI.
  const [autoTagEnabled, setAutoTagEnabled] = useLocalStorage(AUTO_TAG_STORAGE_KEY, false);
  // Persisted preference: summarize document attachments on add.
  const [summaryEnabled, setSummaryEnabled] = useLocalStorage(SUMMARY_STORAGE_KEY, false);
  // Tag suggestion dialog state — active only when autoTagEnabled is ON.
  const [tagDialog, setTagDialog] = useState<{ open: boolean; loading: boolean; suggested: string[]; existing: string[] }>({
    open: false,
    loading: false,
    suggested: [],
    existing: [],
  });

  const memoName = memo?.name;
  const canTranscribe = useMemo(() => {
    const providerId = aiSetting.transcription?.providerId ?? "";
    if (!providerId) return false;
    const provider = aiSetting.providers.find((p) => p.id === providerId);
    return Boolean(provider?.apiKeySet);
  }, [aiSetting.providers, aiSetting.transcription?.providerId]);

  // Get default visibility from user settings
  const defaultVisibility = userGeneralSetting?.memoVisibility ? convertVisibilityFromString(userGeneralSetting.memoVisibility) : undefined;

  const { isInitialized } = useMemoInit({
    editorRef,
    memo,
    cacheKey,
    username: currentUser?.name ?? "",
    autoFocus,
    defaultVisibility,
    defaultCreateTime,
  });
  const isDraftCacheEnabled = !memo;

  // Auto-save content to localStorage (subscribes to the store internally).
  const { discardDraft } = useAutoSave(currentUser?.name ?? "", cacheKey, isInitialized && isDraftCacheEnabled);

  // Focus mode management with body scroll lock
  useFocusMode(isFocusMode);

  // Live-sync the draft's createTime/updateTime to the calendar-derived prop.
  // Only applies in create mode; edit mode owns its own timestamps. Runs after
  // initial mount (the seed value is set in useMemoInit), and again whenever
  // the prop changes — e.g., when the user picks a different calendar date
  // while the editor is open.
  useEffect(() => {
    if (memo) return;
    if (!isInitialized) return;
    dispatch(
      actions.setTimestamps({
        createTime: defaultCreateTime,
        updateTime: defaultCreateTime,
      }),
    );
  }, [defaultCreateTime, memo, isInitialized, actions, dispatch]);

  useEffect(() => {
    if (!currentUser) {
      return;
    }

    void fetchSetting(InstanceSetting_Key.AI).catch(() => undefined);
  }, [currentUser, fetchSetting]);

  const insertTranscribedText = useCallback((text: string) => {
    const editor = editorRef.current;
    if (!editor) {
      return;
    }
    editor.insertMarkdown(text);
    editor.scrollToCursor();
  }, []);

  const handleFileAdded = useCallback(async (localFile: LocalFile) => {
    // 先入队，避免阻塞 UI 与附件流程
    dispatch(actions.addLocalFile(localFile));

    if (!summaryEnabled) return;
    if (!isSummarizable(localFile.file.name)) return;

    const toastId = toast.loading(t("editor.summary.generating", { name: localFile.file.name }));
    try {
      const result = await documentSummaryService.summarize(localFile.file);
      if (result.kind === "skipped") {
        toast.dismiss(toastId);
        return;
      }
      const editor = editorRef.current;
      if (editor) {
        editor.appendMarkdown(`\n\n${result.markdown}`);
      }
      toast.success(t("editor.summary.done"), { id: toastId });
    } catch (e) {
      toast.error(
        t("editor.summary.failed", { name: localFile.file.name, reason: String(e) }),
        { id: toastId },
      );
    }
  }, [actions, dispatch, summaryEnabled, t]);

  const handleToggleSummary = useCallback(() => {
    setSummaryEnabled((v) => !v);
  }, [setSummaryEnabled]);

  const handleTranscribeRecordedAudio = useCallback(
    async (localFile: LocalFile) => {
      if (!canTranscribe) {
        void handleFileAdded(localFile);
        setIsTranscribingAudio(false);
        setIsAudioRecorderOpen(false);
        return;
      }

      try {
        const text = (await transcriptionService.transcribeFile(localFile.file)).trim();
        if (!text) {
          void handleFileAdded(localFile);
          toast.error(t("editor.audio-recorder.transcribe-empty"));
          return;
        }

        insertTranscribedText(text);
        toast.success(t("editor.audio-recorder.transcribe-success"));
      } catch (error) {
        console.error(error);
        toast.error(errorService.getErrorMessage(error) || t("editor.audio-recorder.transcribe-error"));
        void handleFileAdded(localFile);
      } finally {
        setIsTranscribingAudio(false);
        setIsAudioRecorderOpen(false);
      }
    },
    [canTranscribe, handleFileAdded, insertTranscribedText, t],
  );

  const audioRecorder = useAudioRecorder({
    onRecordingComplete: (localFile, mode) => {
      if (mode === "transcribe") {
        void handleTranscribeRecordedAudio(localFile);
        return;
      }

      void handleFileAdded(localFile);
      setIsAudioRecorderOpen(false);
    },
    onRecordingEmpty: (mode) => {
      if (mode === "transcribe") {
        setIsTranscribingAudio(false);
        toast.error(t("editor.audio-recorder.transcribe-empty"));
      }
      setIsAudioRecorderOpen(false);
    },
  });

  // Mirror the recorder's busy state into the store so validationService.canSave
  // (consumed here and by EditorToolbar) can block saves mid-recording without
  // the reducer owning the recorder's full state.
  useEffect(() => {
    dispatch(actions.setRecorderBusy(audioRecorder.isBusy));
  }, [audioRecorder.isBusy, actions, dispatch]);

  useEffect(() => {
    if (!isAudioRecorderOpen) {
      return;
    }

    if (audioRecorder.status === "error" || audioRecorder.status === "unsupported") {
      toast.error(audioRecorder.error || t("editor.audio-recorder.error-description"));
      setIsAudioRecorderOpen(false);
    }
  }, [isAudioRecorderOpen, audioRecorder.error, audioRecorder.status, t]);

  const handleToggleFocusMode = () => {
    dispatch(actions.toggleFocusMode());
  };

  const handleToggleFormattingToolbar = useCallback(() => {
    setFormattingToolbarVisible((visible) => !visible);
  }, [setFormattingToolbarVisible]);

  const handleStartAudioRecording = async () => {
    setIsAudioRecorderOpen(true);
    await audioRecorder.startRecording();
  };

  const handleAudioRecorderClick = () => {
    if (audioRecorder.isBusy) {
      return;
    }

    void handleStartAudioRecording();
  };

  const handleCancelAudioRecording = () => {
    setIsTranscribingAudio(false);
    audioRecorder.resetRecording();
    setIsAudioRecorderOpen(false);
  };

  const handleTranscribeAudioRecording = () => {
    if (!canTranscribe || isTranscribingAudio) {
      return;
    }

    setIsTranscribingAudio(true);
    const didStop = audioRecorder.stopRecording("transcribe");
    if (!didStop) {
      setIsTranscribingAudio(false);
    }
  };

  // Extract inline #tags from content for display in the suggestion dialog.
  const extractExistingTags = (content: string): string[] => {
    const tags = new Set<string>();
    const regex = /(?:^|\s)#([\w\u4e00-\u9fa5-]+)/g;
    let m;
    while ((m = regex.exec(content)) !== null) {
      tags.add(m[1]);
    }
    return Array.from(tags);
  };

  async function handleSave() {
    // Read the latest state imperatively — this component no longer subscribes
    // to content, so the closure can't rely on a per-render `state` snapshot.
    const state = getState();
    // Validate before saving
    const { valid, reason } = validationService.canSave(state);
    if (!valid) {
      toast.error(reason || "Cannot save");
      return;
    }

    // If auto-tag is enabled, intercept save to suggest tags first.
    if (autoTagEnabled) {
      const existing = extractExistingTags(state.content);
      setTagDialog({ open: true, loading: true, suggested: [], existing });
      try {
        const suggested = await invoke<string[]>("suggest_tags", { content: state.content });
        setTagDialog({ open: true, loading: false, suggested, existing });
      } catch (e) {
        // If AI suggestion fails, fall back to normal save.
        setTagDialog({ open: false, loading: false, suggested: [], existing: [] });
        toast.error(String(e));
        void doSave(state.content);
      }
      return;
    }

    void doSave(state.content);
  }

  // Perform the actual save with (optionally modified) content.
  async function doSave(content: string) {
    const state = getState();
    // Shallow-copy state with overridden content so memoService.save sees the
    // tag-appended text without mutating the live editor store.
    const stateToSave = { ...state, content };

    dispatch(actions.setLoading("saving", true));

    try {
      const result = await memoService.save(stateToSave, { memoName, parentMemoName });

      if (!result.hasChanges) {
        toast.error(t("editor.no-changes-detected"));
        onCancel?.();
        return;
      }

      // Clear localStorage cache on successful save and prevent the unmount
      // flush from writing the just-saved content back as a stale draft.
      discardDraft();

      // Invalidate React Query cache to refresh memo lists across the app
      const invalidationPromises = [
        queryClient.invalidateQueries({ queryKey: memoKeys.lists() }),
        queryClient.invalidateQueries({ queryKey: userKeys.stats() }),
      ];

      // Ensure memo detail pages don't keep stale cached content after edits.
      if (memoName) {
        invalidationPromises.push(queryClient.invalidateQueries({ queryKey: memoKeys.detail(memoName) }));
      }

      // If this was a comment, also invalidate the comments query for the parent memo
      if (parentMemoName) {
        invalidationPromises.push(queryClient.invalidateQueries({ queryKey: memoKeys.comments(parentMemoName) }));
      }

      await Promise.all(invalidationPromises);

      // Reset editor state to initial values
      dispatch(actions.reset());
      if (!memoName && defaultVisibility) {
        dispatch(actions.setMetadata({ visibility: defaultVisibility }));
      }
      // Re-seed the calendar-derived timestamps so the popover stays visible
      // and subsequent memos in the same filter session keep the prefilled date.
      // Without this, the live-sync effect won't re-fire (its deps don't change
      // across reset), and memo #2 onward would silently fall back to "now".
      if (!memoName && defaultCreateTime) {
        dispatch(actions.setTimestamps({ createTime: defaultCreateTime, updateTime: defaultCreateTime }));
      }

      // Surface a freshly created top-level memo at the top of the list so it
      // stays visible even when pinned memos would otherwise push it down.
      if (!memoName && !parentMemoName) {
        markNewMemo(result.memoName);
      }

      // Notify parent component of successful save
      onConfirm?.(result.memoName);
    } catch (error) {
      handleError(error, toast.error, {
        context: "Failed to save memo",
        fallbackMessage: errorService.getErrorMessage(error),
      });
    } finally {
      dispatch(actions.setLoading("saving", false));
    }
  }

  // Dialog confirm: append selected tags to content, then save.
  const handleTagConfirm = (selectedTags: string[]) => {
    const state = getState();
    const tagString = selectedTags.map((tag) => `#${tag}`).join(" ");
    const newContent = tagString ? `${state.content}\n\n${tagString}` : state.content;
    setTagDialog({ open: false, loading: false, suggested: [], existing: [] });
    void doSave(newContent);
  };

  // Dialog skip: save without adding tags.
  const handleTagSkip = () => {
    setTagDialog({ open: false, loading: false, suggested: [], existing: [] });
    void doSave(getState().content);
  };

  const handleToggleAutoTag = useCallback(() => {
    setAutoTagEnabled((v) => !v);
  }, [setAutoTagEnabled]);

  return (
    <>
      <FocusModeOverlay isActive={isFocusMode} onToggle={handleToggleFocusMode} />

      {/*
        Layout structure:
        - Uses justify-between to push content to top and bottom
        - In focus mode: becomes fixed with specific spacing, editor grows to fill space
        - In normal mode: stays relative with max-height constraint
      */}
      <div
        className={cn(
          "group relative w-full flex flex-col justify-between items-start bg-card px-4 pt-3 pb-1 rounded-lg border border-border gap-2",
          FOCUS_MODE_STYLES.transition,
          isFocusMode && cn(FOCUS_MODE_STYLES.container.base, FOCUS_MODE_STYLES.container.spacing),
          className,
        )}
      >
        {/* Formatting toolbar. Always shown in focus mode (with an exit button);
            in normal mode it appears only when the user toggled it on via the
            insert menu. */}
        {(isFocusMode || isFormattingToolbarVisible) && (
          <FormattingToolbar controllerRef={editorRef} onExit={isFocusMode ? handleToggleFocusMode : undefined} />
        )}

        {(memoName || (!memo && hasTimestamp)) && (
          <div className="w-full -mb-1">
            <TimestampPopover />
          </div>
        )}

        {/* Editor content grows to fill available space in focus mode */}
        <EditorContent ref={editorRef} placeholder={placeholder} onSubmit={handleSave} onFileAdded={handleFileAdded} />

        {isAudioRecorderOpen && (audioRecorder.isBusy || isTranscribingAudio) && (
          <AudioRecorderPanel
            audioRecorder={{ status: audioRecorder.status, elapsedSeconds: audioRecorder.elapsedSeconds }}
            mediaStream={audioRecorder.recordingStream}
            onStop={audioRecorder.stopRecording}
            onCancel={handleCancelAudioRecording}
            onTranscribe={handleTranscribeAudioRecording}
            canTranscribe={canTranscribe}
            isTranscribing={isTranscribingAudio}
          />
        )}

        {/* Metadata and toolbar grouped together at bottom */}
        <div className="w-full flex flex-col gap-2">
          <EditorMetadata memoName={memoName} />
          <EditorToolbar
            onSave={handleSave}
            onCancel={onCancel}
            memoName={memoName}
            onAudioRecorderClick={handleAudioRecorderClick}
            isFormattingToolbarVisible={isFormattingToolbarVisible}
            onToggleFormattingToolbar={handleToggleFormattingToolbar}
            autoTagEnabled={autoTagEnabled}
            onToggleAutoTag={handleToggleAutoTag}
            summaryEnabled={summaryEnabled}
            onToggleSummary={handleToggleSummary}
            onFileAdded={handleFileAdded}
          />
        </div>
      </div>

      <TagSuggestionDialog
        open={tagDialog.open}
        onOpenChange={(open) => {
          if (!open) handleTagSkip();
        }}
        loading={tagDialog.loading}
        suggestedTags={tagDialog.suggested}
        existingTags={tagDialog.existing}
        onConfirm={handleTagConfirm}
        onSkip={handleTagSkip}
      />
    </>
  );
};

export default MemoEditor;
