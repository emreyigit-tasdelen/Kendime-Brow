//! File-system helpers for downloads (architecture §8.3, task 1.13).

use std::path::{Path, PathBuf};

/// The user's downloads folder (`~/Downloads`; `XDG_DOWNLOAD_DIR` on Linux when set).
pub fn downloads_dir() -> Option<PathBuf> {
    if cfg!(target_os = "linux")
        && let Some(dir) = std::env::var_os("XDG_DOWNLOAD_DIR").filter(|d| !d.is_empty())
    {
        return Some(PathBuf::from(dir));
    }
    let home = if cfg!(windows) {
        std::env::var_os("USERPROFILE")
    } else {
        std::env::var_os("HOME")
    }?;
    Some(PathBuf::from(home).join("Downloads"))
}

/// Makes a server-suggested file name safe: no directories, no reserved characters or
/// Windows device names, no leading dots, bounded length.
pub fn sanitize_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut out: String = base
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    out = out
        .trim()
        .trim_start_matches('.')
        .trim_end_matches(['.', ' '])
        .to_string();
    let stem = out
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&stem.as_str()) {
        out.insert(0, '_');
    }
    if out.is_empty() {
        out = "download".into();
    }
    if out.len() > 200 {
        let mut cut = 200;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
    }
    out
}

/// `dir/name`, or `dir/name (1).ext`, `dir/name (2).ext`, … if it already exists.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    unique_path_with(dir, name, |p| p.exists())
}

pub fn unique_path_with(dir: &Path, name: &str, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let first = dir.join(name);
    if !exists(&first) {
        return first;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !exists(p))
        .expect("unbounded search")
}

/// Shows a file in the platform file manager (selected where supported).
pub fn reveal(path: &Path) -> std::io::Result<()> {
    use std::process::Command;
    if cfg!(windows) {
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(path);
        Command::new("explorer.exe").arg(arg).spawn()?;
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg("-R").arg(path).spawn()?;
    } else {
        let dir = path.parent().unwrap_or(path);
        Command::new("xdg-open").arg(dir).spawn()?;
    }
    Ok(())
}

/// Download rules: one `ext1,ext2 = Subfolder` per line (`#` comments). Returns the subfolder (a
/// single sanitized path segment inside the downloads folder) for the first rule matching the file.
pub fn rule_subdir(rules: &str, file_name: &str) -> Option<String> {
    let ext = Path::new(file_name)
        .extension()?
        .to_str()?
        .to_ascii_lowercase();
    rules.lines().find_map(|line| {
        let (exts, dir) = line.split_once('=')?;
        let hit = exts
            .split(',')
            .any(|e| e.trim().trim_start_matches('.').eq_ignore_ascii_case(&ext));
        let dir = sanitize_file_name(dir.trim());
        (hit && !line.trim_start().starts_with('#') && dir != "download").then_some(dir)
    })
}

/// File types that run code when opened; the browser never opens these itself.
pub fn is_executable_type(path: &Path) -> bool {
    const RISKY: &[&str] = &[
        "exe", "com", "bat", "cmd", "msi", "msp", "ps1", "psm1", "vbs", "vbe", "js", "jse", "wsf",
        "wsh", "scr", "pif", "lnk", "hta", "jar", "reg", "dll", "cpl", "msc", "sh", "appimage",
        "app", "command", "desktop", "deb", "rpm", "apk", "dmg", "pkg",
    ];
    path.extension()
        .and_then(|e| e.to_str())
        .is_none_or(|e| RISKY.contains(&e.to_ascii_lowercase().as_str()))
}

/// Opens a file with the default application. Refuses executable types and files without an
/// extension (use `reveal` for those).
pub fn open_default(path: &Path) -> std::io::Result<()> {
    use std::process::Command;
    if is_executable_type(path) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "refusing to open an executable file type",
        ));
    }
    if cfg!(windows) {
        Command::new("explorer.exe").arg(path).spawn()?;
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(path).spawn()?;
    } else {
        Command::new("xdg-open").arg(path).spawn()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_pick_a_safe_subfolder() {
        let r = "# comment
pdf, .docx = Documents
jpg,png=Pictures
zip = ../../evil
";
        assert_eq!(rule_subdir(r, "a.PDF").as_deref(), Some("Documents"));
        assert_eq!(rule_subdir(r, "b.png").as_deref(), Some("Pictures"));
        assert_eq!(rule_subdir(r, "c.zip").as_deref(), Some("evil"));
        assert_eq!(rule_subdir(r, "d.txt"), None);
        assert_eq!(rule_subdir(r, "noext"), None);
    }

    #[test]
    fn executables_are_never_opened() {
        for n in [
            "a.exe", "A.BAT", "x.ps1", "y.lnk", "z.msi", "noext", "a.tar.sh",
        ] {
            assert!(is_executable_type(Path::new(n)), "{n}");
        }
        for n in ["a.pdf", "b.PNG", "c.txt", "d.zip"] {
            assert!(!is_executable_type(Path::new(n)), "{n}");
        }
    }

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_file_name("report.pdf"), "report.pdf");
        assert_eq!(sanitize_file_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_file_name(r"C:\Windows\evil.exe"), "evil.exe");
        assert_eq!(sanitize_file_name("a<b>:c?.txt"), "a_b__c_.txt");
        assert_eq!(sanitize_file_name(".hidden"), "hidden");
        assert_eq!(sanitize_file_name("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_file_name("name. "), "name");
        assert_eq!(sanitize_file_name(""), "download");
        assert_eq!(sanitize_file_name("..."), "download");
        assert!(sanitize_file_name(&"ş".repeat(300)).len() <= 200);
    }

    #[test]
    fn unique_paths() {
        let dir = Path::new("/d");
        let taken = ["/d/a.txt", "/d/a (1).txt", "/d/b"];
        let exists = |p: &Path| taken.iter().any(|t| Path::new(t) == p);
        assert_eq!(unique_path_with(dir, "c.txt", exists), dir.join("c.txt"));
        assert_eq!(
            unique_path_with(dir, "a.txt", exists),
            dir.join("a (2).txt")
        );
        assert_eq!(unique_path_with(dir, "b", exists), dir.join("b (1)"));
    }
}
