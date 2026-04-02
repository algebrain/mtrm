//! Управление вкладками и живыми окнами.

use std::collections::BTreeMap;
use std::path::PathBuf;

use mtrm_core::{FocusMoveDirection, IdAllocator, PaneId, ResizeDirection, SplitDirection, TabId};
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
    pub title: String,
}

pub struct RuntimeTab {
    pub id: TabId,
    pub title: String,
    pub layout: LayoutTree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTabSummary {
    pub id: TabId,
    pub title: String,
    pub active: bool,
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
        let process = spawn_shell(
            initial_shell,
            initial_shell.initial_cwd.clone(),
            default_pane_title_for(pane_id),
        )?;

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
                let title = if pane.title.is_empty() {
                    default_pane_title_for(pane.id)
                } else {
                    pane.title.clone()
                };
                processes.insert(pane.id, spawn_shell(shell, pane.cwd.clone(), title)?);
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

    pub fn active_tab_title(&self) -> &str {
        &self.active_tab().runtime.title
    }

    pub fn active_pane_id(&self) -> PaneId {
        self.active_tab().runtime.layout.focused_pane()
    }

    pub fn active_pane_title(&self) -> Result<&str, TabsError> {
        self.pane_title(self.active_pane_id())
    }

    pub fn tab_ids(&self) -> Vec<TabId> {
        self.tabs.iter().map(|tab| tab.runtime.id).collect()
    }

    pub fn tab_summaries(&self) -> Vec<RuntimeTabSummary> {
        let active = self.active_tab_id();
        self.tabs
            .iter()
            .map(|tab| RuntimeTabSummary {
                id: tab.runtime.id,
                title: tab.runtime.title.clone(),
                active: tab.runtime.id == active,
            })
            .collect()
    }

    pub fn new_tab(&mut self, shell: &ShellProcessConfig) -> Result<TabId, TabsError> {
        let tab_id = self.ids.next_tab_id();
        let pane_id = self.ids.next_pane_id();
        let process = spawn_shell(
            shell,
            shell.initial_cwd.clone(),
            default_pane_title_for(pane_id),
        )?;

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

    pub fn rename_tab(&mut self, tab_id: TabId, title: String) -> Result<(), TabsError> {
        let tab = self
            .tabs
            .iter_mut()
            .find(|tab| tab.runtime.id == tab_id)
            .ok_or(TabsError::TabNotFound(tab_id))?;
        tab.runtime.title = title;
        Ok(())
    }

    pub fn rename_pane(&mut self, pane_id: PaneId, title: String) -> Result<(), TabsError> {
        let pane = self
            .tabs
            .iter_mut()
            .find_map(|tab| tab.panes.get_mut(&pane_id))
            .ok_or(TabsError::PaneNotFound(pane_id))?;
        pane.title = title;
        Ok(())
    }

    pub fn pane_title(&self, pane_id: PaneId) -> Result<&str, TabsError> {
        self.find_pane(pane_id)
            .map(|pane| pane.title.as_str())
            .ok_or(TabsError::PaneNotFound(pane_id))
    }

    pub fn split_active_pane(
        &mut self,
        direction: SplitDirection,
        shell: &ShellProcessConfig,
    ) -> Result<PaneId, TabsError> {
        let cwd = self.active_pane_cwd()?;
        let new_pane_id = self.ids.next_pane_id();
        let process = spawn_shell(shell, cwd, default_pane_title_for(new_pane_id))?;
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

    pub fn resize_active_pane(
        &mut self,
        direction: ResizeDirection,
        area: Rect,
    ) -> Result<bool, TabsError> {
        self.active_tab_mut()
            .runtime
            .layout
            .resize_focused(direction, area)
            .map_err(TabsError::Layout)
    }

    pub fn focus_pane(&mut self, pane_id: PaneId) -> Result<(), TabsError> {
        let tab = self.active_tab_mut();
        tab.runtime.layout.focus_pane(pane_id).map_err(TabsError::Layout)
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
                panes.push(PaneSnapshot {
                    id: pane_id,
                    cwd,
                    title: process.title.clone(),
                });
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

    pub fn active_pane_is_scrolled_back(&self) -> Result<bool, TabsError> {
        let pane_id = self.active_pane_id();
        self.find_pane(pane_id)
            .map(|pane| pane.screen.scrollback() > 0)
            .ok_or(TabsError::PaneNotFound(pane_id))
    }

    pub fn pane_selection_text(
        &self,
        pane_id: PaneId,
        start: (u16, u16),
        end: (u16, u16),
    ) -> Result<String, TabsError> {
        let lines = self.pane_lines(pane_id)?;
        Ok(selection_text_from_lines(&lines, start, end))
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
    title: String,
}

fn spawn_shell(shell: &ShellProcessConfig, cwd: PathBuf, title: String) -> Result<PaneEntry, TabsError> {
    let config = ShellProcessConfig {
        program: shell.program.clone(),
        args: shell.args.clone(),
        initial_cwd: cwd,
        debug_log_path: shell.debug_log_path.clone(),
    };
    let process = ShellProcess::spawn(config).map_err(process_error)?;
    let screen = TerminalScreen::new(
        DEFAULT_TERMINAL_ROWS,
        DEFAULT_TERMINAL_COLS,
        DEFAULT_SCROLLBACK_LEN,
    );
    Ok(PaneEntry {
        process,
        screen,
        title,
    })
}

fn default_pane_title_for(pane_id: PaneId) -> String {
    format!("pane-{}", pane_id.get())
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

fn normalize_selection(start: (u16, u16), end: (u16, u16)) -> ((u16, u16), (u16, u16)) {
    if start <= end {
        (start, end)
    } else {
        (end, start)
    }
}

fn selection_text_from_lines(lines: &[ScreenLine], start: (u16, u16), end: (u16, u16)) -> String {
    let ((start_row, start_col), (end_row, end_col)) = normalize_selection(start, end);
    let mut selected_lines = Vec::new();

    for row in start_row..=end_row {
        let Some(line) = lines.get(row as usize) else {
            break;
        };

        let row_start = if row == start_row { start_col } else { 0 };
        if line.cells.is_empty() || row_start as usize >= line.cells.len() {
            selected_lines.push(String::new());
            continue;
        }

        let row_end = if row == end_row {
            end_col
        } else {
            line.cells.len().saturating_sub(1) as u16
        };
        let bounded_end = row_end.min(line.cells.len().saturating_sub(1) as u16);

        let mut text = String::new();
        for col in row_start..=bounded_end {
            let cell = &line.cells[col as usize];
            if cell.is_wide_continuation {
                continue;
            }
            if cell.has_contents {
                text.push_str(&cell.text);
            } else {
                text.push(' ');
            }
        }
        selected_lines.push(text.trim_end_matches(' ').to_owned());
    }

    selected_lines.join("\n")
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
            debug_log_path: None,
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

    fn with_env_var_removed<T>(name: &str, f: impl FnOnce() -> T) -> T {
        let previous = std::env::var_os(name);
        unsafe {
            std::env::remove_var(name);
        }
        let result = f();
        if let Some(previous) = previous {
            unsafe {
                std::env::set_var(name, previous);
            }
        }
        result
    }

    fn find_visible_text_position(
        manager: &TabManager,
        pane_id: PaneId,
        needle: &str,
    ) -> (u16, u16) {
        let text = manager.pane_text(pane_id).unwrap();
        for (row, line) in text.split('\n').enumerate() {
            if let Some(col) = line.find(needle) {
                return (row as u16, col as u16);
            }
        }
        panic!("could not find {needle:?} in pane text: {text:?}");
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
    fn resize_active_pane_changes_layout_by_one_cell() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
        manager
            .split_active_pane(
                SplitDirection::Vertical,
                &shell_config(temp.path().to_path_buf()),
            )
            .unwrap();
        manager.focus_pane(PaneId::new(0)).unwrap();

        let area = Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 12,
        };
        let before = manager.placements(area).unwrap();

        let changed = manager
            .resize_active_pane(ResizeDirection::Right, area)
            .unwrap();
        let after = manager.placements(area).unwrap();

        assert!(changed);
        assert_eq!(after[0].1.width, before[0].1.width + 1);
        assert_eq!(after[1].1.width + 1, before[1].1.width);
        assert_eq!(manager.active_pane_id(), PaneId::new(0));
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
        assert_eq!(snapshot.tabs[0].panes[0].title, "pane-0");
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
                        title: "left".to_owned(),
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
                            title: "".to_owned(),
                        },
                        PaneSnapshot {
                            id: PaneId::new(21),
                            cwd: dir_b.clone(),
                            title: "right".to_owned(),
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
        assert_eq!(manager.pane_title(PaneId::new(10)).unwrap(), "left");
        assert_eq!(manager.pane_title(PaneId::new(20)).unwrap(), "pane-20");
        assert_eq!(manager.pane_title(PaneId::new(21)).unwrap(), "right");
    }

    #[test]
    fn rename_tab_updates_runtime_title_and_snapshot() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

        manager
            .rename_tab(TabId::new(0), "renamed".to_owned())
            .unwrap();

        assert_eq!(manager.active_tab_title(), "renamed");
        let snapshot = manager.snapshot().unwrap();
        assert_eq!(snapshot.tabs[0].title, "renamed");
    }

    #[test]
    fn active_pane_title_comes_from_runtime_data() {
        let temp = tempdir().unwrap();
        let manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

        assert_eq!(manager.active_pane_title().unwrap(), "pane-0");
    }

    #[test]
    fn rename_pane_updates_runtime_title_and_snapshot() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

        manager
            .rename_pane(PaneId::new(0), "editor".to_owned())
            .unwrap();

        assert_eq!(manager.active_pane_title().unwrap(), "editor");
        let snapshot = manager.snapshot().unwrap();
        assert_eq!(snapshot.tabs[0].panes[0].title, "editor");
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

    #[test]
    fn active_pane_screen_tracks_alternate_mode_for_fullscreen_sequences() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
        let pane_id = manager.active_pane_id();

        {
            let pane = manager.active_tab_mut().panes.get_mut(&pane_id).unwrap();
            pane.screen.process_bytes(b"shell$ prompt");
            pane.screen
                .process_bytes(b"\x1b[?1049h\x1b[2J\x1b[Hcodex frame");
        }

        let pane = manager.find_pane(pane_id).unwrap();
        assert_eq!(
            pane.screen.screen_mode(),
            mtrm_terminal_screen::ScreenMode::Alternate
        );
        assert!(pane.screen.visible_rows()[0].contains("codex frame"));

        {
            let pane = manager.active_tab_mut().panes.get_mut(&pane_id).unwrap();
            pane.screen.process_bytes(b"\x1b[?1049l");
        }

        let pane = manager.find_pane(pane_id).unwrap();
        assert_eq!(
            pane.screen.screen_mode(),
            mtrm_terminal_screen::ScreenMode::Normal
        );
        assert!(pane.screen.visible_rows()[0].contains("shell$ prompt"));
    }

    #[test]
    fn scrolling_fullscreen_pane_uses_alternate_snapshot_history() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
        let pane_id = manager.active_pane_id();

        {
            let pane = manager.active_tab_mut().panes.get_mut(&pane_id).unwrap();
            pane.screen.process_bytes(b"\x1b[?1049h");
            pane.screen.process_bytes(b"\x1b[2J\x1b[Hframe1");
            pane.screen.process_bytes(b"\x1b[2J\x1b[Hframe2");
            pane.screen.process_bytes(b"\x1b[2J\x1b[Hframe3");
        }

        let live = manager.active_pane_text().unwrap();
        assert!(live.contains("frame3"));

        manager.scroll_active_pane_up_lines(1).unwrap();
        let previous = manager.active_pane_text().unwrap();
        assert!(previous.contains("frame2"));

        manager.scroll_active_pane_to_bottom().unwrap();
        let bottom = manager.active_pane_text().unwrap();
        assert!(bottom.contains("frame3"));
    }

    #[test]
    fn scrolling_normal_screen_decstbm_history_shows_previous_frame_instead_of_mixed_rows() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
        let pane_id = manager.active_pane_id();

        let frame = |frame_label: &str, footer_label: &str| {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(b"\x1b[2J\x1b[H");
            bytes.extend_from_slice(b"hist1\r\nhist2\r\nhist3\r\nhist4\r\n");
            bytes.extend_from_slice(footer_label.as_bytes());
            bytes.extend_from_slice(b"\x1b[1;4r");
            bytes.extend_from_slice(b"\x1b[4;1H\r\n");
            bytes.extend_from_slice(frame_label.as_bytes());
            bytes.extend_from_slice(b"\x1b[r");
            bytes.extend_from_slice(b"\x1b[6;1H");
            bytes.extend_from_slice(footer_label.as_bytes());
            bytes
        };

        {
            let pane = manager.active_tab_mut().panes.get_mut(&pane_id).unwrap();
            pane.screen.process_bytes(&frame("frame1", "footer1"));
            pane.screen.process_bytes(&frame("frame2", "footer2"));
            pane.screen.process_bytes(&frame("frame3", "footer3"));
        }

        let live = manager.active_pane_text().unwrap();
        assert!(live.contains("frame3"));
        assert!(live.contains("footer3"));

        manager.scroll_active_pane_up_lines(1).unwrap();
        let previous = manager.active_pane_text().unwrap();
        assert!(previous.contains("frame2"), "previous text:\n{previous}");
        assert!(previous.contains("footer2"), "previous text:\n{previous}");
        assert!(!previous.contains("footer3"), "previous text:\n{previous}");
    }

    #[test]
    fn active_pane_shell_receives_truecolor_hint_env() {
        let temp = tempdir().unwrap();
        let has_truecolor_hint = with_env_var_removed("COLORTERM", || {
            let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

            manager
                .write_to_active_pane(b"printf '__COLORTERM__%s\\n' \"${COLORTERM:-missing}\"\n")
                .unwrap();

            wait_until(Duration::from_secs(3), || {
                manager.refresh_all_panes().unwrap_or(false)
                    && manager
                        .active_pane_text()
                        .map(|text| text.contains("__COLORTERM__truecolor"))
                        .unwrap_or(false)
            })
        });

        assert!(
            has_truecolor_hint,
            "interactive apps inside mtrm should receive COLORTERM=truecolor so they can enable richer terminal styling"
        );
    }

    #[test]
    fn active_pane_shell_receives_terminal_program_identity() {
        let temp = tempdir().unwrap();
        let has_program_identity = with_env_var_removed("TERM_PROGRAM", || {
            let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

            manager
                .write_to_active_pane(
                    b"printf '__TERM_PROGRAM__%s\\n' \"${TERM_PROGRAM:-missing}\"\n",
                )
                .unwrap();

            wait_until(Duration::from_secs(3), || {
                manager.refresh_all_panes().unwrap_or(false)
                    && manager
                        .active_pane_text()
                        .map(|text| text.contains("__TERM_PROGRAM__mtrm"))
                        .unwrap_or(false)
            })
        });

        assert!(
            has_program_identity,
            "interactive apps inside mtrm should be able to detect that they are running under mtrm via TERM_PROGRAM=mtrm"
        );
    }

    #[test]
    fn pane_selection_text_extracts_single_line_range() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
        manager
            .write_to_active_pane(b"printf 'hello world'\n")
            .unwrap();

        let loaded = wait_until(Duration::from_secs(2), || {
            manager.refresh_all_panes().unwrap_or(false)
                && manager
                    .active_pane_text()
                    .map(|text| text.contains("hello world"))
                    .unwrap_or(false)
        });
        assert!(loaded);
        let (row, col) = find_visible_text_position(&manager, manager.active_pane_id(), "hello");

        let selected = manager
            .pane_selection_text(manager.active_pane_id(), (row, col), (row, col + 4))
            .unwrap();
        assert_eq!(selected, "hello");
    }

    #[test]
    fn pane_selection_text_preserves_internal_spaces_and_wide_chars() {
        let temp = tempdir().unwrap();
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
        manager.write_to_active_pane("界 a".as_bytes()).unwrap();

        let loaded = wait_until(Duration::from_secs(2), || {
            manager.refresh_all_panes().unwrap_or(false)
                && manager
                    .active_pane_text()
                    .map(|text| text.contains("界 a"))
                    .unwrap_or(false)
        });
        assert!(loaded);
        let (row, col) = find_visible_text_position(&manager, manager.active_pane_id(), "界 a");

        let selected = manager
            .pane_selection_text(manager.active_pane_id(), (row, col), (row, col + 3))
            .unwrap();
        assert_eq!(selected, "界 a");
    }
}
