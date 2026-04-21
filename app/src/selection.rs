use crossterm::event::MouseEvent;
use mtrm_ui::{PaneSelectionView, TAB_DIVIDER};

use crate::app::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SelectionPoint {
    pub(crate) row: u16,
    pub(crate) col: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SelectionState {
    pub(crate) pane_id: mtrm_core::PaneId,
    pub(crate) anchor: SelectionPoint,
    pub(crate) focus: SelectionPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PaneSelectionTarget {
    pub(crate) pane_id: mtrm_core::PaneId,
    pub(crate) point: SelectionPoint,
}

impl SelectionState {
    pub(crate) fn is_empty(&self) -> bool {
        self.anchor == self.focus
    }

    fn normalized(&self) -> (SelectionPoint, SelectionPoint) {
        if (self.anchor.row, self.anchor.col) <= (self.focus.row, self.focus.col) {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }

    pub(crate) fn view_for(&self, pane_id: mtrm_core::PaneId) -> Option<PaneSelectionView> {
        if self.pane_id != pane_id || self.is_empty() {
            return None;
        }
        let (start, end) = self.normalized();
        Some(PaneSelectionView {
            start: (start.row, start.col),
            end: (end.row, end.col),
        })
    }
}

impl App {
    pub(crate) fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub(crate) fn selection_target_at(
        &self,
        content_area: mtrm_layout::Rect,
        event: MouseEvent,
    ) -> Option<PaneSelectionTarget> {
        self.tabs
            .placements(content_area)
            .ok()?
            .into_iter()
            .find_map(|(pane_id, area, _)| {
                point_in_pane_content(area, pane_id, event.column, event.row)
            })
    }

    pub(crate) fn drag_selection_target(
        &self,
        content_area: mtrm_layout::Rect,
        event: MouseEvent,
    ) -> Option<PaneSelectionTarget> {
        let selection = self.selection?;
        let (_, pane_area, _) = self
            .tabs
            .placements(content_area)
            .ok()?
            .into_iter()
            .find(|(pane_id, _, _)| *pane_id == selection.pane_id)?;
        point_in_or_clamped_to_pane_content(pane_area, selection.pane_id, event.column, event.row)
    }

    pub(crate) fn tab_id_at_mouse_column(
        &self,
        terminal_width: u16,
        event: MouseEvent,
    ) -> Option<mtrm_core::TabId> {
        tab_id_at_position(
            &self.tabs.tab_summaries(),
            terminal_width,
            event.column,
            event.row,
        )
    }
}

fn point_in_pane_content(
    area: mtrm_layout::Rect,
    pane_id: mtrm_core::PaneId,
    column: u16,
    row: u16,
) -> Option<PaneSelectionTarget> {
    let rect = pane_content_rect(area)?;
    if column < rect.x || column >= rect.x.saturating_add(rect.width) {
        return None;
    }
    if row < rect.y || row >= rect.y.saturating_add(rect.height) {
        return None;
    }
    Some(PaneSelectionTarget {
        pane_id,
        point: SelectionPoint {
            row: row.saturating_sub(rect.y),
            col: column.saturating_sub(rect.x),
        },
    })
}

fn point_in_or_clamped_to_pane_content(
    area: mtrm_layout::Rect,
    pane_id: mtrm_core::PaneId,
    column: u16,
    row: u16,
) -> Option<PaneSelectionTarget> {
    let rect = pane_content_rect(area)?;
    let max_x = rect.x.saturating_add(rect.width.saturating_sub(1));
    let max_y = rect.y.saturating_add(rect.height.saturating_sub(1));
    let clamped_col = column.clamp(rect.x, max_x);
    let clamped_row = row.clamp(rect.y, max_y);
    Some(PaneSelectionTarget {
        pane_id,
        point: SelectionPoint {
            row: clamped_row.saturating_sub(rect.y),
            col: clamped_col.saturating_sub(rect.x),
        },
    })
}

pub(crate) fn pane_content_rect(area: mtrm_layout::Rect) -> Option<mtrm_layout::Rect> {
    if area.width <= 2 || area.height <= 2 {
        return None;
    }
    Some(mtrm_layout::Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(2),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    })
}

pub(crate) fn tab_id_at_position(
    tabs: &[mtrm_tabs::RuntimeTabSummary],
    terminal_width: u16,
    column: u16,
    row: u16,
) -> Option<mtrm_core::TabId> {
    if row != 0 || tabs.is_empty() || column >= terminal_width {
        return None;
    }

    let divider_width = TAB_DIVIDER.chars().count().min(u16::MAX as usize) as u16;
    let mut x = 0_u16;
    for (index, tab) in tabs.iter().enumerate() {
        let title_width = tab.title.chars().count().min(u16::MAX as usize) as u16;
        let end = x.saturating_add(title_width);
        if column >= x && column < end {
            return Some(tab.id);
        }

        x = end;
        if index + 1 < tabs.len() {
            if column >= x && column < x.saturating_add(divider_width) {
                return None;
            }
            x = x.saturating_add(divider_width);
        }
    }

    None
}
