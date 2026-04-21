use std::io::Write;
use std::time::{Instant, UNIX_EPOCH};

use mtrm_keymap::{Keymap, load_keymap};
use mtrm_process::ShellProcessConfig;
use mtrm_state::{load_state, save_state};
use mtrm_tabs::TabManager;
use thiserror::Error;

use crate::cli::{keymap_error, state_error, tabs_error};
use crate::help::HelpOverlayState;
use crate::rename::RenameState;
use crate::selection::SelectionState;

pub(crate) const NOTICE_TTL: std::time::Duration = std::time::Duration::from_secs(3);
pub(crate) const DEFAULT_CONTENT_AREA: mtrm_layout::Rect = mtrm_layout::Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 23,
};

pub struct App {
    pub(crate) shell: ShellProcessConfig,
    pub(crate) keymap: Keymap,
    pub(crate) tabs: TabManager,
    pub(crate) selection: Option<SelectionState>,
    pub(crate) should_quit: bool,
    pub(crate) ui_dirty: bool,
    pub(crate) window_focused: bool,
    pub(crate) pending_alt_prefix_started_at: Option<Instant>,
    pub(crate) rename: Option<RenameState>,
    pub(crate) help_overlay: Option<HelpOverlayState>,
    pub(crate) clipboard_notice: Option<UiNotice>,
    pub(crate) last_content_area: mtrm_layout::Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LayoutCommandResult {
    pub(crate) persist: bool,
    pub(crate) ui_dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UiNotice {
    pub(crate) text: String,
    pub(crate) shown_at: Instant,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("configuration error")]
    Config(String),
    #[error("state error")]
    State(String),
    #[error("tabs error")]
    Tabs(String),
    #[error("terminal io error")]
    TerminalIo(String),
}

impl App {
    #[cfg(test)]
    pub fn new(shell: ShellProcessConfig) -> Result<Self, AppError> {
        Self::new_with_keymap(shell, Keymap::default())
    }

    fn new_with_keymap(shell: ShellProcessConfig, keymap: Keymap) -> Result<Self, AppError> {
        let tabs = TabManager::new(&shell).map_err(tabs_error)?;
        Ok(Self {
            shell,
            keymap,
            tabs,
            selection: None,
            should_quit: false,
            ui_dirty: true,
            window_focused: true,
            pending_alt_prefix_started_at: None,
            rename: None,
            help_overlay: None,
            clipboard_notice: None,
            last_content_area: DEFAULT_CONTENT_AREA,
        })
    }

    pub fn restore_or_new(shell: ShellProcessConfig) -> Result<Self, AppError> {
        let keymap = load_keymap().map_err(keymap_error)?;
        match load_state().map_err(state_error)? {
            Some(snapshot) => {
                let tabs = TabManager::from_snapshot(snapshot, &shell).map_err(tabs_error)?;
                Ok(Self {
                    shell,
                    keymap,
                    tabs,
                    selection: None,
                    should_quit: false,
                    ui_dirty: true,
                    window_focused: true,
                    pending_alt_prefix_started_at: None,
                    rename: None,
                    help_overlay: None,
                    clipboard_notice: None,
                    last_content_area: DEFAULT_CONTENT_AREA,
                })
            }
            None => Self::new_with_keymap(shell, keymap),
        }
    }

    pub fn save(&mut self) -> Result<(), AppError> {
        let snapshot = self.tabs.snapshot().map_err(tabs_error)?;
        save_state(&snapshot).map_err(state_error)
    }

    pub(crate) fn refresh_all_panes_output(&mut self) -> Result<bool, mtrm_tabs::TabsError> {
        self.tabs.refresh_all_panes()
    }

    pub(crate) fn append_debug_log_event(&self, event: &str) {
        let Some(path) = &self.shell.debug_log_path else {
            return;
        };

        let timestamp = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);

        let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        else {
            return;
        };

        let _ = writeln!(file, "[{timestamp}] MTRM_EVENT {event}");
        let _ = file.flush();
    }
}
