//! Image metadata (architecture §8.2, task 3C.4 / V11): read EXIF and remove it losslessly from
//! JPEG files (no re-encoding), so GPS and camera details can be dropped before sharing.

use std::io::Cursor;

use serde::Serialize;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Entry {
    pub tag: String,
    pub value: String,
    /// `gps`, `device`, `time` or `other`: what kind of personal information it is.
    pub group: &'static str,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Metadata {
    pub entries: Vec<Entry>,
    pub has_gps: bool,
}

fn group_of(tag: &str) -> &'static str {
    let t = tag.to_ascii_lowercase();
    if t.starts_with("gps") {
        "gps"
    } else if [
        "make",
        "model",
        "software",
        "lens",
        "serial",
        "owner",
        "artist",
        "copyright",
    ]
    .iter()
    .any(|k| t.contains(k))
    {
        "device"
    } else if t.contains("date") || t.contains("time") {
        "time"
    } else {
        "other"
    }
}

/// Reads EXIF from JPEG / TIFF / HEIF / PNG / WebP bytes. No EXIF → empty result.
pub fn read(data: &[u8]) -> Result<Metadata, String> {
    let exif = match exif::Reader::new().read_from_container(&mut Cursor::new(data)) {
        Ok(e) => e,
        Err(exif::Error::NotFound(_)) => {
            return Ok(Metadata {
                entries: Vec::new(),
                has_gps: false,
            });
        }
        Err(e) => return Err(e.to_string()),
    };
    let entries: Vec<Entry> = exif
        .fields()
        .map(|f| {
            let tag = f.tag.to_string();
            let group = group_of(&tag);
            Entry {
                tag,
                value: f.display_value().with_unit(&exif).to_string(),
                group,
            }
        })
        .collect();
    let has_gps = entries.iter().any(|e| e.group == "gps");
    Ok(Metadata { entries, has_gps })
}

/// Removes every EXIF (APP1 `Exif\0\0`) and XMP (APP1 `http://ns.adobe.com/xap/`) segment from a
/// JPEG without touching the image data. Returns the new bytes and the number of removed
/// segments.
pub fn strip_jpeg_metadata(data: &[u8]) -> Result<(Vec<u8>, usize), String> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return Err("not a JPEG file".into());
    }
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(&data[..2]);
    let mut i = 2;
    let mut removed = 0;
    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            return Err("corrupt JPEG segment structure".into());
        }
        let marker = data[i + 1];
        if marker == 0xFF {
            i += 1; // fill byte
            continue;
        }
        // Start of scan: the rest is entropy-coded data, copy verbatim.
        if marker == 0xDA {
            out.extend_from_slice(&data[i..]);
            return Ok((out, removed));
        }
        // Standalone markers have no length.
        if (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            out.extend_from_slice(&data[i..i + 2]);
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        let end = i + 2 + len;
        if len < 2 || end > data.len() {
            return Err("corrupt JPEG segment length".into());
        }
        let payload = &data[i + 4..end];
        let drop = marker == 0xE1
            && (payload.starts_with(b"Exif\0\0")
                || payload.starts_with(b"http://ns.adobe.com/xap/"));
        if drop {
            removed += 1;
        } else {
            out.extend_from_slice(&data[i..end]);
        }
        i = end;
    }
    Err("no start-of-scan marker found".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny JPEG skeleton: SOI, APP0, APP1(Exif, minimal TIFF with Make), DQT-ish filler, SOS.
    fn jpeg_with_exif() -> Vec<u8> {
        // Minimal big-endian TIFF: header, 1 IFD entry: Make (0x010F) ASCII "KB\0" inline.
        let tiff: Vec<u8> = vec![
            b'M', b'M', 0, 42, 0, 0, 0, 8, // header, IFD at 8
            0, 1, // one entry
            0x01, 0x0F, 0, 2, 0, 0, 0, 3, b'K', b'B', 0, 0, // Make, ASCII, count 3, "KB\0"
            0, 0, 0, 0, // next IFD
        ];
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend(&tiff);
        let mut out = vec![0xFF, 0xD8];
        out.extend([0xFF, 0xE0, 0, 16]);
        out.extend(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        out.extend([0xFF, 0xE1]);
        out.extend(((app1.len() + 2) as u16).to_be_bytes());
        out.extend(&app1);
        out.extend([0xFF, 0xDA, 0, 4, 0, 0, 1, 2, 3, 0xFF, 0xD9]);
        out
    }

    #[test]
    fn reads_exif_and_groups() {
        let m = read(&jpeg_with_exif()).unwrap();
        let make = m
            .entries
            .iter()
            .find(|e| e.tag == "Make")
            .expect("Make tag");
        assert!(make.value.contains("KB"));
        assert_eq!(make.group, "device");
        assert!(!m.has_gps);
        assert_eq!(group_of("GPSLatitude"), "gps");
        assert_eq!(group_of("DateTimeOriginal"), "time");
    }

    #[test]
    fn strips_exif_without_touching_the_rest() {
        let src = jpeg_with_exif();
        let (out, removed) = strip_jpeg_metadata(&src).unwrap();
        assert_eq!(removed, 1);
        assert!(out.len() < src.len());
        assert!(read(&out).unwrap().entries.is_empty());
        // JFIF segment and the scan data are byte-identical.
        assert!(out.windows(4).any(|w| w == b"JFIF"));
        assert!(out.ends_with(&[0xFF, 0xDA, 0, 4, 0, 0, 1, 2, 3, 0xFF, 0xD9]));
    }

    #[test]
    fn rejects_non_jpeg() {
        assert!(strip_jpeg_metadata(b"GIF89a....").is_err());
        assert!(strip_jpeg_metadata(&[0xFF, 0xD8, 0x00, 0x00, 0x00]).is_err());
        assert_eq!(
            read(b"just text here")
                .map(|m| m.entries.len())
                .unwrap_or(0),
            0
        );
    }
}
