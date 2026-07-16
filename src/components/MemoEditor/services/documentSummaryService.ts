import { invoke } from "@tauri-apps/api/core";

export interface DocumentSummaryResult {
  kind: "summary" | "structure" | "skipped";
  markdown: string;
}

/** 支持文档摘要的扩展名（小写，不含点号） */
const SUPPORTED_DOC_EXTS = new Set([
  "pdf", "docx", "doc", "pptx", "ppt", "xlsx", "xls",
  "html", "htm", "csv", "xml", "rss", "atom",
]);
const ZIP_EXTS = new Set(["zip"]);

/** 判断文件是否应触发摘要（开关开启时由调用方决定是否真的调用） */
export function isSummarizable(filename: string): boolean {
  const ext = extractExt(filename);
  return SUPPORTED_DOC_EXTS.has(ext) || ZIP_EXTS.has(ext);
}

/** 调用后端命令 */
export const documentSummaryService = {
  async summarize(file: File): Promise<DocumentSummaryResult> {
    const blob = new Uint8Array(await file.arrayBuffer());
    return invoke<DocumentSummaryResult>("summarize_document_content", {
      blob,
      filename: file.name,
    });
  },
};

function extractExt(filename: string): string {
  const idx = filename.lastIndexOf(".");
  if (idx <= 0 || idx === filename.length - 1) return "";
  return filename.slice(idx + 1).toLowerCase();
}
