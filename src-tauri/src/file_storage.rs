//! 本地文件存储：将附件 blob 写入 attachments_dir，并管理相对路径
//!
//! 路径规则由文件名模板决定，默认 `{uid}_{filename}`。
//! 模板支持变量：`{uid}`、`{filename}`、`{timestamp}`、`{uuid}`，可含子目录。
//! reference 字段存储相对 attachments_dir 的路径。

use crate::error::{IpcError, IpcResult};
use std::path::{Path, PathBuf};

/// 默认文件名模板
const DEFAULT_TEMPLATE: &str = "{uid}_{filename}";

/// 将 blob 写入本地文件，返回相对 attachments_dir 的路径
///
/// `template` 支持变量：`{uid}`、`{filename}`、`{timestamp}`、`{uuid}`。
/// 模板为空时使用默认 `{uid}_{filename}`。模板可含子目录（如 `assets/{uuid}_{filename}`）。
pub fn write_file(
    attachments_dir: &Path,
    uid: &str,
    filename: &str,
    blob: &[u8],
    template: &str,
) -> IpcResult<String> {
    let tmpl = if template.trim().is_empty() {
        DEFAULT_TEMPLATE
    } else {
        template
    };
    let relative = render_template(tmpl, uid, filename);

    // 拒绝路径穿越
    if relative.contains("..") {
        return Err(IpcError::BadRequest("文件名模板包含非法路径".into()));
    }

    let abs_path = attachments_dir.join(&relative);

    // 模板含子目录时先创建
    if let Some(parent) = abs_path.parent() {
        if parent != attachments_dir && !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    if abs_path.exists() {
        // UID 已被校验唯一，理论上不会冲突；若冲突则报错
        return Err(IpcError::BadRequest(format!("文件已存在: {relative}")));
    }

    std::fs::write(&abs_path, blob)?;
    tracing::debug!("写入附件文件: {}", abs_path.display());
    Ok(relative)
}

/// 根据模板渲染相对路径
///
/// 支持变量：`{uid}`、`{filename}`（已 sanitize）、`{timestamp}`（Unix 秒）、`{uuid}`。
fn render_template(template: &str, uid: &str, filename: &str) -> String {
    let safe_name = sanitize_filename(filename);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let uuid = simple_uuid();
    template
        .replace("{uid}", uid)
        .replace("{filename}", &safe_name)
        .replace("{timestamp}", &timestamp.to_string())
        .replace("{uuid}", &uuid)
}

/// 简单 UUID：时间戳 + 计数器，避免引入 uuid crate
fn simple_uuid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let nanos = now.subsec_nanos();
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{secs:08x}{nanos:05x}{seq:03x}")
}

/// 读取本地文件
pub fn read_file(attachments_dir: &Path, reference: &str) -> IpcResult<Vec<u8>> {
    let path = resolve_path(attachments_dir, reference)?;
    Ok(std::fs::read(&path)?)
}

/// 删除本地文件（若不存在则忽略）
pub fn delete_file(attachments_dir: &Path, reference: &str) -> IpcResult<()> {
    // 直接拼接路径（不 canonicalize，避免文件不存在时报错）
    let path = attachments_dir.join(reference);
    if path.exists() {
        std::fs::remove_file(&path)?;
        tracing::debug!("删除附件文件: {}", path.display());
    }
    Ok(())
}

/// 解析相对路径为绝对路径，并防止路径穿越
fn resolve_path(attachments_dir: &Path, reference: &str) -> IpcResult<PathBuf> {
    let path = attachments_dir.join(reference);
    // 规范化后必须仍位于 attachments_dir 内
    let canonical_dir = attachments_dir
        .canonicalize()
        .map_err(|e| IpcError::Internal(format!("无法规范化附件目录: {e}")))?;
    let canonical_path = path
        .canonicalize()
        .map_err(|e| IpcError::NotFound(format!("附件文件不存在: {reference} ({e})")))?;
    if !canonical_path.starts_with(&canonical_dir) {
        return Err(IpcError::BadRequest("非法的附件路径".into()));
    }
    Ok(canonical_path)
}

/// 清理文件名：移除路径分隔符与非法字符
fn sanitize_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        // 保留字母数字、CJK、`-_.()`，其他替换为 `_`
        if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '(' | ')') || (c as u32 >= 0x4E00 && c as u32 <= 0x9FFF) {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    // 避免空名或以 `.` 开头
    if out.is_empty() || out.starts_with('.') {
        out.insert(0, '_');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_handles_special_chars() {
        assert_eq!(sanitize_filename("hello.txt"), "hello.txt");
        assert_eq!(sanitize_filename("a/b\\c"), "a_b_c");
        assert_eq!(sanitize_filename(".hidden"), "_.hidden");
        assert_eq!(sanitize_filename(""), "_");
        assert_eq!(sanitize_filename("中文.png"), "中文.png");
    }

    #[test]
    fn write_and_read_roundtrip() {
        let dir = tempdir();
        let blob = b"hello world".to_vec();
        let reference = write_file(&dir, "uid1", "test.txt", &blob, "{uid}_{filename}").unwrap();
        assert_eq!(reference, "uid1_test.txt");

        let read = read_file(&dir, &reference).unwrap();
        assert_eq!(read, blob);
    }

    #[test]
    fn write_with_subdir_template() {
        let dir = tempdir();
        let blob = b"subdir data".to_vec();
        let reference = write_file(&dir, "uid3", "doc.pdf", &blob, "assets/{uuid}_{filename}").unwrap();
        assert!(reference.starts_with("assets/"), "应在子目录下: {reference}");
        assert!(reference.ends_with("_doc.pdf"));

        let read = read_file(&dir, &reference).unwrap();
        assert_eq!(read, blob);
    }

    #[test]
    fn write_rejects_traversal_template() {
        let dir = tempdir();
        let result = write_file(&dir, "uid4", "evil.txt", b"x", "../escape.txt");
        assert!(result.is_err(), "应拒绝路径穿越模板");
    }

    #[test]
    fn delete_file_is_idempotent() {
        let dir = tempdir();
        let reference = write_file(&dir, "uid2", "x.bin", b"data", "{uid}_{filename}").unwrap();
        delete_file(&dir, &reference).unwrap();
        // 再次删除不应报错
        delete_file(&dir, &reference).unwrap();
    }

    #[test]
    fn reject_path_traversal() {
        let dir = tempdir();
        let result = read_file(&dir, "../../../etc/passwd");
        assert!(result.is_err(), "应拒绝路径穿越");
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "memos_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
