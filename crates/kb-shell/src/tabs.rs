//! Tab list logic (pure, no CEF types): ordering, activation and what becomes active after a
//! close. The CEF side (`shell.rs`) stores a `BrowserView` per tab.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(pub u64);

#[derive(Debug)]
pub struct Tabs<T> {
    items: Vec<(TabId, T)>,
    active: Option<TabId>,
    next_id: u64,
}

impl<T> Default for Tabs<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            active: None,
            next_id: 1,
        }
    }
}

impl<T> Tabs<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn active(&self) -> Option<TabId> {
        self.active
    }

    pub fn active_item(&self) -> Option<&T> {
        self.active.and_then(|id| self.get(id))
    }

    pub fn get(&self, id: TabId) -> Option<&T> {
        self.items.iter().find(|(i, _)| *i == id).map(|(_, t)| t)
    }

    pub fn get_mut(&mut self, id: TabId) -> Option<&mut T> {
        self.items
            .iter_mut()
            .find(|(i, _)| *i == id)
            .map(|(_, t)| t)
    }

    pub fn iter(&self) -> impl Iterator<Item = (TabId, &T)> {
        self.items.iter().map(|(i, t)| (*i, t))
    }

    pub fn find(&self, mut pred: impl FnMut(&T) -> bool) -> Option<TabId> {
        self.items.iter().find(|(_, t)| pred(t)).map(|(i, _)| *i)
    }

    fn index(&self, id: TabId) -> Option<usize> {
        self.items.iter().position(|(i, _)| *i == id)
    }

    /// Opens a tab right after the active one (or at the end) and optionally activates it.
    pub fn open(&mut self, item: T, activate: bool) -> TabId {
        let id = TabId(self.next_id);
        self.next_id += 1;
        let at = self
            .active
            .and_then(|a| self.index(a))
            .map_or(self.items.len(), |i| i + 1);
        self.items.insert(at, (id, item));
        if activate || self.active.is_none() {
            self.active = Some(id);
        }
        id
    }

    pub fn activate(&mut self, id: TabId) -> bool {
        let found = self.index(id).is_some();
        if found {
            self.active = Some(id);
        }
        found
    }

    /// Closes a tab. If it was active, the tab to its right (else left) becomes active.
    pub fn close(&mut self, id: TabId) -> Option<T> {
        let i = self.index(id)?;
        let (_, item) = self.items.remove(i);
        if self.active == Some(id) {
            self.active = self
                .items
                .get(i)
                .or_else(|| i.checked_sub(1).and_then(|j| self.items.get(j)))
                .map(|(id, _)| *id);
        }
        Some(item)
    }

    /// Next (`forward`) or previous tab, wrapping around.
    pub fn cycle(&mut self, forward: bool) -> Option<TabId> {
        let n = self.items.len();
        let i = self.active.and_then(|a| self.index(a))?;
        let j = if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        };
        self.active = Some(self.items[j].0);
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_inserts_after_active_and_activates() {
        let mut t = Tabs::new();
        let a = t.open("a", true);
        let b = t.open("b", true);
        t.activate(a);
        let c = t.open("c", false);
        let order: Vec<_> = t.iter().map(|(_, s)| *s).collect();
        assert_eq!(order, ["a", "c", "b"]);
        assert_eq!(t.active(), Some(a));
        assert_ne!(b, c);
    }

    #[test]
    fn first_tab_is_always_active() {
        let mut t = Tabs::new();
        let a = t.open(1, false);
        assert_eq!(t.active(), Some(a));
    }

    #[test]
    fn closing_active_prefers_right_then_left() {
        let mut t = Tabs::new();
        let a = t.open("a", true);
        let b = t.open("b", true);
        let c = t.open("c", true);
        t.activate(b);
        assert_eq!(t.close(b), Some("b"));
        assert_eq!(t.active(), Some(c));
        assert_eq!(t.close(c), Some("c"));
        assert_eq!(t.active(), Some(a));
        assert_eq!(t.close(a), Some("a"));
        assert_eq!(t.active(), None);
        assert!(t.is_empty());
    }

    #[test]
    fn closing_inactive_keeps_active() {
        let mut t = Tabs::new();
        let a = t.open("a", true);
        let b = t.open("b", true);
        t.close(a);
        assert_eq!(t.active(), Some(b));
        assert_eq!(t.close(a), None);
    }

    #[test]
    fn cycle_wraps() {
        let mut t = Tabs::new();
        let a = t.open("a", true);
        let b = t.open("b", true);
        assert_eq!(t.cycle(true), Some(a));
        assert_eq!(t.cycle(false), Some(b));
        assert_eq!(t.cycle(true), Some(a));
        assert_eq!(Tabs::<()>::new().cycle(true), None);
    }
}
