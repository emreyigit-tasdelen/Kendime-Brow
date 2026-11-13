//! Image format conversion (task 3C.3): decodes PNG/JPEG/BMP/GIF/WebP/TIFF and re-encodes as
//! PNG, JPEG (quality 90), BMP, GIF or lossless WebP. Metadata is not carried over, so the output
//! also carries no EXIF.

use image::{DynamicImage, ImageFormat};
use std::io::Cursor;

pub fn convert(data: &[u8], target: &str) -> Result<Vec<u8>, String> {
    let (format, _) = target_format(target)?;
    let img = image::load_from_memory(data).map_err(|e| format!("cannot read image: {e}"))?;
    let mut out = Cursor::new(Vec::new());
    match format {
        // JPEG and BMP-without-alpha paths need RGB.
        ImageFormat::Jpeg => {
            let rgb = DynamicImage::ImageRgb8(img.to_rgb8());
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90);
            rgb.write_with_encoder(enc).map_err(|e| e.to_string())?;
        }
        f => img.write_to(&mut out, f).map_err(|e| e.to_string())?,
    }
    Ok(out.into_inner())
}

/// `(format, file extension)` for a target name.
pub fn target_format(target: &str) -> Result<(ImageFormat, &'static str), String> {
    match target {
        "png" => Ok((ImageFormat::Png, "png")),
        "jpg" | "jpeg" => Ok((ImageFormat::Jpeg, "jpg")),
        "bmp" => Ok((ImageFormat::Bmp, "bmp")),
        "gif" => Ok((ImageFormat::Gif, "gif")),
        "webp" => Ok((ImageFormat::WebP, "webp")),
        other => Err(format!("unsupported target format: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(4, 3, image::Rgba([200, 10, 10, 255]));
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(img)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn converts_between_formats() {
        let p = png();
        for (t, magic) in [
            ("jpg", &[0xFF, 0xD8][..]),
            ("bmp", b"BM"),
            ("gif", b"GIF"),
            ("webp", b"RIFF"),
            ("png", &[0x89, b'P']),
        ] {
            let out = convert(&p, t).unwrap_or_else(|e| panic!("{t}: {e}"));
            assert!(out.starts_with(magic), "{t}");
            let back = image::load_from_memory(&out).unwrap();
            assert_eq!((back.width(), back.height()), (4, 3), "{t}");
        }
    }

    #[test]
    fn rejects_bad_input() {
        assert!(convert(b"nope", "png").is_err());
        assert!(convert(&png(), "tga").is_err());
    }
}
