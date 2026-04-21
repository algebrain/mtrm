//! Отрисовка интерфейса `mtrm`.

use mtrm_core::{PaneId, TabId};
use mtrm_layout::Rect;
use mtrm_terminal_screen::{ScreenColor, ScreenLine};
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::layout::Rect as TuiRect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs, Widget};

const TAB_BAR_HEIGHT: u16 = 1;
pub const TAB_DIVIDER: &str = " | ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabView {
    pub id: TabId,
    pub title: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneView {
    pub id: PaneId,
    pub title: String,
    pub area: Rect,
    pub active: bool,
    pub lines: Vec<ScreenLine>,
    pub selection: Option<PaneSelectionView>,
    pub cursor: Option<(u16, u16)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneSelectionView {
    pub start: (u16, u16),
    pub end: (u16, u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameView {
    pub tabs: Vec<TabView>,
    pub panes: Vec<PaneView>,
    pub focused: bool,
    pub clipboard_notice: Option<ClipboardNoticeView>,
    pub modal: Option<ModalView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardNoticeView {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModalView {
    pub title: String,
    pub input: String,
    pub cursor: usize,
    pub hint: String,
}

pub fn render_frame<B: Backend>(
    terminal: &mut Terminal<B>,
    frame_view: &FrameView,
) -> Result<(), B::Error> {
    terminal.draw(|frame| {
        let full = frame.area();

        render_tabs(frame, frame_view, full);
        for pane in &frame_view.panes {
            render_pane(frame, pane, full, frame_view.focused);
        }
        if let Some(notice) = &frame_view.clipboard_notice {
            render_clipboard_notice(frame, frame_view, notice, full, TuiRect {
                x: full.x,
                y: full.y,
                width: full.width,
                height: TAB_BAR_HEIGHT.min(full.height),
            });
        }
        if let Some(modal) = &frame_view.modal {
            render_modal(frame, modal, full);
        }
    })?;
    Ok(())
}

fn render_tabs(frame: &mut ratatui::Frame<'_>, frame_view: &FrameView, area: TuiRect) {
    let titles: Vec<Line<'_>> = if frame_view.tabs.is_empty() {
        vec![Line::from("mtrm")]
    } else {
        frame_view
            .tabs
            .iter()
            .map(|tab| Line::from(tab.title.clone()))
            .collect()
    };

    let selected = frame_view
        .tabs
        .iter()
        .position(|tab| tab.active)
        .unwrap_or(0);

    let tabs = Tabs::new(titles)
        .select(selected)
        .highlight_style(active_tab_style(frame_view.focused))
        .divider(Span::styled(TAB_DIVIDER, Style::default().fg(Color::DarkGray)))
        .padding("", "")
        .style(Style::default().fg(Color::Gray));

    let tab_area = TuiRect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: TAB_BAR_HEIGHT.min(area.height),
    };
    frame.render_widget(tabs, tab_area);

}

fn render_clipboard_notice(
    frame: &mut ratatui::Frame<'_>,
    frame_view: &FrameView,
    notice: &ClipboardNoticeView,
    full_area: TuiRect,
    tab_area: TuiRect,
) {
    let notice_width = notice.text.chars().count().min(u16::MAX as usize) as u16;
    let tabs_width = tab_bar_titles_width(frame_view);
    let has_space_in_tab_bar = notice_width > 0
        && notice_width < tab_area.width
        && tabs_width.saturating_add(1).saturating_add(notice_width) <= tab_area.width;

    if has_space_in_tab_bar {
        let notice_area = TuiRect {
            x: tab_area.x + tab_area.width - notice_width,
            y: tab_area.y,
            width: notice_width,
            height: tab_area.height,
        };
        Paragraph::new(notice.text.as_str())
            .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
            .render(notice_area, frame.buffer_mut());
        return;
    }

    let overlay_x = if full_area.width > 2 {
        full_area.x + 1
    } else {
        full_area.x
    };
    let overlay_y = if full_area.height > 1 {
        full_area.y + 1
    } else {
        full_area.y
    };
    let overlay_width = notice_width.min(full_area.width.saturating_sub(overlay_x - full_area.x));
    if overlay_width == 0 {
        return;
    }
    let overlay_area = TuiRect {
        x: overlay_x,
        y: overlay_y,
        width: overlay_width,
        height: 1,
    };
    Clear.render(overlay_area, frame.buffer_mut());
    Paragraph::new(notice.text.as_str())
        .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
        .render(overlay_area, frame.buffer_mut());
}

fn tab_bar_titles_width(frame_view: &FrameView) -> u16 {
    if frame_view.tabs.is_empty() {
        return 4;
    }

    let divider_width = TAB_DIVIDER.chars().count().min(u16::MAX as usize) as u16;
    let titles_width: u16 = frame_view
        .tabs
        .iter()
        .map(|tab| tab.title.chars().count().min(u16::MAX as usize) as u16)
        .sum();
    let divider_total = divider_width.saturating_mul(frame_view.tabs.len().saturating_sub(1) as u16);
    titles_width.saturating_add(divider_total)
}

fn render_pane(
    frame: &mut ratatui::Frame<'_>,
    pane: &PaneView,
    full_area: TuiRect,
    window_focused: bool,
) {
    let area = shift_and_clip_rect(pane.area, full_area);
    if area.width == 0 || area.height == 0 {
        return;
    }

    let border_style = if pane.active {
        active_pane_border_style(window_focused)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(pane.title.clone())
        .borders(Borders::ALL)
        .border_style(border_style);
    block.render(area, frame.buffer_mut());
    render_pane_content(frame, pane, area);
    render_selection_overlay(frame, pane, area);
    render_cursor_overlay(frame, pane, area);
}

fn active_tab_style(window_focused: bool) -> Style {
    if window_focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::White)
            .bg(Color::Red)
            .add_modifier(Modifier::BOLD)
    }
}

fn active_pane_border_style(window_focused: bool) -> Style {
    if window_focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    }
}

fn render_pane_content(frame: &mut ratatui::Frame<'_>, pane: &PaneView, area: TuiRect) {
    let content_x = area.x.saturating_add(1);
    let content_y = area.y.saturating_add(1);
    let content_width = area.width.saturating_sub(2);
    let content_height = area.height.saturating_sub(2);
    if content_width == 0 || content_height == 0 {
        return;
    }

    let buffer = frame.buffer_mut();

    for dy in 0..content_height {
        for dx in 0..content_width {
            let cell = &mut buffer[(content_x.saturating_add(dx), content_y.saturating_add(dy))];
            cell.set_symbol(" ");
            cell.set_style(Style::default());
        }
    }

    for (row_index, line) in pane.lines.iter().enumerate() {
        let row = row_index as u16;
        if row >= content_height {
            break;
        }

        for (col_index, cell) in line.cells.iter().enumerate() {
            let col = col_index as u16;
            if col >= content_width {
                break;
            }

            let symbol = if cell.has_contents {
                cell.text.as_str()
            } else {
                " "
            };

            let mut style = Style::default();
            if let Some(color) = to_tui_color(cell.fg) {
                style = style.fg(color);
            }
            if let Some(color) = to_tui_color(cell.bg) {
                style = style.bg(color);
            }
            if cell.bold {
                style = style.add_modifier(Modifier::BOLD);
            }
            if cell.dim {
                style = style.add_modifier(Modifier::DIM);
            }
            if cell.italic {
                style = style.add_modifier(Modifier::ITALIC);
            }
            if cell.underline {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            if cell.inverse {
                style = invert_style(style);
            }

            let buffer_cell =
                &mut buffer[(content_x.saturating_add(col), content_y.saturating_add(row))];
            buffer_cell.set_symbol(symbol);
            buffer_cell.set_style(style);
        }
    }
}

fn to_tui_color(color: ScreenColor) -> Option<Color> {
    match color {
        ScreenColor::Default => None,
        ScreenColor::Indexed(index) => Some(Color::Indexed(index)),
        ScreenColor::Rgb(r, g, b) => Some(Color::Rgb(r, g, b)),
    }
}

fn invert_style(style: Style) -> Style {
    let fg = style.fg;
    let bg = style.bg;
    match (fg, bg) {
        (Some(fg), Some(bg)) => style.fg(bg).bg(fg).add_modifier(Modifier::REVERSED),
        (Some(fg), None) => style
            .fg(Color::Reset)
            .bg(fg)
            .add_modifier(Modifier::REVERSED),
        (None, Some(bg)) => style
            .fg(bg)
            .bg(Color::Reset)
            .add_modifier(Modifier::REVERSED),
        (None, None) => style
            .fg(Color::Black)
            .bg(Color::Gray)
            .add_modifier(Modifier::REVERSED),
    }
}

fn shift_and_clip_rect(rect: Rect, full_area: TuiRect) -> TuiRect {
    let x = full_area.x.saturating_add(rect.x);
    let y = full_area
        .y
        .saturating_add(TAB_BAR_HEIGHT)
        .saturating_add(rect.y);
    let max_y = full_area.y.saturating_add(full_area.height);
    let max_x = full_area.x.saturating_add(full_area.width);

    if x >= max_x || y >= max_y {
        return TuiRect::new(x, y, 0, 0);
    }

    let width = rect.width.min(max_x.saturating_sub(x));
    let height = rect.height.min(max_y.saturating_sub(y));
    TuiRect::new(x, y, width, height)
}

fn render_selection_overlay(frame: &mut ratatui::Frame<'_>, pane: &PaneView, area: TuiRect) {
    let Some(selection) = &pane.selection else {
        return;
    };

    let content_x = area.x.saturating_add(1);
    let content_y = area.y.saturating_add(1);
    let content_width = area.width.saturating_sub(2);
    let content_height = area.height.saturating_sub(2);
    if content_width == 0 || content_height == 0 {
        return;
    }

    let buffer = frame.buffer_mut();
    for row in 0..content_height {
        for col in 0..content_width {
            if !selection_contains(selection, row, col) {
                continue;
            }
            let cell = &mut buffer[(content_x.saturating_add(col), content_y.saturating_add(row))];
            let style = cell.style();
            cell.set_style(style.bg(Color::DarkGray).add_modifier(Modifier::REVERSED));
        }
    }
}

fn selection_contains(selection: &PaneSelectionView, row: u16, col: u16) -> bool {
    let (start, end) = if selection.start <= selection.end {
        (selection.start, selection.end)
    } else {
        (selection.end, selection.start)
    };

    if row < start.0 || row > end.0 {
        return false;
    }
    if start.0 == end.0 {
        return row == start.0 && col >= start.1 && col <= end.1;
    }
    if row == start.0 {
        return col >= start.1;
    }
    if row == end.0 {
        return col <= end.1;
    }
    true
}

fn render_cursor_overlay(frame: &mut ratatui::Frame<'_>, pane: &PaneView, area: TuiRect) {
    if !pane.active {
        return;
    }

    let Some((row, mut col)) = pane.cursor else {
        return;
    };

    let content_x = area.x.saturating_add(1);
    let content_y = area.y.saturating_add(1);
    let content_width = area.width.saturating_sub(2);
    let content_height = area.height.saturating_sub(2);
    if content_width == 0 || content_height == 0 {
        return;
    }
    if row >= content_height || col >= content_width {
        return;
    }

    if pane
        .lines
        .get(row as usize)
        .and_then(|line| line.cells.get(col as usize))
        .is_some_and(|cell| cell.is_wide_continuation)
    {
        col = col.saturating_sub(1);
    }

    let x = content_x.saturating_add(col);
    let y = content_y.saturating_add(row);
    let buffer = frame.buffer_mut();
    let cell = &mut buffer[(x, y)];

    if cell.symbol().trim().is_empty() {
        cell.set_symbol(" ");
    }
    cell.set_bg(Color::White);
    cell.set_fg(Color::Blue);
    cell.set_style(
        Style::default()
            .bg(Color::White)
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED),
    );
}

fn render_modal(frame: &mut ratatui::Frame<'_>, modal: &ModalView, full_area: TuiRect) {
    let width = full_area.width.min(60).max(24);
    let height = 4.min(full_area.height);
    let x = full_area.x + full_area.width.saturating_sub(width) / 2;
    let y = full_area.y + full_area.height.saturating_sub(height) / 2;
    let area = TuiRect::new(x, y, width, height);

    Clear.render(area, frame.buffer_mut());
    let block = Block::default()
        .title(modal.title.clone())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    block.render(area, frame.buffer_mut());

    let inner = TuiRect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let input_area = TuiRect::new(inner.x, inner.y, inner.width, 1.min(inner.height));
    let (visible_input, visible_cursor) =
        visible_input_window(&modal.input, modal.cursor, input_area.width as usize);
    Paragraph::new(visible_input).render(input_area, frame.buffer_mut());

    if inner.height > 1 {
        let hint_area = TuiRect::new(inner.x, inner.y + 1, inner.width, 1);
        Paragraph::new(modal.hint.clone())
            .style(Style::default().fg(Color::DarkGray))
            .render(hint_area, frame.buffer_mut());
    }

    let cursor_col = visible_cursor as u16;
    if cursor_col < input_area.width {
        let cell = &mut frame.buffer_mut()[(
            input_area.x.saturating_add(cursor_col),
            input_area.y,
        )];
        if cell.symbol().trim().is_empty() {
            cell.set_symbol(" ");
        }
        cell.set_style(Style::default().fg(Color::Black).bg(Color::Yellow));
    }
}

fn visible_input_window(input: &str, cursor: usize, width: usize) -> (String, usize) {
    if width == 0 {
        return (String::new(), 0);
    }

    let chars: Vec<char> = input.chars().collect();
    if chars.len() <= width {
        return (input.to_owned(), cursor.min(chars.len()));
    }

    let cursor = cursor.min(chars.len());
    let mut start = cursor.saturating_sub(width.saturating_sub(1));
    if start + width > chars.len() {
        start = chars.len().saturating_sub(width);
    }
    let end = (start + width).min(chars.len());
    let visible: String = chars[start..end].iter().collect();
    (visible, cursor.saturating_sub(start))
}

#[cfg(test)]
mod tests;
