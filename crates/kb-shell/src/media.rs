//! Audio/video info and conversion through FFmpeg (tasks 3C.1 and 3C.2): `probe` runs ffprobe
//! and returns format and stream details; `convert` runs ffmpeg with a preset, optional trim
//! (`start` / `end` in seconds) and never overwrites an existing file.
//!
//! The binaries are looked up in `KB_FFMPEG_DIR`, `<profile>/components/ffmpeg/bin`, then PATH.
//! Limitation: ffmpeg/ffprobe run as ordinary child processes; the planned sandboxed `kb-media`
//! host is not built yet, so only open media you trust.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MEDIA_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "webm", "mov", "avi", "m4v", "flv", "wmv", "mp3", "m4a", "aac", "flac", "ogg",
    "opus", "wav", "wma",
];

fn tool(name: &str) -> String {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let candidates = [
        std::env::var_os("KB_FFMPEG_DIR").map(PathBuf::from),
        crate::shell::profile_dir().map(|p| p.join("components/ffmpeg/bin")),
    ];
    for dir in candidates.into_iter().flatten() {
        let p = dir.join(&exe);
        if p.exists() {
            return p.display().to_string();
        }
    }
    exe
}

fn media_path(args: &Value) -> Result<PathBuf, String> {
    let p = PathBuf::from(args.get("path").and_then(Value::as_str).unwrap_or_default());
    if !p.is_absolute() {
        return Err("path must be absolute".into());
    }
    let ok = p
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MEDIA_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    if !ok {
        return Err("unsupported media type".into());
    }
    if !p.is_file() {
        return Err("file not found".into());
    }
    Ok(p)
}

fn seconds(args: &Value, k: &str) -> Result<Option<f64>, String> {
    match args.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 0.0)
            .map(Some)
            .ok_or_else(|| format!("{k} must be a number of seconds >= 0")),
    }
}

/// `(extension, ffmpeg output arguments)` for a preset name.
fn preset(name: &str) -> Result<(&'static str, Vec<&'static str>), String> {
    Ok(match name {
        "mp4" => (
            "mp4",
            vec![
                "-c:v",
                "libx264",
                "-crf",
                "23",
                "-preset",
                "medium",
                "-c:a",
                "aac",
                "-movflags",
                "+faststart",
            ],
        ),
        "webm" => (
            "webm",
            vec![
                "-c:v",
                "libvpx-vp9",
                "-crf",
                "32",
                "-b:v",
                "0",
                "-c:a",
                "libopus",
            ],
        ),
        "mp3" => ("mp3", vec!["-vn", "-c:a", "libmp3lame", "-q:a", "2"]),
        "m4a" => ("m4a", vec!["-vn", "-c:a", "aac", "-b:a", "192k"]),
        "copy" => ("mkv", vec!["-c", "copy"]),
        other => return Err(format!("unknown preset: {other}")),
    })
}

/// ffmpeg arguments for a conversion (pure; unit-tested).
pub fn convert_args(
    input: &Path,
    output: &Path,
    preset_name: &str,
    start: Option<f64>,
    end: Option<f64>,
) -> Result<Vec<String>, String> {
    let (_, enc) = preset(preset_name)?;
    if let (Some(s), Some(e)) = (start, end)
        && e <= s
    {
        return Err("end must be after start".into());
    }
    let mut a: Vec<String> = vec!["-hide_banner".into(), "-nostdin".into(), "-n".into()];
    if let Some(s) = start {
        a.extend(["-ss".into(), s.to_string()]);
    }
    a.extend(["-i".into(), input.display().to_string()]);
    if let Some(e) = end {
        // With -ss before -i the output timestamps restart at 0, so -t is the duration.
        let dur = e - start.unwrap_or(0.0);
        a.extend(["-t".into(), dur.to_string()]);
    }
    a.extend(enc.iter().map(|s| s.to_string()));
    a.push(output.display().to_string());
    Ok(a)
}

pub fn call(op: &str, args: &Value) -> Result<String, String> {
    match op {
        "probe" => {
            let p = media_path(args)?;
            let out = Command::new(tool("ffprobe"))
                .args([
                    "-v",
                    "error",
                    "-print_format",
                    "json",
                    "-show_format",
                    "-show_streams",
                ])
                .arg(&p)
                .stdin(Stdio::null())
                .output()
                .map_err(|e| {
                    format!("ffprobe not available ({e}); install FFmpeg or set KB_FFMPEG_DIR")
                })?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
            }
            let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
            let streams: Vec<Value> = v["streams"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|s| {
                    json!({ "type": s["codec_type"], "codec": s["codec_name"], "width": s["width"],
                            "height": s["height"], "fps": s["avg_frame_rate"],
                            "channels": s["channels"], "sample_rate": s["sample_rate"],
                            "bit_rate": s["bit_rate"] })
                })
                .collect();
            Ok(json!({ "format": v["format"]["format_long_name"], "duration": v["format"]["duration"],
                       "size": v["format"]["size"], "bit_rate": v["format"]["bit_rate"],
                       "streams": streams })
            .to_string())
        }
        "convert" => {
            let p = media_path(args)?;
            let name = args.get("preset").and_then(Value::as_str).unwrap_or("mp4");
            let (ext, _) = preset(name)?;
            let dir = p.parent().unwrap_or(Path::new("."));
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("media");
            let target = kb_platform::files::unique_path(dir, &format!("{stem} (converted).{ext}"));
            let a = convert_args(
                &p,
                &target,
                name,
                seconds(args, "start")?,
                seconds(args, "end")?,
            )?;
            let out = Command::new(tool("ffmpeg"))
                .args(&a)
                .stdin(Stdio::null())
                .output()
                .map_err(|e| {
                    format!("ffmpeg not available ({e}); install FFmpeg or set KB_FFMPEG_DIR")
                })?;
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                let tail: Vec<&str> = err.lines().rev().take(4).collect();
                return Err(tail.into_iter().rev().collect::<Vec<_>>().join("\n"));
            }
            Ok(json!({ "path": target.display().to_string() }).to_string())
        }
        _ => Err(format!("unknown media op: {op}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_trim_and_convert_arguments() {
        let a = convert_args(
            Path::new("/in.mkv"),
            Path::new("/out.mp4"),
            "mp4",
            Some(10.0),
            Some(25.5),
        )
        .unwrap();
        let i = a.iter().position(|x| x == "-ss").unwrap();
        assert_eq!(a[i + 1], "10");
        assert!(a.windows(2).any(|w| w[0] == "-t" && w[1] == "15.5"));
        assert!(a.contains(&"-n".to_string()), "never overwrites");
        assert_eq!(a.last().unwrap(), "/out.mp4");
        assert!(
            convert_args(
                Path::new("/i.mp4"),
                Path::new("/o.mp4"),
                "mp4",
                Some(5.0),
                Some(5.0)
            )
            .is_err()
        );
        assert!(
            convert_args(Path::new("/i.mp4"), Path::new("/o.xyz"), "nope", None, None).is_err()
        );
    }

    #[test]
    fn rejects_bad_paths_and_types() {
        assert!(media_path(&json!({ "path": "rel.mp4" })).is_err());
        assert!(media_path(&json!({ "path": "/x/y.exe" })).is_err());
        assert!(seconds(&json!({ "start": -1 }), "start").is_err());
        assert_eq!(seconds(&json!({}), "start").unwrap(), None);
    }
}
