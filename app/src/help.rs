use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::cli::help_text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HelpOverlayState {
    pub(crate) scroll_row: usize,
    pub(crate) scroll_col: usize,
}

impl App {
    pub(crate) fn open_help_overlay(&mut self) {
        let body_rows = self.help_body_rows();
        self.help_overlay = Some(HelpOverlayState {
            scroll_row: clamp_help_scroll_row(keybindings_section_row(), body_rows),
            scroll_col: 0,
        });
        self.ui_dirty = true;
    }

    pub(crate) fn handle_help_key_event(&mut self, event: KeyEvent) {
        let page_rows = self.help_body_rows();
        let max_row = max_help_scroll_row(page_rows);
        let max_col = max_help_scroll_col();

        let Some(state) = &mut self.help_overlay else {
            return;
        };

        match event.code {
            KeyCode::Esc => {
                self.help_overlay = None;
                self.ui_dirty = true;
            }
            KeyCode::Up => {
                state.scroll_row = state.scroll_row.saturating_sub(1);
                self.ui_dirty = true;
            }
            KeyCode::Down => {
                state.scroll_row = (state.scroll_row + 1).min(max_row);
                self.ui_dirty = true;
            }
            KeyCode::Left => {
                state.scroll_col = state.scroll_col.saturating_sub(1);
                self.ui_dirty = true;
            }
            KeyCode::Right => {
                state.scroll_col = (state.scroll_col + 1).min(max_col);
                self.ui_dirty = true;
            }
            KeyCode::PageUp => {
                state.scroll_row = state.scroll_row.saturating_sub(page_rows);
                self.ui_dirty = true;
            }
            KeyCode::PageDown => {
                state.scroll_row = (state.scroll_row + page_rows).min(max_row);
                self.ui_dirty = true;
            }
            KeyCode::Home => {
                state.scroll_row = 0;
                state.scroll_col = 0;
                self.ui_dirty = true;
            }
            KeyCode::End => {
                state.scroll_row = max_row;
                state.scroll_col = max_col;
                self.ui_dirty = true;
            }
            KeyCode::F(1) if event.modifiers == KeyModifiers::SHIFT => {
                self.help_overlay = None;
                self.ui_dirty = true;
            }
            _ => {}
        }
    }

    pub(crate) fn help_body_rows(&self) -> usize {
        let full_height = self.last_content_area.height.saturating_add(1);
        let modal_height = full_height.min(20);
        let inner_height = modal_height.saturating_sub(2);
        let hint_height = if inner_height > 1 { 1 } else { 0 };
        inner_height.saturating_sub(hint_height).max(1) as usize
    }
}

pub(crate) fn is_toggle_help_overlay_event(event: KeyEvent) -> bool {
    event.modifiers == KeyModifiers::SHIFT && matches!(event.code, KeyCode::F(1))
}

pub(crate) fn help_overlay_lines() -> Vec<String> {
    help_text().lines().map(str::to_owned).collect()
}

pub(crate) fn keybindings_section_row() -> usize {
    help_overlay_lines()
        .iter()
        .position(|line| line.trim() == "Keybindings:")
        .unwrap_or(0)
}

pub(crate) fn clamp_help_scroll_row(row: usize, page_rows: usize) -> usize {
    row.min(max_help_scroll_row(page_rows))
}

fn max_help_scroll_row(page_rows: usize) -> usize {
    help_overlay_lines().len().saturating_sub(page_rows)
}

fn max_help_scroll_col() -> usize {
    help_overlay_lines()
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
        .saturating_sub(1)
}
