export const FOCUS_MODE_STYLES = {
  backdrop: "fixed inset-0 bg-black/20 backdrop-blur-sm z-40",
  container: {
    // Centered both axes: card appears in the middle of the app at any size.
    // Responsive sizing:
    //   - Width: viewport minus responsive side margin (1rem → 8rem), capped
    //     by max-w-5xl (xl: max-w-6xl) so it doesn't get too wide on large
    //     monitors.
    //   - Height: viewport-percentage that shrinks as the screen gets larger
    //     (85vh on phones → 70vh on lg+), avoiding an oversized card on big
    //     displays while maximizing space on small ones.
    //   - Definite h-* is required so the inner flex-1 editor can grow.
    // Border: explicit `border` width/style so `border-border` color renders.
    base: "fixed z-50 left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-[calc(100vw-1rem)] sm:w-[calc(100vw-2rem)] md:w-[calc(100vw-4rem)] lg:w-[calc(100vw-8rem)] max-w-5xl xl:max-w-6xl h-[85vh] sm:h-[80vh] md:h-[75vh] lg:h-[70vh] shadow-2xl border border-border",
    spacing: "",
  },
  transition: "transition-all duration-300 ease-in-out",
  exitButton: "absolute top-2 right-2 z-10 opacity-60 hover:opacity-100",
} as const;

export const EDITOR_HEIGHT = {
  // Max height for normal mode - focus mode uses flex-1 to grow dynamically
  normal: "max-h-[50vh]",
} as const;

// localStorage key for the user's preference to show the formatting toolbar in
// normal (non-focus) mode. Defaults to off.
export const FORMATTING_TOOLBAR_STORAGE_KEY = "memos-editor-formatting-toolbar";

// localStorage key for the auto-tag toggle. Defaults to off.
export const AUTO_TAG_STORAGE_KEY = "memos-editor-auto-tag";

// localStorage key for the document-summary toggle. Defaults to off.
export const SUMMARY_STORAGE_KEY = "memos-editor-summary";
