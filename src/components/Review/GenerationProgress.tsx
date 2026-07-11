import type { FC } from "react";
import { useTranslate } from "@/utils/i18n";

interface Props {
  generating: boolean;
  progress: string;
  result: { count: number; errors: string[] } | null;
  error: string | null;
}

const GenerationProgress: FC<Props> = ({ generating, progress, result, error }) => {
  const t = useTranslate();

  if (!generating && !result && !error) return null;

  return (
    <div className="rounded-lg border border-border p-3 space-y-2">
      {generating && (
        <>
          <div className="text-sm font-medium">{t("review.generating")}</div>
          <div className="text-xs text-muted-foreground max-h-32 overflow-auto whitespace-pre-wrap">
            {progress}
          </div>
        </>
      )}
      {result && (
        <div className="text-sm text-green-600">
          {t("review.generated", { count: result.count })}
          {result.errors.length > 0 && (
            <span className="text-destructive ml-2">
              {result.errors.length} {t("review.errors")}
            </span>
          )}
        </div>
      )}
      {error && <div className="text-sm text-destructive">{error}</div>}
    </div>
  );
};

export default GenerationProgress;
