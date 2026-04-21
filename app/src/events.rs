use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use mtrm_clipboard::ClipboardBackend;
use mtrm_input::{InputAction, map_key_event_with_keymap};
use ratatui::Terminal;
use ratatui::backend::Backend;

use crate::app::{App, AppError};
use crate::cli::{tabs_error, terminal_content_area, terminal_io_error};
use crate::rename::{is_start_rename_pane_event, is_start_rename_tab_event};

const ALT_PREFIX_TIMEOUT: Duration = Duration::from_millis(80);

impl App {
    pub fn handle_key_event(
        &mut self,
        event: KeyEvent,
        clipboard: &mut dyn ClipboardBackend,
    ) -> Result<(), AppError> {
        if self.rename.is_some() {
            return self.handle_rename_key_event(event);
        }

        let Some(event) = self.resolve_alt_prefixed_key_event(event) else {
            return Ok(());
        };

        if is_start_rename_tab_event(event, &self.keymap) {
            self.open_rename_tab_modal();
            return Ok(());
        }
        if is_start_rename_pane_event(event, &self.keymap) {
            self.open_rename_pane_modal();
            return Ok(());
        }

        if self.handle_terminal_navigation_key(event) {
            return Ok(());
        }

        match map_key_event_with_keymap(event, &self.keymap) {
            InputAction::Ignore => {}
            InputAction::PtyBytes(bytes) => {
                self.clear_selection();
                if self.try_write_to_active_pane(&bytes, "Failed to write to active pane") {
                    self.ui_dirty |= self.try_refresh_all_panes_output();
                }
            }
            InputAction::Command(command) => {
                self.handle_command(command, clipboard)?;
            }
        }

        Ok(())
    }

    pub(crate) fn handle_mouse_event(
        &mut self,
        event: MouseEvent,
        content_area: mtrm_layout::Rect,
    ) -> Result<(), AppError> {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(tab_id) = self.tab_id_at_mouse_column(content_area.width, event) {
                    self.clear_selection();
                    if self.try_activate_tab(tab_id) {
                        self.ui_dirty = true;
                        self.try_save_with_notice();
                    }
                } else if let Some(target) = self.selection_target_at(content_area, event) {
                    if self.try_focus_pane(target.pane_id) {
                        self.selection = Some(crate::selection::SelectionState {
                            pane_id: target.pane_id,
                            anchor: target.point,
                            focus: target.point,
                        });
                    }
                } else {
                    self.clear_selection();
                }
                self.ui_dirty = true;
            }
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) => {
                if let Some(target) = self.drag_selection_target(content_area, event) {
                    if let Some(selection) = &mut self.selection {
                        selection.focus = target.point;
                        self.ui_dirty = true;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_terminal_navigation_key(&mut self, event: KeyEvent) -> bool {
        if event.modifiers != KeyModifiers::NONE {
            return false;
        }

        match event.code {
            KeyCode::Home => {
                if self.try_write_to_active_pane(b"\x1b[H", "Failed to move shell cursor") {
                    self.ui_dirty |= self.try_refresh_all_panes_output();
                }
                true
            }
            KeyCode::End => {
                if self.tabs.active_pane_is_scrolled_back().unwrap_or(false) {
                    match self.tabs.scroll_active_pane_to_bottom().map_err(tabs_error) {
                        Ok(()) => {
                            self.ui_dirty = true;
                        }
                        Err(_) => self.show_notice("Failed to scroll active pane"),
                    }
                } else if self.try_write_to_active_pane(b"\x1b[F", "Failed to move shell cursor") {
                    self.ui_dirty |= self.try_refresh_all_panes_output();
                }
                true
            }
            _ => false,
        }
    }

    pub fn redraw<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), AppError> {
        self.ui_dirty |= self.try_refresh_all_panes_output();
        let content_area = terminal_content_area(terminal)?;
        self.last_content_area = content_area;
        self.try_resize_active_tab(content_area);
        let frame_view = self.build_frame_view(content_area)?;
        mtrm_ui::render_frame(terminal, &frame_view).map_err(terminal_io_error)
    }

    pub fn run<B: Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
        clipboard: &mut dyn ClipboardBackend,
    ) -> Result<(), AppError> {
        while !self.should_quit {
            self.flush_pending_alt_prefix_if_expired();
            self.ui_dirty |= self.try_refresh_all_panes_output();
            if self.ui_dirty {
                self.redraw(terminal)?;
                self.ui_dirty = false;
            }

            if event::poll(Duration::from_millis(50)).map_err(terminal_io_error)? {
                match event::read().map_err(terminal_io_error)? {
                    Event::Key(key_event) => self.handle_key_event(key_event, clipboard)?,
                    Event::Mouse(mouse_event) => {
                        let content_area = terminal_content_area(terminal)?;
                        self.handle_mouse_event(mouse_event, content_area)?;
                    }
                    Event::Resize(cols, rows) => {
                        self.clear_selection();
                        let resized = self.try_resize_active_tab(mtrm_layout::Rect {
                            x: 0,
                            y: 0,
                            width: cols,
                            height: rows.saturating_sub(1),
                        });
                        if resized {
                            self.last_content_area = mtrm_layout::Rect {
                                x: 0,
                                y: 0,
                                width: cols,
                                height: rows.saturating_sub(1),
                            };
                            self.ui_dirty = true;
                        }
                    }
                    Event::FocusGained => {
                        self.window_focused = true;
                        self.ui_dirty = true;
                    }
                    Event::FocusLost => {
                        self.window_focused = false;
                        self.ui_dirty = true;
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn resolve_alt_prefixed_key_event(&mut self, event: KeyEvent) -> Option<KeyEvent> {
        if let Some(started_at) = self.pending_alt_prefix_started_at.take() {
            if started_at.elapsed() <= ALT_PREFIX_TIMEOUT {
                if let Some(synthetic) = synthesize_alt_prefixed_key_event(event) {
                    return Some(synthetic);
                }
            }

            if self.try_write_to_active_pane(b"\x1b", "Failed to write to active pane") {
                self.ui_dirty |= self.try_refresh_all_panes_output();
            }
        }

        if event.modifiers == KeyModifiers::NONE && matches!(event.code, KeyCode::Esc) {
            self.pending_alt_prefix_started_at = Some(std::time::Instant::now());
            return None;
        }

        Some(event)
    }

    fn flush_pending_alt_prefix_if_expired(&mut self) {
        let Some(started_at) = self.pending_alt_prefix_started_at else {
            return;
        };
        if started_at.elapsed() <= ALT_PREFIX_TIMEOUT {
            return;
        }

        self.pending_alt_prefix_started_at = None;
        if self.try_write_to_active_pane(b"\x1b", "Failed to write to active pane") {
            self.ui_dirty |= self.try_refresh_all_panes_output();
        }
    }
}

fn synthesize_alt_prefixed_key_event(event: KeyEvent) -> Option<KeyEvent> {
    let modifiers = match event.modifiers {
        KeyModifiers::NONE => KeyModifiers::ALT,
        KeyModifiers::SHIFT => KeyModifiers::ALT | KeyModifiers::SHIFT,
        _ => return None,
    };

    match event.code {
        KeyCode::Char(_) | KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
            Some(KeyEvent {
                code: event.code,
                modifiers,
                kind: event.kind,
                state: event.state,
            })
        }
        _ => None,
    }
}
