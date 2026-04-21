use std::time::Instant;

use mtrm_clipboard::{ClipboardBackend, ClipboardError};
use mtrm_core::{AppCommand, ClipboardCommand, LayoutCommand, TabCommand};

use crate::app::{App, AppError, LayoutCommandResult, NOTICE_TTL, UiNotice};
use crate::cli::{notice_for_clipboard_error, tabs_error};

impl App {
    pub(crate) fn handle_command(
        &mut self,
        command: AppCommand,
        clipboard: &mut dyn ClipboardBackend,
    ) -> Result<(), AppError> {
        match command {
            AppCommand::Clipboard(ClipboardCommand::CopySelection) => {
                if let Some(selection) = self.selection.filter(|selection| !selection.is_empty()) {
                    let text = self
                        .tabs
                        .pane_selection_text(
                            selection.pane_id,
                            (selection.anchor.row, selection.anchor.col),
                            (selection.focus.row, selection.focus.col),
                        )
                        .map_err(tabs_error)?;
                    match clipboard.set_text(&text) {
                        Ok(()) => {}
                        Err(ClipboardError::Unavailable) => {
                            self.show_notice("Clipboard is unavailable");
                        }
                        Err(error) => {
                            self.show_notice(notice_for_clipboard_error(&error));
                        }
                    }
                }
            }
            AppCommand::Clipboard(ClipboardCommand::PasteFromSystem) => {
                self.clear_selection();
                let text = match clipboard.get_text() {
                    Ok(text) => text,
                    Err(ClipboardError::Unavailable) => {
                        self.show_notice("Clipboard is unavailable");
                        return Ok(());
                    }
                    Err(error) => {
                        self.show_notice(notice_for_clipboard_error(&error));
                        return Ok(());
                    }
                };
                self.tabs
                    .write_to_active_pane(text.as_bytes())
                    .map_err(tabs_error)?;
                self.ui_dirty |= self.refresh_all_panes_output().map_err(tabs_error)?;
                self.try_save_with_notice();
            }
            AppCommand::Layout(layout_command) => {
                self.clear_selection();
                match self.handle_layout_command(layout_command) {
                    Ok(result) => {
                        self.ui_dirty |= result.ui_dirty;
                        if result.persist {
                            self.try_save_with_notice();
                        }
                    }
                    Err(_) => self.show_notice("Failed to update layout"),
                }
            }
            AppCommand::Tabs(tab_command) => {
                self.clear_selection();
                match self.handle_tab_command(tab_command) {
                    Ok(()) => {
                        self.ui_dirty = true;
                        self.try_save_with_notice();
                    }
                    Err(_) => self.show_notice("Failed to update tabs"),
                }
            }
            AppCommand::SendInterrupt => {
                self.clear_selection();
                match self.tabs.send_interrupt_to_active_pane().map_err(tabs_error) {
                    Ok(()) => {
                        self.ui_dirty = true;
                    }
                    Err(_) => self.show_notice("Failed to interrupt active process"),
                }
            }
            AppCommand::RequestSave => {
                self.try_save_with_notice();
            }
            AppCommand::Quit => {
                if self.try_save_with_notice() {
                    self.should_quit = true;
                }
            }
        }

        Ok(())
    }

    pub(crate) fn show_notice(&mut self, text: impl Into<String>) {
        let now = Instant::now();
        let should_refresh = self
            .clipboard_notice
            .as_ref()
            .map(|notice| now.duration_since(notice.shown_at) >= NOTICE_TTL)
            .unwrap_or(true);

        if should_refresh {
            self.clipboard_notice = Some(UiNotice {
                text: text.into(),
                shown_at: now,
            });
        }
        self.ui_dirty = true;
    }

    pub(crate) fn try_save_with_notice(&mut self) -> bool {
        match self.save() {
            Ok(()) => true,
            Err(_) => {
                self.show_notice("Failed to save state");
                false
            }
        }
    }

    pub(crate) fn try_refresh_all_panes_output(&mut self) -> bool {
        match self.refresh_all_panes_output() {
            Ok(changed) => changed,
            Err(_) => {
                self.show_notice("Failed to refresh pane output");
                false
            }
        }
    }

    pub(crate) fn try_write_to_active_pane(&mut self, bytes: &[u8], notice: &'static str) -> bool {
        match self.tabs.write_to_active_pane(bytes).map_err(tabs_error) {
            Ok(()) => true,
            Err(_) => {
                self.show_notice(notice);
                false
            }
        }
    }

    pub(crate) fn try_resize_active_tab(&mut self, area: mtrm_layout::Rect) -> bool {
        match self.tabs.resize_active_tab(area).map_err(tabs_error) {
            Ok(()) => true,
            Err(_) => {
                self.show_notice("Failed to resize active tab");
                false
            }
        }
    }

    pub(crate) fn try_activate_tab(&mut self, tab_id: mtrm_core::TabId) -> bool {
        match self.tabs.activate_tab(tab_id).map_err(tabs_error) {
            Ok(()) => true,
            Err(_) => {
                self.show_notice("Failed to update tabs");
                false
            }
        }
    }

    pub(crate) fn try_focus_pane(&mut self, pane_id: mtrm_core::PaneId) -> bool {
        match self.tabs.focus_pane(pane_id).map_err(tabs_error) {
            Ok(()) => true,
            Err(_) => {
                self.show_notice("Failed to focus pane");
                false
            }
        }
    }

    pub(crate) fn handle_layout_command(
        &mut self,
        command: LayoutCommand,
    ) -> Result<LayoutCommandResult, AppError> {
        let result = match command {
            LayoutCommand::SplitFocused(direction) => {
                let _ = self
                    .tabs
                    .split_active_pane(direction, &self.shell)
                    .map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: true,
                    ui_dirty: true,
                }
            }
            LayoutCommand::CloseFocusedPane => {
                let _ = self.tabs.close_active_pane().map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: true,
                    ui_dirty: true,
                }
            }
            LayoutCommand::MoveFocus(direction) => {
                self.tabs.move_focus(direction).map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: true,
                    ui_dirty: true,
                }
            }
            LayoutCommand::ResizeFocused(direction) => {
                let changed = self
                    .tabs
                    .resize_active_pane(direction, self.last_content_area)
                    .map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: changed,
                    ui_dirty: changed,
                }
            }
            LayoutCommand::ScrollUpLines(lines) => {
                self.append_debug_log_event(&format!("SCROLL_UP_LINES lines={lines}"));
                self.tabs
                    .scroll_active_pane_up_lines(lines)
                    .map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: false,
                    ui_dirty: true,
                }
            }
            LayoutCommand::ScrollDownLines(lines) => {
                self.append_debug_log_event(&format!("SCROLL_DOWN_LINES lines={lines}"));
                self.tabs
                    .scroll_active_pane_down_lines(lines)
                    .map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: false,
                    ui_dirty: true,
                }
            }
            LayoutCommand::ScrollUpPages(pages) => {
                self.append_debug_log_event(&format!("SCROLL_UP_PAGES pages={pages}"));
                self.tabs
                    .scroll_active_pane_up_pages(pages)
                    .map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: false,
                    ui_dirty: true,
                }
            }
            LayoutCommand::ScrollDownPages(pages) => {
                self.append_debug_log_event(&format!("SCROLL_DOWN_PAGES pages={pages}"));
                self.tabs
                    .scroll_active_pane_down_pages(pages)
                    .map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: false,
                    ui_dirty: true,
                }
            }
            LayoutCommand::ScrollToBottom => {
                self.append_debug_log_event("SCROLL_TO_BOTTOM");
                self.tabs
                    .scroll_active_pane_to_bottom()
                    .map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: false,
                    ui_dirty: true,
                }
            }
        };
        Ok(result)
    }

    pub(crate) fn handle_tab_command(&mut self, command: TabCommand) -> Result<(), AppError> {
        match command {
            TabCommand::NewTab => {
                let _ = self.tabs.new_tab(&self.shell).map_err(tabs_error)?;
            }
            TabCommand::CloseCurrentTab => {
                self.tabs.close_active_tab().map_err(tabs_error)?;
            }
            TabCommand::NextTab => {
                let ids = self.tabs.tab_ids();
                let current = self.tabs.active_tab_id();
                if let Some(index) = ids.iter().position(|id| *id == current) {
                    let next = ids[(index + 1) % ids.len()];
                    self.tabs.activate_tab(next).map_err(tabs_error)?;
                }
            }
            TabCommand::PreviousTab => {
                let ids = self.tabs.tab_ids();
                let current = self.tabs.active_tab_id();
                if let Some(index) = ids.iter().position(|id| *id == current) {
                    let previous = ids[(index + ids.len() - 1) % ids.len()];
                    self.tabs.activate_tab(previous).map_err(tabs_error)?;
                }
            }
            TabCommand::Activate(tab_id) => {
                self.tabs.activate_tab(tab_id).map_err(tabs_error)?;
            }
        }
        Ok(())
    }
}
