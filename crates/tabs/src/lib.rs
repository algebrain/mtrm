//! Управление вкладками и живыми окнами.

mod selection;

use std::collections::BTreeMap;
use std::path::PathBuf;

use mtrm_core::{FocusMoveDirection, IdAllocator, PaneId, ResizeDirection, SplitDirection, TabId};
use mtrm_layout::{LayoutError, LayoutTree, Rect};
use mtrm_process::{ShellProcess, ShellProcessConfig};
use mtrm_session::{PaneSnapshot, SessionSnapshot, TabSnapshot};
use mtrm_terminal_screen::TerminalScreen;
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
        let pane = self
            .active_tab()
            .panes
            .get(&pane_id)
            .ok_or(TabsError::PaneNotFound(pane_id))?;
        Ok(live_or_last_known_cwd(pane))
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
                let cwd = live_or_last_known_cwd(process);
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

    pub fn inject_bytes_into_active_pane_screen(&mut self, bytes: &[u8]) -> Result<(), TabsError> {
        let pane_id = self.active_pane_id();
        let pane = self
            .active_tab_mut()
            .panes
            .get_mut(&pane_id)
            .ok_or(TabsError::PaneNotFound(pane_id))?;
        pane.screen.process_bytes(bytes);
        Ok(())
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
}

struct PaneEntry {
    process: ShellProcess,
    screen: TerminalScreen,
    last_known_cwd: PathBuf,
    title: String,
}

fn spawn_shell(shell: &ShellProcessConfig, cwd: PathBuf, title: String) -> Result<PaneEntry, TabsError> {
    let config = ShellProcessConfig {
        program: shell.program.clone(),
        args: shell.args.clone(),
        initial_cwd: cwd.clone(),
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
        last_known_cwd: cwd,
        title,
    })
}

fn default_pane_title_for(pane_id: PaneId) -> String {
    format!("pane-{}", pane_id.get())
}

fn process_error(error: impl ToString) -> TabsError {
    TabsError::Process(error.to_string())
}

fn live_or_last_known_cwd(pane: &PaneEntry) -> PathBuf {
    pane.process
        .current_dir()
        .unwrap_or_else(|_| pane.last_known_cwd.clone())
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
mod tests;
