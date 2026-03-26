//! Управление вкладками и живыми окнами.

use std::collections::BTreeMap;
use std::path::PathBuf;

use mtrm_core::{FocusMoveDirection, IdAllocator, PaneId, SplitDirection, TabId};
use mtrm_layout::{LayoutError, LayoutTree, Rect};
use mtrm_process::{ShellProcess, ShellProcessConfig};
use mtrm_session::{PaneSnapshot, SessionSnapshot, TabSnapshot};
use mtrm_terminal_screen::{ScreenLine, TerminalScreen};
use thiserror::Error;

const DEFAULT_TERMINAL_ROWS: u16 = 24;
const DEFAULT_TERMINAL_COLS: u16 = 80;
const DEFAULT_SCROLLBACK_LEN: usize = 1_000;

#[derive(Debug, Error)]
pub enum TabsError {
    #[error("tab not found: {0:?}")]
    TabNotFound(TabId),
    #[error("pane not found: {0:?}")]
    PaneNotFound(PaneId),
    #[error("layout error: {0:?}")]
    Layout(LayoutError),
    #[error("process error: {0}")]
    Process(String),
    #[error("cannot close last tab")]
    CannotCloseLastTab,
}

pub struct RuntimePane {
    pub id: PaneId,
    pub cwd: PathBuf,
}

pub struct RuntimeTab {
    pub id: TabId,
    pub title: String,
    pub layout: LayoutTree,
}

struct TabEntry {
    runtime: RuntimeTab,
    panes: BTreeMap<PaneId, PaneEntry>,
}

pub struct TabManager {
    tabs: Vec<TabEntry>,
    active_tab: usize,
    ids: IdAllocator,
}

impl TabManager {
    pub fn new(initial_shell: &ShellProcessConfig) -> Result<Self, TabsError> {
        let mut ids = IdAllocator::new();
        let tab_id = ids.next_tab_id();
        let pane_id = ids.next_pane_id();
        let process = spawn_shell(initial_shell, initial_shell.initial_cwd.clone())?;

        let tab = TabEntry {
            runtime: RuntimeTab {
                id: tab_id,
                title: format!("Tab {}", tab_id.get() + 1),
                layout: LayoutTree::new(pane_id),
            },
            panes: BTreeMap::from([(pane_id, process)]),
        };

        Ok(Self {
            tabs: vec![tab],
            active_tab: 0,
            ids,
        })
    }

    pub fn from_snapshot(
        snapshot: SessionSnapshot,
        shell: &ShellProcessConfig,
    ) -> Result<Self, TabsError> {
        snapshot
            .validate()
            .map_err(|error| TabsError::Process(format!("invalid snapshot: {error:?}")))?;

        let mut tabs = Vec::with_capacity(snapshot.tabs.len());
        let mut ids = IdAllocator::new();
        let mut active_tab_index = 0;
        let mut max_tab = 0_u64;
        let mut max_pane = 0_u64;

        for (index, tab_snapshot) in snapshot.tabs.into_iter().enumerate() {
            let mut processes = BTreeMap::new();
            for pane in &tab_snapshot.panes {
                processes.insert(pane.id, spawn_shell(shell, pane.cwd.clone())?);
                max_pane = max_pane.max(pane.id.get());
            }

            let mut layout =
                LayoutTree::from_snapshot(tab_snapshot.layout).map_err(TabsError::Layout)?;
            layout
                .focus_pane(tab_snapshot.active_pane)
                .map_err(TabsError::Layout)?;

            if tab_snapshot.id == snapshot.active_tab {
                active_tab_index = index;
            }
            max_tab = max_tab.max(tab_snapshot.id.get());

            tabs.push(TabEntry {
                runtime: RuntimeTab {
                    id: tab_snapshot.id,
                    title: tab_snapshot.title,
                    layout,
                },
                panes: processes,
            });
        }

        seed_allocator(&mut ids, max_tab + 1, max_pane + 1);

        Ok(Self {
            tabs,
            active_tab: active_tab_index,
            ids,
        })
    }

    pub fn active_tab_id(&self) -> TabId {
        self.active_tab().runtime.id
    }

    pub fn active_pane_id(&self) -> PaneId {
        self.active_tab().runtime.layout.focused_pane()
    }

    pub fn tab_ids(&self) -> Vec<TabId> {
        self.tabs.iter().map(|tab| tab.runtime.id).collect()
    }

    pub fn new_tab(&mut self, shell: &ShellProcessConfig) -> Result<TabId, TabsError> {
        let tab_id = self.ids.next_tab_id();
        let pane_id = self.ids.next_pane_id();
        let process = spawn_shell(shell, shell.initial_cwd.clone())?;

        self.tabs.push(TabEntry {
            runtime: RuntimeTab {
                id: tab_id,
                title: format!("Tab {}", tab_id.get() + 1),
                layout: LayoutTree::new(pane_id),
            },
            panes: BTreeMap::from([(pane_id, process)]),
        });
        self.active_tab = self.tabs.len() - 1;
        Ok(tab_id)
    }

    pub fn close_active_tab(&mut self) -> Result<TabId, TabsError> {
        if self.tabs.len() == 1 {
            return Err(TabsError::CannotCloseLastTab);
        }

        let removed = self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        Ok(removed.runtime.id)
    }

    pub fn activate_tab(&mut self, tab_id: TabId) -> Result<(), TabsError> {
        let index = self
            .tabs
            .iter()
            .position(|tab| tab.runtime.id == tab_id)
            .ok_or(TabsError::TabNotFound(tab_id))?;
        self.active_tab = index;
        Ok(())
    }

    pub fn split_active_pane(
        &mut self,
        direction: SplitDirection,
        shell: &ShellProcessConfig,
    ) -> Result<PaneId, TabsError> {
        let cwd = self.active_pane_cwd()?;
        let new_pane_id = self.ids.next_pane_id();
        let process = spawn_shell(shell, cwd)?;
        let tab = self.active_tab_mut();
        tab.runtime.layout.split_focused(direction, new_pane_id);
        tab.panes.insert(new_pane_id, process);
        Ok(new_pane_id)
    }

    pub fn close_active_pane(&mut self) -> Result<PaneId, TabsError> {
        let pane_id = self.active_pane_id();
        let tab = self.active_tab_mut();
        let closed = tab
            .runtime
            .layout
            .close_focused()
            .map_err(TabsError::Layout)?;
        let _ = tab
            .panes
            .remove(&closed)
            .ok_or(TabsError::PaneNotFound(closed))?;
        debug_assert_eq!(closed, pane_id);
        Ok(closed)
    }

    pub fn move_focus(&mut self, direction: FocusMoveDirection) -> Result<PaneId, TabsError> {
        self.active_tab_mut()
            .runtime
            .layout
            .move_focus(direction)
            .map_err(TabsError::Layout)
    }

    pub fn write_to_active_pane(&mut self, bytes: &[u8]) -> Result<(), TabsError> {
        let pane_id = self.active_pane_id();
        let pane = self
            .active_tab_mut()
            .panes
            .get_mut(&pane_id)
            .ok_or(TabsError::PaneNotFound(pane_id))?;
        pane.screen.set_scrollback(0);
        pane.process.write_all(bytes).map_err(process_error)
    }

    pub fn read_from_active_pane(&mut self) -> Result<Vec<u8>, TabsError> {
        self.active_process_mut()?.try_read().map_err(process_error)
    }

    pub fn read_from_all_panes(&mut self) -> Result<Vec<(PaneId, Vec<u8>)>, TabsError> {
        let mut output = Vec::new();

        for tab in &mut self.tabs {
            for (pane_id, pane) in &mut tab.panes {
                let bytes = pane.process.try_read().map_err(process_error)?;
                if !bytes.is_empty() {
                    output.push((*pane_id, bytes));
                }
            }
        }

        Ok(output)
    }

    pub fn send_interrupt_to_active_pane(&mut self) -> Result<(), TabsError> {
        self.active_process_mut()?
            .send_interrupt()
            .map_err(process_error)
    }

    pub fn active_pane_cwd(&self) -> Result<PathBuf, TabsError> {
        let pane_id = self.active_pane_id();
        self.active_tab()
            .panes
            .get(&pane_id)
            .ok_or(TabsError::PaneNotFound(pane_id))?
            .process
            .current_dir()
            .map_err(process_error)
    }

    pub fn resize_active_tab(&mut self, area: Rect) -> Result<(), TabsError> {
        let placements = self.active_tab().runtime.layout.placements(area);
        let tab = self.active_tab_mut();

        for placement in placements {
            let pane = tab
                .panes
                .get_mut(&placement.pane_id)
                .ok_or(TabsError::PaneNotFound(placement.pane_id))?;
            let cols = placement.rect.width.saturating_sub(2).max(1);
            let rows = placement.rect.height.saturating_sub(2).max(1);
            let scrollback = pane.screen.scrollback();
            pane.process.resize(cols, rows).map_err(process_error)?;
            pane.screen.resize(rows, cols);
            pane.screen.set_scrollback(scrollback);
        }
        Ok(())
    }

    pub fn scroll_active_pane_up_lines(&mut self, lines: u16) -> Result<(), TabsError> {
        let pane_id = self.active_pane_id();
        let pane = self
            .active_tab_mut()
            .panes
            .get_mut(&pane_id)
            .ok_or(TabsError::PaneNotFound(pane_id))?;
        let next = pane.screen.scrollback().saturating_add(lines as usize);
        pane.screen.set_scrollback(next);
        Ok(())
    }

    pub fn scroll_active_pane_down_lines(&mut self, lines: u16) -> Result<(), TabsError> {
        let pane_id = self.active_pane_id();
        let pane = self
            .active_tab_mut()
            .panes
            .get_mut(&pane_id)
            .ok_or(TabsError::PaneNotFound(pane_id))?;
        let next = pane.screen.scrollback().saturating_sub(lines as usize);
        pane.screen.set_scrollback(next);
        Ok(())
    }

    pub fn scroll_active_pane_up_pages(&mut self, pages: u16) -> Result<(), TabsError> {
        let page_size = self.active_visible_rows()?;
        self.scroll_active_pane_up_lines(page_size.saturating_mul(pages))
    }

    pub fn scroll_active_pane_down_pages(&mut self, pages: u16) -> Result<(), TabsError> {
        let page_size = self.active_visible_rows()?;
        self.scroll_active_pane_down_lines(page_size.saturating_mul(pages))
    }

    pub fn scroll_active_pane_to_bottom(&mut self) -> Result<(), TabsError> {
        let pane_id = self.active_pane_id();
        let pane = self
            .active_tab_mut()
            .panes
            .get_mut(&pane_id)
            .ok_or(TabsError::PaneNotFound(pane_id))?;
        pane.screen.set_scrollback(0);
        Ok(())
    }

    pub fn placements(&self, area: Rect) -> Result<Vec<(PaneId, Rect, bool)>, TabsError> {
        Ok(self
            .active_tab()
            .runtime
            .layout
            .placements(area)
            .into_iter()
            .map(|placement| (placement.pane_id, placement.rect, placement.focused))
            .collect())
    }

    pub fn snapshot(&self) -> Result<SessionSnapshot, TabsError> {
        let mut tabs = Vec::with_capacity(self.tabs.len());

        for tab in &self.tabs {
            let active_pane = tab.runtime.layout.focused_pane();
            let mut panes = Vec::with_capacity(tab.panes.len());

            for pane_id in tab.runtime.layout.pane_ids() {
                let process = tab
                    .panes
                    .get(&pane_id)
                    .ok_or(TabsError::PaneNotFound(pane_id))?;
                let cwd = process.process.current_dir().map_err(process_error)?;
                panes.push(PaneSnapshot { id: pane_id, cwd });
            }

            tabs.push(TabSnapshot {
                id: tab.runtime.id,
                title: tab.runtime.title.clone(),
                layout: tab.runtime.layout.to_snapshot(),
                panes,
                active_pane,
            });
        }

        Ok(SessionSnapshot {
            tabs,
            active_tab: self.active_tab_id(),
        })
    }

    fn active_tab(&self) -> &TabEntry {
        &self.tabs[self.active_tab]
    }

    fn active_tab_mut(&mut self) -> &mut TabEntry {
        &mut self.tabs[self.active_tab]
    }

    fn active_process_mut(&mut self) -> Result<&mut ShellProcess, TabsError> {
        let pane_id = self.active_pane_id();
        self.active_tab_mut()
            .panes
            .get_mut(&pane_id)
            .map(|pane| &mut pane.process)
            .ok_or(TabsError::PaneNotFound(pane_id))
    }

    pub fn refresh_all_panes(&mut self) -> Result<bool, TabsError> {
        let mut changed = false;

        for tab in &mut self.tabs {
            for pane in tab.panes.values_mut() {
                let bytes = pane.process.try_read().map_err(process_error)?;
                if bytes.is_empty() {
                    continue;
                }
                pane.screen.process_bytes(&bytes);
                changed = true;
            }
        }

        Ok(changed)
    }

    pub fn pane_text(&self, pane_id: PaneId) -> Result<String, TabsError> {
        self.find_pane(pane_id)
            .map(|pane| pane.screen.visible_rows().join("\n"))
            .ok_or(TabsError::PaneNotFound(pane_id))
    }

    pub fn active_pane_text(&self) -> Result<String, TabsError> {
        self.pane_text(self.active_pane_id())
    }

    pub fn pane_lines(&self, pane_id: PaneId) -> Result<Vec<ScreenLine>, TabsError> {
        self.find_pane(pane_id)
            .map(|pane| pane.screen.visible_lines())
            .ok_or(TabsError::PaneNotFound(pane_id))
    }

    pub fn pane_cursor(&self, pane_id: PaneId) -> Result<Option<(u16, u16)>, TabsError> {
        self.find_pane(pane_id)
            .map(|pane| {
                if pane.screen.scrollback() == 0 {
                    Some(pane.screen.cursor_position())
                } else {
                    None
                }
            })
            .ok_or(TabsError::PaneNotFound(pane_id))
    }

    fn active_visible_rows(&self) -> Result<u16, TabsError> {
        let pane_id = self.active_pane_id();
        self.find_pane(pane_id)
            .map(|pane| pane.screen.size().0)
            .ok_or(TabsError::PaneNotFound(pane_id))
    }

    fn find_pane(&self, pane_id: PaneId) -> Option<&PaneEntry> {
        self.tabs.iter().find_map(|tab| tab.panes.get(&pane_id))
    }

    #[cfg(test)]
    fn pane_has_empty_screen(&self, pane_id: PaneId) -> Result<bool, TabsError> {
        self.find_pane(pane_id)
            .map(|pane| pane.screen.text_contents().trim().is_empty())
            .ok_or(TabsError::PaneNotFound(pane_id))
    }
}

struct PaneEntry {
    process: ShellProcess,
    screen: TerminalScreen,
}

fn spawn_shell(shell: &ShellProcessConfig, cwd: PathBuf) -> Result<PaneEntry, TabsError> {
    let config = ShellProcessConfig {
        program: shell.program.clone(),
        args: shell.args.clone(),
        initial_cwd: cwd,
    };
    let process = ShellProcess::spawn(config).map_err(process_error)?;
    let screen = TerminalScreen::new(
        DEFAULT_TERMINAL_ROWS,
        DEFAULT_TERMINAL_COLS,
        DEFAULT_SCROLLBACK_LEN,
    );
    Ok(PaneEntry { process, screen })
}

fn process_error(error: impl ToString) -> TabsError {
    TabsError::Process(error.to_string())
}

fn seed_allocator(ids: &mut IdAllocator, next_tab: u64, next_pane: u64) {
    for _ in 0..next_tab {
        let _ = ids.next_tab_id();
    }
    for _ in 0..next_pane {
        let _ = ids.next_pane_id();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread;
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    fn shell_config(initial_cwd: PathBuf) -> ShellProcessConfig {
        ShellProcessConfig {
            program: PathBuf::from("/bin/sh"),
            args: vec![],
            initial_cwd,
        }
    }

    fn wait_until<F>(timeout: Duration, mut predicate: F) -> bool
    where
        F: FnMut() -> bool,
    {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if predicate() {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        false
    }

    fn read_until_contains(
        manager: &mut TabManager,
        needle: &str,
        timeout: Duration,
    ) -> Result<String, TabsError> {
        let deadline = Instant::now() + timeout;
        let mut combined = String::new();

        while Instant::now() < deadline {
            let chunk = manager.read_from_active_pane()?;
            if !chunk.is_empty() {
                combined.push_str(&String::from_utf8_lossy(&chunk));
                if combined.contains(needle) {
                    return Ok(combined);
                }
            }
            thread::sleep(Duration::from_millis(20));
        }

        Err(TabsError::Process(format!(
            "timed out waiting for output containing {needle:?}; got {combined:?}"
        )))
    }

    #[test]
    fn new_creates_single_tab_with_single_pane() {
        let temp = tempdir().unwrap();
        let manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

        assert_eq!(manager.tab_ids().len(), 1);
        assert_eq!(manager.active_tab_id(), TabId::new(0));
        assert_eq!(manager.active_pane_id(), PaneId::new(0));
    }

    #[test]
    fn new_tab_adds_tab_and_activates_it() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

        let tab_id = manager
            .new_tab(&shell_config(temp.path().to_path_buf()))
            .unwrap();

        assert_eq!(manager.tab_ids(), vec![TabId::new(0), tab_id]);
        assert_eq!(manager.active_tab_id(), tab_id);
    }

    #[test]
    fn activate_tab_switches_active_tab() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
        let tab_id = manager
            .new_tab(&shell_config(temp.path().to_path_buf()))
            .unwrap();

        manager.activate_tab(TabId::new(0)).unwrap();
        assert_eq!(manager.active_tab_id(), TabId::new(0));

        manager.activate_tab(tab_id).unwrap();
        assert_eq!(manager.active_tab_id(), tab_id);
    }

    #[test]
    fn split_active_pane_adds_new_pane() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

        let pane_id = manager
            .split_active_pane(
                SplitDirection::Vertical,
                &shell_config(temp.path().to_path_buf()),
            )
            .unwrap();

        let placements = manager
            .placements(Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 30,
            })
            .unwrap();
        assert_eq!(pane_id, PaneId::new(1));
        assert_eq!(placements.len(), 2);
        assert!(manager.pane_has_empty_screen(pane_id).unwrap());
    }

    #[test]
    fn close_active_pane_removes_it_and_keeps_tab_alive() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
        manager
            .split_active_pane(
                SplitDirection::Vertical,
                &shell_config(temp.path().to_path_buf()),
            )
            .unwrap();

        let closed = manager.close_active_pane().unwrap();

        assert_eq!(closed, PaneId::new(1));
        assert_eq!(manager.active_pane_id(), PaneId::new(0));
        assert_eq!(
            manager
                .placements(Rect {
                    x: 0,
                    y: 0,
                    width: 100,
                    height: 30,
                })
                .unwrap()
                .len(),
            1
        );
        assert!(matches!(
            manager.pane_text(closed),
            Err(TabsError::PaneNotFound(_))
        ));
    }

    #[test]
    fn snapshot_reflects_current_state() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
        let next_dir = temp.path().join("next");
        fs::create_dir(&next_dir).unwrap();
        manager
            .write_to_active_pane(format!("cd '{}'\n", next_dir.display()).as_bytes())
            .unwrap();
        let changed = wait_until(Duration::from_secs(2), || {
            manager
                .active_pane_cwd()
                .map(|cwd| cwd == next_dir)
                .unwrap_or(false)
        });
        assert!(changed);

        let snapshot = manager.snapshot().unwrap();

        assert_eq!(snapshot.active_tab, TabId::new(0));
        assert_eq!(snapshot.tabs.len(), 1);
        assert_eq!(snapshot.tabs[0].panes[0].cwd, next_dir);
    }

    #[test]
    fn from_snapshot_restores_tabs_and_panes() {
        let temp = tempdir().unwrap();
        let dir_a = temp.path().join("a");
        let dir_b = temp.path().join("b");
        fs::create_dir(&dir_a).unwrap();
        fs::create_dir(&dir_b).unwrap();

        let snapshot = SessionSnapshot {
            tabs: vec![
                TabSnapshot {
                    id: TabId::new(5),
                    title: "first".to_owned(),
                    layout: LayoutTree::new(PaneId::new(10)).to_snapshot(),
                    panes: vec![PaneSnapshot {
                        id: PaneId::new(10),
                        cwd: dir_a.clone(),
                    }],
                    active_pane: PaneId::new(10),
                },
                TabSnapshot {
                    id: TabId::new(6),
                    title: "second".to_owned(),
                    layout: {
                        let mut layout = LayoutTree::new(PaneId::new(20));
                        layout.split_focused(SplitDirection::Vertical, PaneId::new(21));
                        layout.focus_pane(PaneId::new(21)).unwrap();
                        layout.to_snapshot()
                    },
                    panes: vec![
                        PaneSnapshot {
                            id: PaneId::new(20),
                            cwd: dir_a.clone(),
                        },
                        PaneSnapshot {
                            id: PaneId::new(21),
                            cwd: dir_b.clone(),
                        },
                    ],
                    active_pane: PaneId::new(21),
                },
            ],
            active_tab: TabId::new(6),
        };

        let manager =
            TabManager::from_snapshot(snapshot, &shell_config(temp.path().to_path_buf())).unwrap();

        assert_eq!(manager.tab_ids(), vec![TabId::new(5), TabId::new(6)]);
        assert_eq!(manager.active_tab_id(), TabId::new(6));
        assert_eq!(manager.active_pane_id(), PaneId::new(21));
    }

    #[test]
    fn send_interrupt_is_forwarded_to_active_process() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

        manager.write_to_active_pane(b"sleep 5\n").unwrap();
        thread::sleep(Duration::from_millis(150));
        manager.send_interrupt_to_active_pane().unwrap();
        manager
            .write_to_active_pane(b"printf '__TABS_INTERRUPT__\\n'\n")
            .unwrap();

        let output =
            read_until_contains(&mut manager, "__TABS_INTERRUPT__", Duration::from_secs(3))
                .unwrap();
        assert!(output.contains("__TABS_INTERRUPT__"));
    }

    #[test]
    fn active_pane_cwd_returns_live_process_directory() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
        let next_dir = temp.path().join("cwd-next");
        fs::create_dir(&next_dir).unwrap();

        manager
            .write_to_active_pane(format!("cd '{}'\n", next_dir.display()).as_bytes())
            .unwrap();

        let changed = wait_until(Duration::from_secs(2), || {
            manager
                .active_pane_cwd()
                .map(|cwd| cwd == next_dir)
                .unwrap_or(false)
        });
        assert!(changed);
    }

    #[test]
    fn refresh_all_panes_preserves_output_of_inactive_pane() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
        manager
            .split_active_pane(
                SplitDirection::Vertical,
                &shell_config(temp.path().to_path_buf()),
            )
            .unwrap();
        let right_pane = manager.active_pane_id();

        manager
            .write_to_active_pane(b"printf '__INACTIVE_SCREEN__\\n'\n")
            .unwrap();
        manager.move_focus(FocusMoveDirection::Left).unwrap();

        let captured = wait_until(Duration::from_secs(2), || {
            manager.refresh_all_panes().unwrap_or(false)
                && manager
                    .pane_text(right_pane)
                    .map(|text| text.contains("__INACTIVE_SCREEN__"))
                    .unwrap_or(false)
        });

        assert!(
            captured,
            "inactive pane output must stay in pane screen state"
        );
    }

    #[test]
    fn scrolling_active_pane_changes_visible_text() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
        manager
            .resize_active_tab(Rect {
                x: 0,
                y: 0,
                width: 20,
                height: 6,
            })
            .unwrap();
        manager
            .write_to_active_pane(
                b"i=1; while [ \"$i\" -le 20 ]; do printf 'line%s\\n' \"$i\"; i=$((i+1)); done\n",
            )
            .unwrap();

        let loaded = wait_until(Duration::from_secs(2), || {
            manager.refresh_all_panes().unwrap_or(false)
                && manager
                    .active_pane_text()
                    .map(|text| text.contains("line20"))
                    .unwrap_or(false)
        });
        assert!(loaded);

        let before = manager.active_pane_text().unwrap();
        assert!(before.contains("line20"));
        assert!(manager.scroll_active_pane_up_lines(2).is_ok());
        let after = manager.active_pane_text().unwrap();

        assert_ne!(before, after);
    }

    #[test]
    fn input_returns_scrolled_pane_to_bottom() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
        manager
            .resize_active_tab(Rect {
                x: 0,
                y: 0,
                width: 20,
                height: 6,
            })
            .unwrap();
        manager
            .write_to_active_pane(
                b"i=1; while [ \"$i\" -le 20 ]; do printf 'line%s\\n' \"$i\"; i=$((i+1)); done\n",
            )
            .unwrap();

        let loaded = wait_until(Duration::from_secs(2), || {
            manager.refresh_all_panes().unwrap_or(false)
                && manager
                    .active_pane_text()
                    .map(|text| text.contains("line20"))
                    .unwrap_or(false)
        });
        assert!(loaded);

        manager.scroll_active_pane_up_lines(3).unwrap();
        assert!(manager.active_pane_text().unwrap().contains("line17"));
        manager
            .write_to_active_pane(b"printf '__BOTTOM__\\n'\n")
            .unwrap();

        let reset = wait_until(Duration::from_secs(2), || {
            manager.refresh_all_panes().unwrap_or(false)
                && manager
                    .active_pane_text()
                    .map(|text| text.contains("__BOTTOM__"))
                    .unwrap_or(false)
        });
        assert!(reset);
    }
}
