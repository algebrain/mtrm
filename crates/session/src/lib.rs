//! Представление сохраняемого состояния: вкладки, раскладки и рабочие каталоги.

use std::collections::BTreeSet;
use std::path::PathBuf;

use mtrm_core::{PaneId, TabId};
use mtrm_layout::{LayoutSnapshot, LayoutTree};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub tabs: Vec<TabSnapshot>,
    pub active_tab: TabId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabSnapshot {
    pub id: TabId,
    pub title: String,
    pub layout: LayoutSnapshot,
    pub panes: Vec<PaneSnapshot>,
    pub active_pane: PaneId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneSnapshot {
    pub id: PaneId,
    pub cwd: PathBuf,
}

impl SessionSnapshot {
    pub fn validate(&self) -> Result<(), SessionValidationError> {
        if self.tabs.is_empty() {
            return Err(SessionValidationError::NoTabs);
        }

        let mut seen_tabs = BTreeSet::new();
        let mut seen_panes = BTreeSet::new();
        let mut has_active_tab = false;

        for tab in &self.tabs {
            if !seen_tabs.insert(tab.id) {
                return Err(SessionValidationError::DuplicateTabId(tab.id));
            }

            if tab.id == self.active_tab {
                has_active_tab = true;
            }

            let layout_tree = LayoutTree::from_snapshot(tab.layout.clone()).map_err(|_| {
                SessionValidationError::MissingPaneInLayout {
                    tab_id: tab.id,
                    pane_id: tab.active_pane,
                }
            })?;
            let layout_panes: BTreeSet<_> = layout_tree.pane_ids().into_iter().collect();
            let pane_ids: BTreeSet<_> = tab.panes.iter().map(|pane| pane.id).collect();

            if !pane_ids.contains(&tab.active_pane) {
                return Err(SessionValidationError::MissingActivePane {
                    tab_id: tab.id,
                    pane_id: tab.active_pane,
                });
            }

            for pane in &tab.panes {
                if !seen_panes.insert(pane.id) {
                    return Err(SessionValidationError::DuplicatePaneId(pane.id));
                }
            }

            for pane_id in &layout_panes {
                if !pane_ids.contains(pane_id) {
                    return Err(SessionValidationError::MissingPaneInLayout {
                        tab_id: tab.id,
                        pane_id: *pane_id,
                    });
                }
            }

            if !layout_panes.contains(&tab.active_pane) {
                return Err(SessionValidationError::MissingPaneInLayout {
                    tab_id: tab.id,
                    pane_id: tab.active_pane,
                });
            }
        }

        if !has_active_tab {
            return Err(SessionValidationError::MissingActiveTab(self.active_tab));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionValidationError {
    NoTabs,
    MissingActiveTab(TabId),
    DuplicateTabId(TabId),
    DuplicatePaneId(PaneId),
    MissingActivePane { tab_id: TabId, pane_id: PaneId },
    MissingPaneInLayout { tab_id: TabId, pane_id: PaneId },
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtrm_core::SplitDirection;

    fn single_pane_layout_snapshot(pane_id: PaneId) -> LayoutSnapshot {
        LayoutTree::new(pane_id).to_snapshot()
    }

    fn two_pane_layout_snapshot(first: PaneId, second: PaneId) -> LayoutSnapshot {
        let mut layout = LayoutTree::new(first);
        layout.split_focused(SplitDirection::Vertical, second);
        layout.focus_pane(first).unwrap();
        layout.to_snapshot()
    }

    fn pane_snapshot(id: PaneId, cwd: &str) -> PaneSnapshot {
        PaneSnapshot {
            id,
            cwd: PathBuf::from(cwd),
        }
    }

    #[test]
    fn validates_single_tab_snapshot() {
        let snapshot = SessionSnapshot {
            tabs: vec![TabSnapshot {
                id: TabId::new(1),
                title: "main".to_owned(),
                layout: single_pane_layout_snapshot(PaneId::new(10)),
                panes: vec![pane_snapshot(PaneId::new(10), "/tmp")],
                active_pane: PaneId::new(10),
            }],
            active_tab: TabId::new(1),
        };

        assert_eq!(snapshot.validate(), Ok(()));
    }

    #[test]
    fn validates_multiple_tabs_snapshot() {
        let snapshot = SessionSnapshot {
            tabs: vec![
                TabSnapshot {
                    id: TabId::new(1),
                    title: "one".to_owned(),
                    layout: single_pane_layout_snapshot(PaneId::new(10)),
                    panes: vec![pane_snapshot(PaneId::new(10), "/tmp/one")],
                    active_pane: PaneId::new(10),
                },
                TabSnapshot {
                    id: TabId::new(2),
                    title: "two".to_owned(),
                    layout: two_pane_layout_snapshot(PaneId::new(20), PaneId::new(21)),
                    panes: vec![
                        pane_snapshot(PaneId::new(20), "/tmp/two-a"),
                        pane_snapshot(PaneId::new(21), "/tmp/two-b"),
                    ],
                    active_pane: PaneId::new(20),
                },
            ],
            active_tab: TabId::new(2),
        };

        assert_eq!(snapshot.validate(), Ok(()));
    }

    #[test]
    fn rejects_missing_active_tab() {
        let snapshot = SessionSnapshot {
            tabs: vec![TabSnapshot {
                id: TabId::new(1),
                title: "main".to_owned(),
                layout: single_pane_layout_snapshot(PaneId::new(10)),
                panes: vec![pane_snapshot(PaneId::new(10), "/tmp")],
                active_pane: PaneId::new(10),
            }],
            active_tab: TabId::new(2),
        };

        assert_eq!(
            snapshot.validate(),
            Err(SessionValidationError::MissingActiveTab(TabId::new(2)))
        );
    }

    #[test]
    fn rejects_duplicate_tab_id() {
        let snapshot = SessionSnapshot {
            tabs: vec![
                TabSnapshot {
                    id: TabId::new(1),
                    title: "one".to_owned(),
                    layout: single_pane_layout_snapshot(PaneId::new(10)),
                    panes: vec![pane_snapshot(PaneId::new(10), "/tmp/one")],
                    active_pane: PaneId::new(10),
                },
                TabSnapshot {
                    id: TabId::new(1),
                    title: "two".to_owned(),
                    layout: single_pane_layout_snapshot(PaneId::new(20)),
                    panes: vec![pane_snapshot(PaneId::new(20), "/tmp/two")],
                    active_pane: PaneId::new(20),
                },
            ],
            active_tab: TabId::new(1),
        };

        assert_eq!(
            snapshot.validate(),
            Err(SessionValidationError::DuplicateTabId(TabId::new(1)))
        );
    }

    #[test]
    fn rejects_duplicate_pane_id() {
        let snapshot = SessionSnapshot {
            tabs: vec![
                TabSnapshot {
                    id: TabId::new(1),
                    title: "one".to_owned(),
                    layout: single_pane_layout_snapshot(PaneId::new(10)),
                    panes: vec![pane_snapshot(PaneId::new(10), "/tmp/one")],
                    active_pane: PaneId::new(10),
                },
                TabSnapshot {
                    id: TabId::new(2),
                    title: "two".to_owned(),
                    layout: single_pane_layout_snapshot(PaneId::new(10)),
                    panes: vec![pane_snapshot(PaneId::new(10), "/tmp/two")],
                    active_pane: PaneId::new(10),
                },
            ],
            active_tab: TabId::new(1),
        };

        assert_eq!(
            snapshot.validate(),
            Err(SessionValidationError::DuplicatePaneId(PaneId::new(10)))
        );
    }

    #[test]
    fn rejects_missing_active_pane() {
        let snapshot = SessionSnapshot {
            tabs: vec![TabSnapshot {
                id: TabId::new(1),
                title: "main".to_owned(),
                layout: single_pane_layout_snapshot(PaneId::new(10)),
                panes: vec![pane_snapshot(PaneId::new(10), "/tmp")],
                active_pane: PaneId::new(11),
            }],
            active_tab: TabId::new(1),
        };

        assert_eq!(
            snapshot.validate(),
            Err(SessionValidationError::MissingActivePane {
                tab_id: TabId::new(1),
                pane_id: PaneId::new(11),
            })
        );
    }

    #[test]
    fn rejects_layout_pane_missing_from_pane_list() {
        let snapshot = SessionSnapshot {
            tabs: vec![TabSnapshot {
                id: TabId::new(1),
                title: "main".to_owned(),
                layout: two_pane_layout_snapshot(PaneId::new(10), PaneId::new(11)),
                panes: vec![pane_snapshot(PaneId::new(10), "/tmp")],
                active_pane: PaneId::new(10),
            }],
            active_tab: TabId::new(1),
        };

        assert_eq!(
            snapshot.validate(),
            Err(SessionValidationError::MissingPaneInLayout {
                tab_id: TabId::new(1),
                pane_id: PaneId::new(11),
            })
        );
    }

    #[test]
    fn serializes_and_deserializes_full_snapshot() {
        let snapshot = SessionSnapshot {
            tabs: vec![TabSnapshot {
                id: TabId::new(1),
                title: "main".to_owned(),
                layout: two_pane_layout_snapshot(PaneId::new(10), PaneId::new(11)),
                panes: vec![
                    pane_snapshot(PaneId::new(10), "/tmp/a"),
                    pane_snapshot(PaneId::new(11), "/tmp/b"),
                ],
                active_pane: PaneId::new(10),
            }],
            active_tab: TabId::new(1),
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        let restored: SessionSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(restored, snapshot);
        assert_eq!(restored.validate(), Ok(()));
    }
}
