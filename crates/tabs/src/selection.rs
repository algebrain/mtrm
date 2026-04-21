use mtrm_core::PaneId;
use mtrm_terminal_screen::ScreenLine;

use crate::{PaneEntry, TabManager, TabsError};

impl TabManager {
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
            .map(|pane| pane.screen.shows_history_snapshot() || pane.screen.scrollback() > 0)
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
                if pane.screen.shows_history_snapshot() || pane.screen.scrollback() > 0 {
                    None
                } else {
                    Some(pane.screen.cursor_position())
                }
            })
            .ok_or(TabsError::PaneNotFound(pane_id))
    }

    pub(crate) fn active_visible_rows(&self) -> Result<u16, TabsError> {
        let pane_id = self.active_pane_id();
        self.find_pane(pane_id)
            .map(|pane| pane.screen.size().0)
            .ok_or(TabsError::PaneNotFound(pane_id))
    }

    pub(crate) fn find_pane(&self, pane_id: PaneId) -> Option<&PaneEntry> {
        self.tabs.iter().find_map(|tab| tab.panes.get(&pane_id))
    }

    #[cfg(test)]
    pub(crate) fn pane_has_empty_screen(&self, pane_id: PaneId) -> Result<bool, TabsError> {
        self.find_pane(pane_id)
            .map(|pane| pane.screen.text_contents().trim().is_empty())
            .ok_or(TabsError::PaneNotFound(pane_id))
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
