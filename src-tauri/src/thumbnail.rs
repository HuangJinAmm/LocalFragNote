//! 缩略图生成：使用 image crate 缩放图片
//!
//! 策略：
//! - 仅对 image/* 类型生成缩略图
//! - 保持宽高比，最长边不超过 max_size（默认 512）
//! - 输出格式：JPEG（quality 85）以减小体积
//! - 非图片或过小图片返回 None

use image::{imageops::FilterType, ImageFormat, ImageReader};
use std::io::Cursor;

/// 默认缩略图最长边
pub const DEFAULT_THUMBNAIL_SIZE: u32 = 512;
/// 最小原图尺寸（小于此尺寸不生成缩略图）
pub const MIN_ORIGINAL_SIZE: u32 = 128;

/// 生成缩略图字节流
///
/// 参数：
/// - `blob`: 原始图片字节
/// - `mime_type`: 如 "image/png"
/// - `max_size`: 缩略图最长边像素
///
/// 返回：
/// - `Ok(Some(bytes))`: 生成的 JPEG 缩略图
/// - `Ok(None)`: 原图太小或非图片，无需缩略图
pub fn generate_thumbnail(
    blob: &[u8],
    mime_type: &str,
    max_size: u32,
) -> Result<Option<Vec<u8>>, String> {
    if !mime_type.starts_with("image/") {
        return Ok(None);
    }

    let reader = ImageReader::new(Cursor::new(blob))
        .with_guessed_format()
        .map_err(|e| format!("无法识别图片格式: {e}"))?;
    let format = reader.format().ok_or("无法确定图片格式")?;
    let img = reader.decode().map_err(|e| format!("解码图片失败: {e}"))?;

    let (w, h) = (img.width(), img.height());
    // 原图太小则不生成
    if w.min(h) < MIN_ORIGINAL_SIZE {
        return Ok(None);
    }

    let scaled = img.resize(max_size, max_size, FilterType::Lanczos3);
    let mut out = Cursor::new(Vec::with_capacity(8 * 1024));
    scaled
        .write_to(&mut out, ImageFormat::Jpeg)
        .map_err(|e| format!("编码缩略图失败: {e}"))?;
    let _ = format; // 仅用于潜在的未来格式判断
    Ok(Some(out.into_inner()))
}

/// 判断 MIME 类型是否为可生成缩略图的图片
pub fn is_thumbnailable_image(mime_type: &str) -> bool {
    matches!(
        mime_type,
        "image/png" | "image/jpeg" | "image/jpg" | "image/webp" | "image/gif"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbaImage::new(width, height);
        let mut buf = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut buf, ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    #[test]
    fn non_image_returns_none() {
        let result = generate_thumbnail(b"not an image", "application/pdf", 256).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn small_image_returns_none() {
        let png = make_png(64, 64);
        let result = generate_thumbnail(&png, "image/png", 256).unwrap();
        assert!(result.is_none(), "小于 MIN_ORIGINAL_SIZE 的图片不应生成缩略图");
    }

    #[test]
    fn large_image_generates_thumbnail() {
        let png = make_png(1024, 768);
        let result = generate_thumbnail(&png, "image/png", 256).unwrap();
        assert!(result.is_some(), "大图应生成缩略图");
        let bytes = result.unwrap();
        assert!(!bytes.is_empty());

        // 验证生成的缩略图可解码且尺寸正确
        let thumb = ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .unwrap()
            .decode()
            .unwrap();
        assert!(thumb.width() <= 256);
        assert!(thumb.height() <= 256);
    }

    #[test]
    fn is_thumbnailable_recognizes_common_types() {
        assert!(is_thumbnailable_image("image/png"));
        assert!(is_thumbnailable_image("image/jpeg"));
        assert!(is_thumbnailable_image("image/webp"));
        assert!(!is_thumbnailable_image("image/svg+xml"));
        assert!(!is_thumbnailable_image("application/pdf"));
    }
}
