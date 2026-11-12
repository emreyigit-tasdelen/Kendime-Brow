//! Shell actions in the action registry (architecture §3.5, task 1.11). The command palette,
//! shortcuts and (later) automation chains all resolve to these ids.

use std::sync::OnceLock;

use kb_actions::{Action, DataType, Registry};
use serde::Serialize;

use crate::protocol::Command;

/// (id, title, command)
fn table() -> Vec<(&'static str, &'static str, Command)> {
    vec![
        (
            "tab.new",
            "New tab",
            Command::NewTab {
                url: None,
                container: None,
            },
        ),
        ("tab.close", "Close tab", Command::CloseActiveTab),
        (
            "tab.new-private",
            "New private tab",
            Command::NewTab {
                url: None,
                container: Some("temporary".into()),
            },
        ),
        (
            "tab.new-work",
            "New tab in the Work container",
            Command::NewTab {
                url: None,
                container: Some("work".into()),
            },
        ),
        (
            "tab.new-personal",
            "New tab in the Personal container",
            Command::NewTab {
                url: None,
                container: Some("personal".into()),
            },
        ),
        ("tab.next", "Next tab", Command::CycleTab { forward: true }),
        (
            "tab.prev",
            "Previous tab",
            Command::CycleTab { forward: false },
        ),
        ("nav.back", "Go back", Command::Back),
        ("nav.forward", "Go forward", Command::Forward),
        ("nav.reload", "Reload page", Command::Reload),
        ("nav.stop", "Stop loading", Command::Stop),
        (
            "bookmark.toggle",
            "Bookmark this page",
            Command::ToggleBookmark,
        ),
        (
            "shield.toggle",
            "Toggle content blocking for this site",
            Command::ToggleShield,
        ),
        (
            "terminal.open",
            "Open a terminal",
            Command::NewTab {
                url: Some("kb://term/".into()),
                container: None,
            },
        ),
        (
            "net.open",
            "Open the network inspector",
            Command::NewTab {
                url: Some("kb://net/".into()),
                container: None,
            },
        ),
        (
            "connections.open",
            "Open saved connections (SSH)",
            Command::NewTab {
                url: Some("kb://connections/".into()),
                container: None,
            },
        ),
        (
            "files.open",
            "Open remote files (SFTP)",
            Command::NewTab {
                url: Some("kb://files/".into()),
                container: None,
            },
        ),
        (
            "vault.open",
            "Open the password vault",
            Command::NewTab {
                url: Some("kb://vault/".into()),
                container: None,
            },
        ),
        (
            "tools.open",
            "Open developer tools (JSON, regex, hash, base64, JWT)",
            Command::NewTab {
                url: Some("kb://tools/".into()),
                container: None,
            },
        ),
        (
            "tasks.open",
            "Show task manager",
            Command::NewTab {
                url: Some("kb://tasks/".into()),
                container: None,
            },
        ),
        (
            "downloads.open",
            "Show downloads",
            Command::NewTab {
                url: Some("kb://downloads/".into()),
                container: None,
            },
        ),
        (
            "settings.open",
            "Open settings",
            Command::NewTab {
                url: Some("kb://settings/".into()),
                container: None,
            },
        ),
    ]
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut r = Registry::new();
        for (id, title, _) in table() {
            r.register(Action {
                id: id.into(),
                title: title.into(),
                module: id.split('.').next().unwrap_or_default().into(),
                input: DataType::none(),
                output: DataType::none(),
                dangerous: false,
            })
            .expect("built-in action table is valid");
        }
        r
    })
}

#[derive(Debug, Serialize)]
pub struct ActionInfo {
    pub id: String,
    pub title: String,
    pub module: String,
}

/// Actions for the palette, optionally filtered by a search query.
pub fn list(query: &str) -> Vec<ActionInfo> {
    registry()
        .search(query)
        .map(|a| ActionInfo {
            id: a.id.clone(),
            title: a.title.clone(),
            module: a.module.clone(),
        })
        .collect()
}

pub fn list_json(query: &str) -> String {
    serde_json::to_string(&list(query)).unwrap_or_else(|_| "[]".into())
}

/// The command an action id runs.
pub fn command_for(id: &str) -> Option<Command> {
    registry().get(id)?;
    table()
        .into_iter()
        .find(|(i, ..)| *i == id)
        .map(|(.., c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_resolves() {
        let all = list("");
        assert_eq!(all.len(), table().len());
        for a in &all {
            assert!(command_for(&a.id).is_some(), "{}", a.id);
        }
        assert!(command_for("tab.nope").is_none());
    }

    #[test]
    fn search_and_json() {
        let ids: Vec<_> = list("TAB").into_iter().map(|a| a.id).collect();
        assert!(ids.contains(&"tab.new".to_string()));
        assert!(!ids.contains(&"nav.back".to_string()));
        assert!(list_json("reload").contains("\"nav.reload\""));
    }
}
