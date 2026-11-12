//! Platform abstraction layer (architecture §3.8).
//!
//! Rule: this crate is the only place in the Rust workspace that uses `#[cfg(target_os)]`.
//! Other crates call the functions and traits exposed here.

use std::path::PathBuf;

pub mod files;
pub mod ipc;
pub mod sandbox;

/// Application directory name used on Windows and macOS.
pub const APP_DIR_NAME: &str = "Kendime-Brow";
/// Application directory name used on Linux (XDG convention: lowercase).
pub const APP_DIR_NAME_LINUX: &str = "kendime-brow";

/// AppUserModelID shared by every Kendime-Brow process, so Windows groups all windows and
/// helper processes under one taskbar icon (architecture §13).
pub const APP_USER_MODEL_ID: &str = "EmreYigitTasdelen.KendimeBrow";

/// Sets the process AppUserModelID (Windows; no-op elsewhere). Call first thing in `main`.
pub fn set_app_user_model_id() {
    #[cfg(windows)]
    {
        let id: Vec<u16> = APP_USER_MODEL_ID
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: valid NUL-terminated UTF-16 string for the duration of the call.
        unsafe {
            windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(id.as_ptr());
        }
    }
}

/// The current platform, for features that must report "not supported on this platform".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Linux,
    MacOs,
    Other,
}

pub fn current() -> Platform {
    if cfg!(target_os = "windows") {
        Platform::Windows
    } else if cfg!(target_os = "linux") {
        Platform::Linux
    } else if cfg!(target_os = "macos") {
        Platform::MacOs
    } else {
        Platform::Other
    }
}

/// Root data directory (architecture §3.7 / §3.8):
/// - Windows: `%LOCALAPPDATA%\Kendime-Brow`
/// - Linux: `$XDG_DATA_HOME/kendime-brow` (fallback `~/.local/share/kendime-brow`)
/// - macOS: `~/Library/Application Support/Kendime-Brow`
pub fn data_dir() -> Option<PathBuf> {
    data_dir_from(&|k| std::env::var_os(k).map(PathBuf::from), current())
}

/// Testable core of [`data_dir`]: `env` looks up an environment variable.
pub fn data_dir_from(env: &dyn Fn(&str) -> Option<PathBuf>, platform: Platform) -> Option<PathBuf> {
    let non_empty = |k: &str| env(k).filter(|p| !p.as_os_str().is_empty());
    match platform {
        Platform::Windows => non_empty("LOCALAPPDATA").map(|p| p.join(APP_DIR_NAME)),
        Platform::Linux | Platform::Other => non_empty("XDG_DATA_HOME")
            .or_else(|| non_empty("HOME").map(|h| h.join(".local").join("share")))
            .map(|p| p.join(APP_DIR_NAME_LINUX)),
        Platform::MacOs => non_empty("HOME").map(|h| {
            h.join("Library")
                .join("Application Support")
                .join(APP_DIR_NAME)
        }),
    }
}

/// Subdirectories of the data directory (architecture §3.7).
pub mod layout {
    use std::path::{Path, PathBuf};

    pub fn profile(root: &Path, name: &str) -> PathBuf {
        root.join("profiles").join(name)
    }
    pub fn vault(root: &Path) -> PathBuf {
        root.join("vault")
    }
    pub fn components(root: &Path) -> PathBuf {
        root.join("components")
    }
    pub fn logs(root: &Path) -> PathBuf {
        root.join("logs")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<PathBuf> {
        let map: HashMap<String, PathBuf> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), PathBuf::from(v)))
            .collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn windows_uses_localappdata() {
        let env = env_of(&[("LOCALAPPDATA", "C:/Users/u/AppData/Local")]);
        assert_eq!(
            data_dir_from(&env, Platform::Windows),
            Some(PathBuf::from("C:/Users/u/AppData/Local").join("Kendime-Brow"))
        );
    }

    #[test]
    fn linux_prefers_xdg_then_home() {
        let env = env_of(&[("XDG_DATA_HOME", "/x"), ("HOME", "/home/u")]);
        assert_eq!(
            data_dir_from(&env, Platform::Linux),
            Some(PathBuf::from("/x").join("kendime-brow"))
        );
        let env = env_of(&[("XDG_DATA_HOME", ""), ("HOME", "/home/u")]);
        assert_eq!(
            data_dir_from(&env, Platform::Linux),
            Some(PathBuf::from("/home/u/.local/share").join("kendime-brow"))
        );
    }

    #[test]
    fn macos_uses_application_support() {
        let env = env_of(&[("HOME", "/Users/u")]);
        assert_eq!(
            data_dir_from(&env, Platform::MacOs),
            Some(PathBuf::from("/Users/u/Library/Application Support").join("Kendime-Brow"))
        );
    }

    #[test]
    fn missing_env_gives_none() {
        let env = env_of(&[]);
        assert_eq!(data_dir_from(&env, Platform::Windows), None);
        assert_eq!(data_dir_from(&env, Platform::Linux), None);
    }
}
