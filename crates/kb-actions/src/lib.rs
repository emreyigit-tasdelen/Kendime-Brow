//! Action registry (architecture §3.5): every module registers its capabilities here; the
//! command palette, shortcuts, context menus and automation chains all use the same registry.

use std::collections::BTreeMap;
use std::fmt;

/// Data type flowing between chained actions, e.g. `files`, `pdf`, `none`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DataType(pub String);

impl DataType {
    pub fn new(s: &str) -> Self {
        Self(s.to_string())
    }
    pub fn none() -> Self {
        Self::new("none")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    /// Stable id, `<module>.<verb>`, e.g. `pdf.merge`.
    pub id: String,
    pub title: String,
    pub module: String,
    pub input: DataType,
    pub output: DataType,
    /// Dangerous actions need confirmation before running.
    pub dangerous: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RegistryError {
    InvalidId(String),
    Duplicate(String),
    Unknown(String),
    EmptyChain,
    TypeMismatch {
        from: String,
        to: String,
        output: DataType,
        input: DataType,
    },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(id) => write!(f, "invalid action id `{id}` (expected `module.verb`)"),
            Self::Duplicate(id) => write!(f, "action `{id}` is already registered"),
            Self::Unknown(id) => write!(f, "unknown action `{id}`"),
            Self::EmptyChain => write!(f, "chain is empty"),
            Self::TypeMismatch {
                from,
                to,
                output,
                input,
            } => write!(
                f,
                "`{from}` outputs `{}` but `{to}` expects `{}`",
                output.0, input.0
            ),
        }
    }
}

impl std::error::Error for RegistryError {}

#[derive(Debug, Default)]
pub struct Registry {
    actions: BTreeMap<String, Action>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, action: Action) -> Result<(), RegistryError> {
        let valid = action
            .id
            .split_once('.')
            .is_some_and(|(m, v)| !m.is_empty() && !v.is_empty() && m == action.module);
        if !valid {
            return Err(RegistryError::InvalidId(action.id));
        }
        if self.actions.contains_key(&action.id) {
            return Err(RegistryError::Duplicate(action.id));
        }
        self.actions.insert(action.id.clone(), action);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Action> {
        self.actions.get(id)
    }

    /// All actions, sorted by id.
    pub fn list(&self) -> impl Iterator<Item = &Action> {
        self.actions.values()
    }

    /// Case-insensitive search over id and title, for the command palette.
    pub fn search<'a>(&'a self, query: &'a str) -> impl Iterator<Item = &'a Action> + 'a {
        let q = query.to_lowercase();
        self.actions.values().filter(move |a| {
            a.id.to_lowercase().contains(&q) || a.title.to_lowercase().contains(&q)
        })
    }

    /// Validates a chain: every action exists and each output type matches the next input type.
    pub fn validate_chain(&self, ids: &[&str]) -> Result<Vec<&Action>, RegistryError> {
        if ids.is_empty() {
            return Err(RegistryError::EmptyChain);
        }
        let actions = ids
            .iter()
            .map(|id| {
                self.get(id)
                    .ok_or_else(|| RegistryError::Unknown(id.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        for pair in actions.windows(2) {
            if pair[0].output != pair[1].input {
                return Err(RegistryError::TypeMismatch {
                    from: pair[0].id.clone(),
                    to: pair[1].id.clone(),
                    output: pair[0].output.clone(),
                    input: pair[1].input.clone(),
                });
            }
        }
        Ok(actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(id: &str, input: &str, output: &str) -> Action {
        Action {
            id: id.into(),
            title: id.replace('.', " "),
            module: id.split('.').next().unwrap().into(),
            input: DataType::new(input),
            output: DataType::new(output),
            dangerous: false,
        }
    }

    fn sample() -> Registry {
        let mut r = Registry::new();
        r.register(action("files.select", "none", "files")).unwrap();
        r.register(action("pdf.merge", "files", "pdf")).unwrap();
        r.register(action("pdf.compress", "pdf", "pdf")).unwrap();
        r.register(action("file.save", "pdf", "none")).unwrap();
        r
    }

    #[test]
    fn rejects_bad_and_duplicate_ids() {
        let mut r = sample();
        assert_eq!(
            r.register(action("pdf.merge", "files", "pdf")),
            Err(RegistryError::Duplicate("pdf.merge".into()))
        );
        let mut bad = action("merge", "files", "pdf");
        bad.module = "pdf".into();
        assert!(matches!(r.register(bad), Err(RegistryError::InvalidId(_))));
    }

    #[test]
    fn valid_chain_from_architecture_example() {
        let r = sample();
        let chain = r
            .validate_chain(&["files.select", "pdf.merge", "pdf.compress", "file.save"])
            .unwrap();
        assert_eq!(chain.len(), 4);
    }

    #[test]
    fn chain_errors() {
        let r = sample();
        assert_eq!(r.validate_chain(&[]), Err(RegistryError::EmptyChain));
        assert_eq!(
            r.validate_chain(&["pdf.nope"]),
            Err(RegistryError::Unknown("pdf.nope".into()))
        );
        assert!(matches!(
            r.validate_chain(&["files.select", "pdf.compress"]),
            Err(RegistryError::TypeMismatch { .. })
        ));
    }

    #[test]
    fn search_is_case_insensitive() {
        let r = sample();
        let ids: Vec<_> = r.search("PDF").map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["pdf.compress", "pdf.merge"]);
    }
}
