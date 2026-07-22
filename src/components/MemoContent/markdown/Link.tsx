import { invoke } from "@tauri-apps/api/core";
import { markdownStyles } from "@/lib/markdownStyles";
import { cn } from "@/lib/utils";
import type { ReactMarkdownProps } from "./types";

interface LinkProps extends React.AnchorHTMLAttributes<HTMLAnchorElement>, ReactMarkdownProps {
  children: React.ReactNode;
}

/**
 * Link component for external links
 * 调用系统默认浏览器打开外部链接，避免在 Tauri WebView 中打开新窗口
 */
export const Link = ({ children, className, href, node: _node, onClick, ...props }: LinkProps) => {
  const handleClick = async (event: React.MouseEvent<HTMLAnchorElement>) => {
    // 先让外部传入的 onClick 执行
    onClick?.(event);
    if (event.defaultPrevented) return;
    if (typeof href !== "string" || !href) return;
    event.preventDefault();
    try {
      await invoke("open_external_url", { url: href });
    } catch (err) {
      console.error("打开外部链接失败:", err);
    }
  };

  return (
    <a href={href} onClick={handleClick} className={cn(markdownStyles.link, className)} {...props}>
      {children}
    </a>
  );
};
