use mtrm_ui::{
    ClipboardNoticeView, FrameView, InputModalView, ModalView, PaneView, TabView, TextModalView,
};

use crate::app::{App, AppError, NOTICE_TTL};
use crate::cli::tabs_error;
use crate::help::help_overlay_lines;
use crate::rename::RenameTarget;

impl App {
    pub(crate) fn build_frame_view(
        &mut self,
        content_area: mtrm_layout::Rect,
    ) -> Result<FrameView, AppError> {
        let snapshot = self.tabs.snapshot().map_err(tabs_error)?;
        let active_tab = snapshot.active_tab;
        let active_pane = self.tabs.active_pane_id();
        let active_pane_is_scrolled_back =
            self.tabs.active_pane_is_scrolled_back().unwrap_or(false);
        let active_tab_snapshot = snapshot
            .tabs
            .iter()
            .find(|tab| tab.id == active_tab)
            .ok_or_else(|| AppError::Tabs("active tab missing in snapshot".to_owned()))?;

        let tabs = snapshot
            .tabs
            .iter()
            .map(|tab| TabView {
                id: tab.id,
                title: tab.title.clone(),
                active: tab.id == active_tab,
            })
            .collect();

        let placements = self.tabs.placements(content_area).map_err(tabs_error)?;
        let panes = placements
            .into_iter()
            .map(|(id, area, focused)| {
                let cursor = if id == active_pane && active_pane_is_scrolled_back {
                    None
                } else {
                    self.tabs.pane_cursor(id).ok().flatten()
                };
                PaneView {
                    id,
                    title: self
                        .tabs
                        .pane_title(id)
                        .map(|title| title.to_owned())
                        .map_err(tabs_error)
                        .unwrap_or_else(|_| format!("pane-{}", id.get())),
                    area,
                    active: focused,
                    lines: self
                        .tabs
                        .pane_lines(id)
                        .map_err(tabs_error)
                        .unwrap_or_default(),
                    selection: self.selection.and_then(|selection| selection.view_for(id)),
                    cursor,
                }
            })
            .collect();

        let modal = if let Some(rename) = self.rename.as_ref() {
            Some(ModalView::Input(InputModalView {
                title: match rename.target {
                    RenameTarget::Tab(_) => "Rename Tab".to_owned(),
                    RenameTarget::Pane(_) => "Rename Pane".to_owned(),
                },
                input: rename.input.clone(),
                cursor: rename.cursor,
                hint: "Enter apply, Esc cancel".to_owned(),
            }))
        } else {
            self.help_overlay.as_ref().map(|help| {
                ModalView::Text(TextModalView {
                    title: "Help".to_owned(),
                    lines: help_overlay_lines(),
                    scroll_row: help.scroll_row,
                    scroll_col: help.scroll_col,
                    hint: "Esc close, arrows scroll, PgUp/PgDn page".to_owned(),
                })
            })
        };
        let clipboard_notice = self
            .clipboard_notice
            .as_ref()
            .filter(|notice| notice.shown_at.elapsed() < NOTICE_TTL)
            .map(|notice| ClipboardNoticeView {
                text: notice.text.clone(),
            });

        let _ = active_tab_snapshot;
        Ok(FrameView {
            tabs,
            panes,
            focused: self.window_focused,
            clipboard_notice,
            modal,
        })
    }
}
