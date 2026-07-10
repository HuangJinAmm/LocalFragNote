//! Markdown 处理：tag/mention 提取 + HTML 渲染
//!
//! - comrak 负责 CommonMark/GFM 渲染（含表格、任务列表、删除线、自动链接）
//! - tag/mention 提取用手写状态机，与原 Go goldmark parser 行为对齐

use comrak::{markdown_to_html, Options};
use unicode_categories::UnicodeCategories;

/// 最大 tag 长度（Unicode 字符数）
pub const MAX_TAG_LENGTH: usize = 100;
/// 最大 mention 长度
pub const MAX_MENTION_LENGTH: usize = 32;

/// 提取结果
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtractedData {
    pub tags: Vec<String>,
    pub mentions: Vec<String>,
}

/// 从 markdown 内容提取 tag 与 mention
pub fn extract_all(content: &str) -> ExtractedData {
    ExtractedData {
        tags: extract_tags(content),
        mentions: extract_mentions(content),
    }
}

/// 提取所有 #tag
///
/// 规则（与 Go goldmark parser 对齐）：
/// - `#` 后跟 Unicode 字母/数字/符号/mark + `_`/`-`/`/`/`&`
/// - 排除 `##`（标题）与 `# `（标题）
pub fn extract_tags(content: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] != '#' {
            i += 1;
            continue;
        }
        // 排除 ## (标题) 与 # 后空格
        if i + 1 < chars.len() && (chars[i + 1] == '#' || chars[i + 1] == ' ') {
            i += 1;
            continue;
        }
        if i + 1 >= chars.len() {
            break;
        }
        // 收集 tag
        let mut tag = String::new();
        let mut j = i + 1;
        while j < chars.len() && is_valid_tag_rune(chars[j]) && tag.chars().count() < MAX_TAG_LENGTH {
            tag.push(chars[j]);
            j += 1;
        }
        if !tag.is_empty() && !seen.contains(&tag) {
            seen.insert(tag.clone());
            tags.push(tag);
        }
        i = j;
    }
    tags
}

/// 提取所有 @mention（排除邮箱）
///
/// 规则：
/// - `@` 后跟 Unicode 字母/数字/`-`
/// - `@` 前一个字符必须是边界（行首/空白/标点/符号），不能是字母数字（排除邮箱）
pub fn extract_mentions(content: &str) -> Vec<String> {
    let mut mentions: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] != '@' {
            i += 1;
            continue;
        }
        // 检查前一个字符是否是边界
        if i > 0 && !is_mention_boundary(chars[i - 1]) {
            i += 1;
            continue;
        }
        // 收集 mention
        let mut username = String::new();
        let mut j = i + 1;
        while j < chars.len() && is_valid_mention_rune(chars[j]) && username.chars().count() < MAX_MENTION_LENGTH {
            username.push(chars[j]);
            j += 1;
        }
        // 必须至少有一个字母或数字
        if !username.is_empty() && username.chars().any(|c| c.is_alphanumeric()) && !seen.contains(&username) {
            seen.insert(username.clone());
            mentions.push(username);
        }
        i = j;
    }
    mentions
}

/// 渲染 markdown 为 HTML（GFM 兼容）
pub fn render_html(content: &str) -> String {
    markdown_to_html(content, &render_options())
}

/// 生成纯文本摘要（前 N 字符）
pub fn generate_snippet(content: &str, max_len: usize) -> String {
    let plain = strip_markdown(content);
    let count = plain.chars().count();
    if count <= max_len {
        plain
    } else {
        let truncated: String = plain.chars().take(max_len).collect();
        format!("{truncated}…")
    }
}

fn is_valid_tag_rune(r: char) -> bool {
    // 与 Go goldmark tag.go 对齐：
    // Unicode 字母/数字 + Symbol（Sm/Sc/Sk/So）+ Mark（Mn/Mc/Me）+ ZWJ + _-/&
    if r.is_alphanumeric() {
        return true;
    }
    if r.is_symbol() || r.is_mark() {
        return true;
    }
    if r == '\u{200D}' {
        // Zero Width Joiner，用于 emoji 序列
        return true;
    }
    matches!(r, '_' | '-' | '/' | '&')
}

fn is_valid_mention_rune(r: char) -> bool {
    r.is_alphanumeric() || r == '-'
}

fn is_mention_boundary(r: char) -> bool {
    r.is_whitespace() || r.is_ascii_punctuation() || is_symbol_punct(r)
}

fn is_symbol_punct(r: char) -> bool {
    // Unicode 标点/符号
    !r.is_alphanumeric() && !r.is_whitespace()
}

fn strip_markdown(content: &str) -> String {
    let mut result = String::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        let cleaned = trimmed
            .trim_start_matches(|c: char| c == '#' || c == '-' || c == '*' || c == '+' || c == '>');
        let cleaned = cleaned.trim_start();
        result.push_str(cleaned);
        result.push(' ');
    }
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn render_options() -> Options {
    let mut opts = Options::default();
    opts.extension.strikethrough = true;
    opts.extension.table = true;
    opts.extension.autolink = true;
    opts.extension.tasklist = true;
    opts.render.hardbreaks = false;
    opts.render.github_pre_lang = true;
    opts.render.escape = false;
    opts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_tags_basic() {
        let data = extract_all("Hello #tag and #another");
        assert_eq!(data.tags, vec!["tag", "another"]);
    }

    #[test]
    fn extract_tags_ignore_heading() {
        let data = extract_all("# Heading\n\nText with #tag");
        assert_eq!(data.tags, vec!["tag"]);
    }

    #[test]
    fn extract_tags_unicode() {
        let data = extract_all("标签 #中文标签 #emoji_🎉");
        assert!(data.tags.contains(&"中文标签".to_string()));
    }

    #[test]
    fn extract_tags_dedup() {
        let data = extract_all("#tag #tag #tag");
        assert_eq!(data.tags, vec!["tag"]);
    }

    #[test]
    fn extract_tags_slash() {
        let data = extract_all("#tag1/subtag");
        assert_eq!(data.tags, vec!["tag1/subtag"]);
    }

    #[test]
    fn extract_mentions_basic() {
        let data = extract_all("Hi @alice and @bob");
        assert_eq!(data.mentions, vec!["alice", "bob"]);
    }

    #[test]
    fn extract_mentions_exclude_email() {
        let data = extract_all("Email support@example.com @real");
        assert_eq!(data.mentions, vec!["real"]);
    }

    #[test]
    fn extract_mentions_at_line_start() {
        let data = extract_all("@alice\n@bob");
        assert_eq!(data.mentions, vec!["alice", "bob"]);
    }

    #[test]
    fn render_html_basic() {
        let html = render_html("**bold** and _italic_");
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
    }

    #[test]
    fn render_html_table() {
        let md = "| a | b |\n|---|---|\n| 1 | 2 |";
        let html = render_html(md);
        assert!(html.contains("<table>"));
    }

    #[test]
    fn snippet_truncates() {
        let s = generate_snippet("This is a long sentence that should be truncated", 10);
        assert!(s.ends_with('…'));
    }

    #[test]
    fn snippet_short_unchanged() {
        let s = generate_snippet("short", 100);
        assert_eq!(s, "short");
    }
}
