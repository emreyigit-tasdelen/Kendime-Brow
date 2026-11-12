//! Resource monitor (architecture §5.5, task 2.10): memory/CPU of the browser's own process tree.

use std::sync::Mutex;

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskInfo {
    pub pid: u32,
    pub name: String,
    /// `browser`, `renderer`, `gpu`, `utility`, `other`.
    pub kind: String,
    pub memory: u64,
    pub cpu: f32,
}

static SYS: Mutex<Option<System>> = Mutex::new(None);

/// Process kind from the helper's `--type=` argument.
pub fn kind_of(args: &[String], is_root: bool) -> &'static str {
    if is_root {
        return "browser";
    }
    match args.iter().find_map(|a| a.strip_prefix("--type=")) {
        Some("renderer") => "renderer",
        Some("gpu-process") => "gpu",
        Some("utility") => "utility",
        _ => "other",
    }
}

/// Own process plus all descendants.
pub fn list() -> Vec<TaskInfo> {
    let Ok(mut guard) = SYS.lock() else {
        return Vec::new();
    };
    if guard.is_none() {
        match System::new() {
            Ok(s) => *guard = Some(s),
            Err(_) => return Vec::new(),
        }
    }
    let Some(sys) = guard.as_mut() else {
        return Vec::new();
    };
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::everything(),
    );
    let me = Pid::from_u32(std::process::id());
    let mut tasks = Vec::new();
    for (pid, p) in sys.processes() {
        let mut cur = Some(*pid);
        let mut ours = false;
        for _ in 0..8 {
            match cur {
                Some(c) if c == me => {
                    ours = true;
                    break;
                }
                Some(c) => cur = sys.process(c).and_then(|x| x.parent()),
                None => break,
            }
        }
        if !ours {
            continue;
        }
        let args: Vec<String> = p
            .cmd()
            .iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        tasks.push(TaskInfo {
            pid: pid.as_u32(),
            name: p.name().to_string_lossy().to_string(),
            kind: kind_of(&args, *pid == me).to_string(),
            memory: p.memory(),
            cpu: p.cpu_usage(),
        });
    }
    tasks.sort_by_key(|t| std::cmp::Reverse(t.memory));
    tasks
}

/// Kills a descendant of this process (never the browser itself).
pub fn kill(pid: u32) -> bool {
    if pid == std::process::id() || !list().iter().any(|t| t.pid == pid) {
        return false;
    }
    SYS.lock()
        .ok()
        .and_then(|g| {
            g.as_ref()
                .and_then(|s| s.process(Pid::from_u32(pid)).map(|p| p.kill()))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        let a = |s: &str| vec![s.to_string()];
        assert_eq!(kind_of(&a("--type=renderer"), false), "renderer");
        assert_eq!(kind_of(&a("--type=gpu-process"), false), "gpu");
        assert_eq!(kind_of(&[], true), "browser");
        assert_eq!(kind_of(&a("--x"), false), "other");
    }

    #[test]
    fn lists_self() {
        let t = list();
        assert!(
            t.iter()
                .any(|x| x.pid == std::process::id() && x.kind == "browser")
        );
        assert!(!kill(std::process::id()));
    }
}
