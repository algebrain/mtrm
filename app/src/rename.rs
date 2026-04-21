use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtrm_keymap::Keymap;

use crate::app::{App, AppError};
use crate::cli::tabs_error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RenameTarget {
    Tab(mtrm_core::TabId),
    Pane(mtrm_core::PaneId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenameState {
    pub(crate) target: RenameTarget,
    pub(crate) input: String,
    pub(crate) cursor: usize,
}

pub(crate) fn is_start_rename_tab_event(event: KeyEvent, keymap: &Keymap) -> bool {
    event.modifiers == (KeyModifiers::ALT | KeyModifiers::SHIFT)
        && matches!(event.code, KeyCode::Char(ch) if keymap.matches_rename_tab(ch))
}

pub(crate) fn is_start_rename_pane_event(event: KeyEvent, keymap: &Keymap) -> bool {
    event.modifiers == (KeyModifiers::ALT | KeyModifiers::SHIFT)
        && matches!(event.code, KeyCode::Char(ch) if keymap.matches_rename_pane(ch))
}

fn remove_char_at(input: &str, index: usize) -> String {
    input
        .chars()
        .enumerate()
        .filter_map(|(i, ch)| (i != index).then_some(ch))
        .collect()
}

fn insert_char_at(input: &str, index: usize, ch: char) -> String {
    let mut result = String::new();
    let mut inserted = false;
    for (i, existing) in input.chars().enumerate() {
        if i == index {
            result.push(ch);
            inserted = true;
        }
        result.push(existing);
    }
    if !inserted {
        result.push(ch);
    }
    result
}

fn normalized_tab_title(tab_id: mtrm_core::TabId, input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        format!("Tab {}", tab_id.get() + 1)
    } else {
        trimmed.to_owned()
    }
}

fn normalized_pane_title(pane_id: mtrm_core::PaneId, input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        format!("pane-{}", pane_id.get())
    } else {
        trimmed.to_owned()
    }
}

impl App {
    pub(crate) fn open_rename_tab_modal(&mut self) {
        let input = self.tabs.active_tab_title().to_owned();
        let cursor = input.chars().count();
        self.rename = Some(RenameState {
            target: RenameTarget::Tab(self.tabs.active_tab_id()),
            input,
            cursor,
        });
        self.ui_dirty = true;
    }

    pub(crate) fn open_rename_pane_modal(&mut self) {
        let input = self
            .tabs
            .active_pane_title()
            .map(|title| title.to_owned())
            .unwrap_or_else(|_| format!("pane-{}", self.tabs.active_pane_id().get()));
        let cursor = input.chars().count();
        self.rename = Some(RenameState {
            target: RenameTarget::Pane(self.tabs.active_pane_id()),
            input,
            cursor,
        });
        self.ui_dirty = true;
    }

    pub(crate) fn handle_rename_key_event(&mut self, event: KeyEvent) -> Result<(), AppError> {
        let Some(state) = &mut self.rename else {
            return Ok(());
        };

        match event.code {
            KeyCode::Esc => {
                self.rename = None;
                self.ui_dirty = true;
            }
            KeyCode::Enter => {
                let target = state.target.clone();
                let title = match target {
                    RenameTarget::Tab(tab_id) => normalized_tab_title(tab_id, &state.input),
                    RenameTarget::Pane(pane_id) => normalized_pane_title(pane_id, &state.input),
                };
                match target {
                    RenameTarget::Tab(tab_id) => {
                        self.tabs.rename_tab(tab_id, title).map_err(tabs_error)?;
                    }
                    RenameTarget::Pane(pane_id) => {
                        self.tabs.rename_pane(pane_id, title).map_err(tabs_error)?;
                    }
                }
                self.rename = None;
                self.ui_dirty = true;
                self.try_save_with_notice();
            }
            KeyCode::Backspace => {
                if state.cursor > 0 {
                    let remove_at = state.cursor - 1;
                    state.input = remove_char_at(&state.input, remove_at);
                    state.cursor -= 1;
                    self.ui_dirty = true;
                }
            }
            KeyCode::Left => {
                state.cursor = state.cursor.saturating_sub(1);
                self.ui_dirty = true;
            }
            KeyCode::Right => {
                let len = state.input.chars().count();
                state.cursor = (state.cursor + 1).min(len);
                self.ui_dirty = true;
            }
            KeyCode::Home => {
                state.cursor = 0;
                self.ui_dirty = true;
            }
            KeyCode::End => {
                state.cursor = state.input.chars().count();
                self.ui_dirty = true;
            }
            KeyCode::Char(ch) if !event.modifiers.contains(KeyModifiers::CONTROL) => {
                state.input = insert_char_at(&state.input, state.cursor, ch);
                state.cursor += 1;
                self.ui_dirty = true;
            }
            _ => {}
        }

        Ok(())
    }
}
