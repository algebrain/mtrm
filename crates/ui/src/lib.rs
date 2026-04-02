//! Отрисовка интерфейса `mtrm`.

use std::io;

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
    pub modal: Option<ModalView>,
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
) -> io::Result<()> {
    terminal.draw(|frame| {
        let full = frame.area();

        render_tabs(frame, frame_view, full);
        for pane in &frame_view.panes {
            render_pane(frame, pane, full, frame_view.focused);
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
        .divider(Span::styled(" | ", Style::default().fg(Color::DarkGray)))
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
mod tests {
    use super::*;
    use mtrm_terminal_screen::TerminalScreen;
    use ratatui::backend::TestBackend;

    fn render(frame_view: &FrameView, width: u16, height: u16) -> Terminal<TestBackend> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        render_frame(&mut terminal, frame_view).unwrap();
        terminal
    }

    fn bytes_with_gap(left: &str, gap_cols: u16, right: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(left.as_bytes());
        bytes.extend_from_slice(format!("\x1b[{}C", gap_cols).as_bytes());
        bytes.extend_from_slice(right.as_bytes());
        bytes
    }

    fn pane_from_screen(screen: &TerminalScreen) -> PaneView {
        PaneView {
            id: PaneId::new(1),
            title: "pane".to_owned(),
            area: Rect {
                x: 0,
                y: 0,
                width: 20,
                height: 5,
            },
            active: true,
            lines: screen.visible_lines(),
            selection: None,
            cursor: Some(screen.cursor_position()),
        }
    }

    fn pane_from_screen_at(
        pane_id: u64,
        title: &str,
        area: Rect,
        active: bool,
        screen: &TerminalScreen,
    ) -> PaneView {
        PaneView {
            id: PaneId::new(pane_id),
            title: title.to_owned(),
            area,
            active,
            lines: screen.visible_lines(),
            selection: None,
            cursor: if active {
                Some(screen.cursor_position())
            } else {
                None
            },
        }
    }

    #[test]
    fn renders_single_tab_and_single_pane() {
        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![PaneView {
                    id: PaneId::new(1),
                    title: "pane-1".to_owned(),
                    area: Rect {
                        x: 0,
                        y: 0,
                        width: 20,
                        height: 5,
                    },
                    active: true,
                    lines: vec![ScreenLine {
                        cells: "hello"
                            .chars()
                            .map(|ch| mtrm_terminal_screen::ScreenCell {
                                text: ch.to_string(),
                                has_contents: true,
                                is_wide: false,
                                is_wide_continuation: false,
                                fg: ScreenColor::Default,
                                bg: ScreenColor::Default,
                                dim: false,
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            })
                            .collect(),
                    }],
                    selection: None,
                    cursor: None,
                }],
                focused: true,
                modal: None,
            },
            30,
            10,
        );

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(0, 0)].symbol(), "m");
        assert_eq!(buffer[(0, 1)].symbol(), "┌");
        assert_eq!(buffer[(1, 2)].symbol(), "h");
    }

    #[test]
    fn renders_multiple_tabs_titles() {
        let terminal = render(
            &FrameView {
                tabs: vec![
                    TabView {
                        id: TabId::new(1),
                        title: "one".to_owned(),
                        active: false,
                    },
                    TabView {
                        id: TabId::new(2),
                        title: "two".to_owned(),
                        active: true,
                    },
                ],
                panes: vec![],
                focused: true,
                modal: None,
            },
            20,
            5,
        );

        let line: String = (0..20)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
            .collect();
        assert!(line.contains("one"));
        assert!(line.contains("two"));
    }

    #[test]
    fn tab_divider_uses_dark_gray_style() {
        let terminal = render(
            &FrameView {
                tabs: vec![
                    TabView {
                        id: TabId::new(1),
                        title: "one".to_owned(),
                        active: false,
                    },
                    TabView {
                        id: TabId::new(2),
                        title: "two".to_owned(),
                        active: true,
                    },
                ],
                panes: vec![],
                focused: true,
                modal: None,
            },
            20,
            5,
        );

        let buffer = terminal.backend().buffer();
        let divider_x = (0..20)
            .find(|x| buffer[(*x, 0)].symbol() == "|")
            .expect("expected divider between tab titles");

        assert_eq!(buffer[(divider_x, 0)].style().fg, Some(Color::DarkGray));
    }

    #[test]
    fn highlights_active_tab() {
        let terminal = render(
            &FrameView {
                tabs: vec![
                    TabView {
                        id: TabId::new(1),
                        title: "one".to_owned(),
                        active: false,
                    },
                    TabView {
                        id: TabId::new(2),
                        title: "two".to_owned(),
                        active: true,
                    },
                ],
                panes: vec![],
                focused: true,
                modal: None,
            },
            20,
            5,
        );

        let buffer = terminal.backend().buffer();
        let highlighted = (0..20).any(|x| {
            let cell = &buffer[(x, 0)];
            cell.style().bg == Some(Color::Yellow)
        });
        assert!(highlighted);
    }

    #[test]
    fn highlights_active_tab_in_red_when_window_is_unfocused() {
        let terminal = render(
            &FrameView {
                tabs: vec![
                    TabView {
                        id: TabId::new(1),
                        title: "one".to_owned(),
                        active: false,
                    },
                    TabView {
                        id: TabId::new(2),
                        title: "two".to_owned(),
                        active: true,
                    },
                ],
                panes: vec![],
                focused: false,
                modal: None,
            },
            20,
            5,
        );

        let buffer = terminal.backend().buffer();
        let highlighted = (0..20).any(|x| {
            let cell = &buffer[(x, 0)];
            cell.style().bg == Some(Color::Red)
        });
        assert!(highlighted);
    }

    #[test]
    fn highlights_active_pane_border() {
        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![
                    PaneView {
                        id: PaneId::new(1),
                        title: "active".to_owned(),
                        area: Rect {
                            x: 0,
                            y: 0,
                            width: 10,
                            height: 5,
                        },
                        active: true,
                        lines: vec![],
                        selection: None,
                        cursor: None,
                    },
                    PaneView {
                        id: PaneId::new(2),
                        title: "inactive".to_owned(),
                        area: Rect {
                            x: 10,
                            y: 0,
                            width: 10,
                            height: 5,
                        },
                        active: false,
                        lines: vec![],
                        selection: None,
                        cursor: None,
                    },
                ],
                focused: true,
                modal: None,
            },
            25,
            10,
        );

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(0, 1)].style().fg, Some(Color::Yellow));
        assert_eq!(buffer[(10, 1)].style().fg, Some(Color::DarkGray));
    }

    #[test]
    fn highlights_active_pane_border_in_red_when_window_is_unfocused() {
        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![PaneView {
                    id: PaneId::new(1),
                    title: "active".to_owned(),
                    area: Rect {
                        x: 0,
                        y: 0,
                        width: 10,
                        height: 5,
                    },
                    active: true,
                    lines: vec![],
                    selection: None,
                    cursor: None,
                }],
                focused: false,
                modal: None,
            },
            20,
            8,
        );

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(0, 1)].style().fg, Some(Color::Red));
    }

    #[test]
    fn highlights_selected_cells_inside_pane() {
        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![PaneView {
                    id: PaneId::new(1),
                    title: "pane".to_owned(),
                    area: Rect {
                        x: 0,
                        y: 0,
                        width: 12,
                        height: 5,
                    },
                    active: true,
                    lines: vec![ScreenLine {
                        cells: "abcd"
                            .chars()
                            .map(|ch| mtrm_terminal_screen::ScreenCell {
                                text: ch.to_string(),
                                has_contents: true,
                                is_wide: false,
                                is_wide_continuation: false,
                                fg: ScreenColor::Default,
                                bg: ScreenColor::Default,
                                dim: false,
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            })
                            .collect(),
                    }],
                    selection: Some(PaneSelectionView {
                        start: (0, 1),
                        end: (0, 2),
                    }),
                    cursor: None,
                }],
                focused: true,
                modal: None,
            },
            20,
            8,
        );

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(2, 2)].style().bg, Some(Color::DarkGray));
        assert_eq!(buffer[(3, 2)].style().bg, Some(Color::DarkGray));
        assert_ne!(buffer[(1, 2)].style().bg, Some(Color::DarkGray));
    }

    #[test]
    fn places_pane_text_inside_pane_area() {
        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![PaneView {
                    id: PaneId::new(1),
                    title: "pane".to_owned(),
                    area: Rect {
                        x: 2,
                        y: 1,
                        width: 12,
                        height: 5,
                    },
                    active: true,
                    lines: vec![ScreenLine {
                        cells: "abc"
                            .chars()
                            .map(|ch| mtrm_terminal_screen::ScreenCell {
                                text: ch.to_string(),
                                has_contents: true,
                                is_wide: false,
                                is_wide_continuation: false,
                                fg: ScreenColor::Default,
                                bg: ScreenColor::Default,
                                dim: false,
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            })
                            .collect(),
                    }],
                    selection: None,
                    cursor: None,
                }],
                focused: true,
                modal: None,
            },
            30,
            12,
        );

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(3, 3)].symbol(), "a");
        assert_eq!(buffer[(4, 3)].symbol(), "b");
        assert_eq!(buffer[(5, 3)].symbol(), "c");
    }

    #[test]
    fn highlights_cursor_cell_in_active_pane() {
        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![PaneView {
                    id: PaneId::new(1),
                    title: "pane".to_owned(),
                    area: Rect {
                        x: 0,
                        y: 0,
                        width: 12,
                        height: 5,
                    },
                    active: true,
                    lines: vec![ScreenLine {
                        cells: "ab"
                            .chars()
                            .map(|ch| mtrm_terminal_screen::ScreenCell {
                                text: ch.to_string(),
                                has_contents: true,
                                is_wide: false,
                                is_wide_continuation: false,
                                fg: ScreenColor::Default,
                                bg: ScreenColor::Default,
                                dim: false,
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            })
                            .collect(),
                    }],
                    selection: None,
                    cursor: Some((0, 1)),
                }],
                focused: true,
                modal: None,
            },
            20,
            8,
        );

        let buffer = terminal.backend().buffer();
        let cell = &buffer[(2, 2)];
        assert_eq!(cell.symbol(), "b");
        assert_eq!(cell.style().bg, Some(Color::White));
    }

    #[test]
    fn renders_visible_cursor_on_empty_cell() {
        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![PaneView {
                    id: PaneId::new(1),
                    title: "pane".to_owned(),
                    area: Rect {
                        x: 0,
                        y: 0,
                        width: 12,
                        height: 5,
                    },
                    active: true,
                    lines: vec![ScreenLine {
                        cells: vec![
                            mtrm_terminal_screen::ScreenCell {
                                text: "a".to_owned(),
                                has_contents: true,
                                is_wide: false,
                                is_wide_continuation: false,
                                fg: ScreenColor::Default,
                                bg: ScreenColor::Default,
                                dim: false,
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            },
                            mtrm_terminal_screen::ScreenCell {
                                text: " ".to_owned(),
                                has_contents: true,
                                is_wide: false,
                                is_wide_continuation: false,
                                fg: ScreenColor::Default,
                                bg: ScreenColor::Default,
                                dim: false,
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            },
                        ],
                    }],
                    selection: None,
                    cursor: Some((0, 1)),
                }],
                focused: true,
                modal: None,
            },
            20,
            8,
        );

        let buffer = terminal.backend().buffer();
        let cell = &buffer[(2, 2)];
        assert_ne!(cell.style().bg, Some(Color::Reset));
    }

    #[test]
    fn renders_cursor_after_end_of_line() {
        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![PaneView {
                    id: PaneId::new(1),
                    title: "pane".to_owned(),
                    area: Rect {
                        x: 0,
                        y: 0,
                        width: 12,
                        height: 5,
                    },
                    active: true,
                    lines: vec![ScreenLine {
                        cells: "ab"
                            .chars()
                            .map(|ch| mtrm_terminal_screen::ScreenCell {
                                text: ch.to_string(),
                                has_contents: true,
                                is_wide: false,
                                is_wide_continuation: false,
                                fg: ScreenColor::Default,
                                bg: ScreenColor::Default,
                                dim: false,
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            })
                            .collect(),
                    }],
                    selection: None,
                    cursor: Some((0, 2)),
                }],
                focused: true,
                modal: None,
            },
            20,
            8,
        );

        let buffer = terminal.backend().buffer();
        let cell = &buffer[(3, 2)];
        assert_eq!(cell.symbol(), " ");
        assert_eq!(cell.style().bg, Some(Color::White));
    }

    #[test]
    fn does_not_draw_cursor_in_inactive_pane() {
        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![PaneView {
                    id: PaneId::new(1),
                    title: "pane".to_owned(),
                    area: Rect {
                        x: 0,
                        y: 0,
                        width: 12,
                        height: 5,
                    },
                    active: false,
                    lines: vec![ScreenLine {
                        cells: "ab"
                            .chars()
                            .map(|ch| mtrm_terminal_screen::ScreenCell {
                                text: ch.to_string(),
                                has_contents: true,
                                is_wide: false,
                                is_wide_continuation: false,
                                fg: ScreenColor::Default,
                                bg: ScreenColor::Default,
                                dim: false,
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            })
                            .collect(),
                    }],
                    selection: None,
                    cursor: Some((0, 1)),
                }],
                focused: true,
                modal: None,
            },
            20,
            8,
        );

        let buffer = terminal.backend().buffer();
        let cell = &buffer[(2, 2)];
        assert_ne!(cell.style().bg, Some(Color::Yellow));
    }

    #[test]
    fn renders_terminal_cell_gaps_between_cyrillic_characters() {
        let mut screen = TerminalScreen::new(3, 10, 0);
        screen.process_bytes(&bytes_with_gap("я", 2, "б"));

        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![pane_from_screen(&screen)],
                focused: true,
                modal: None,
            },
            30,
            8,
        );

        let buffer = terminal.backend().buffer();
        let row: String = (1..8).map(|x| buffer[(x, 2)].symbol()).collect();

        assert!(
            row.starts_with("я  б"),
            "expected visible gap cells, got {row:?}"
        );
    }

    #[test]
    fn cursor_respects_terminal_cell_gaps() {
        let mut screen = TerminalScreen::new(3, 10, 0);
        screen.process_bytes(&bytes_with_gap("я", 2, "б"));

        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![pane_from_screen(&screen)],
                focused: true,
                modal: None,
            },
            30,
            8,
        );

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(4, 2)].symbol(), "б");
        assert_eq!(buffer[(5, 2)].style().bg, Some(Color::White));
    }

    #[test]
    fn redraw_clears_previous_panel_content() {
        let backend = TestBackend::new(30, 8);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut first = TerminalScreen::new(3, 10, 0);
        first.process_bytes(b"abcdef");
        render_frame(
            &mut terminal,
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![pane_from_screen(&first)],
                focused: true,
                modal: None,
            },
        )
        .unwrap();

        let mut second = TerminalScreen::new(3, 10, 0);
        second.process_bytes(b"ab");
        render_frame(
            &mut terminal,
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![pane_from_screen(&second)],
                focused: true,
                modal: None,
            },
        )
        .unwrap();

        let buffer = terminal.backend().buffer();
        let row: String = (1..8).map(|x| buffer[(x, 2)].symbol()).collect();

        assert!(
            row.starts_with("ab     "),
            "stale content remained in row: {row:?}"
        );
    }

    #[test]
    fn redraw_clears_trailing_text_in_left_pane_after_split_frame_change() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut left_first = TerminalScreen::new(4, 16, 0);
        left_first.process_bytes(b"left pane had a much longer line");
        let right_empty = TerminalScreen::new(4, 16, 0);
        render_frame(
            &mut terminal,
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![
                    pane_from_screen_at(
                        1,
                        "left",
                        Rect {
                            x: 0,
                            y: 0,
                            width: 20,
                            height: 6,
                        },
                        true,
                        &left_first,
                    ),
                    pane_from_screen_at(
                        2,
                        "right",
                        Rect {
                            x: 20,
                            y: 0,
                            width: 20,
                            height: 6,
                        },
                        false,
                        &right_empty,
                    ),
                ],
                focused: true,
                modal: None,
            },
        )
        .unwrap();

        let mut left_second = TerminalScreen::new(4, 16, 0);
        left_second.process_bytes(b"short");
        render_frame(
            &mut terminal,
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![
                    pane_from_screen_at(
                        1,
                        "left",
                        Rect {
                            x: 0,
                            y: 0,
                            width: 20,
                            height: 6,
                        },
                        true,
                        &left_second,
                    ),
                    pane_from_screen_at(
                        2,
                        "right",
                        Rect {
                            x: 20,
                            y: 0,
                            width: 20,
                            height: 6,
                        },
                        false,
                        &right_empty,
                    ),
                ],
                focused: true,
                modal: None,
            },
        )
        .unwrap();

        let buffer = terminal.backend().buffer();
        let left_row: String = (1..18).map(|x| buffer[(x, 2)].symbol()).collect();

        assert!(
            left_row.starts_with("short"),
            "updated left pane prefix missing: {left_row:?}"
        );
        assert!(
            !left_row.contains("much longer"),
            "stale left-pane text remained after redraw: {left_row:?}"
        );
    }

    #[test]
    fn redraw_does_not_leak_old_left_pane_text_into_right_pane() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut left_first = TerminalScreen::new(4, 16, 0);
        left_first.process_bytes(b"left stale text");
        let mut right_first = TerminalScreen::new(4, 16, 0);
        right_first.process_bytes(b"right");
        render_frame(
            &mut terminal,
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![
                    pane_from_screen_at(
                        1,
                        "left",
                        Rect {
                            x: 0,
                            y: 0,
                            width: 20,
                            height: 6,
                        },
                        true,
                        &left_first,
                    ),
                    pane_from_screen_at(
                        2,
                        "right",
                        Rect {
                            x: 20,
                            y: 0,
                            width: 20,
                            height: 6,
                        },
                        false,
                        &right_first,
                    ),
                ],
                focused: true,
                modal: None,
            },
        )
        .unwrap();

        let left_second = TerminalScreen::new(4, 16, 0);
        let mut right_second = TerminalScreen::new(4, 16, 0);
        right_second.process_bytes(b"ok");
        render_frame(
            &mut terminal,
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![
                    pane_from_screen_at(
                        1,
                        "left",
                        Rect {
                            x: 0,
                            y: 0,
                            width: 20,
                            height: 6,
                        },
                        true,
                        &left_second,
                    ),
                    pane_from_screen_at(
                        2,
                        "right",
                        Rect {
                            x: 20,
                            y: 0,
                            width: 20,
                            height: 6,
                        },
                        false,
                        &right_second,
                    ),
                ],
                focused: true,
                modal: None,
            },
        )
        .unwrap();

        let buffer = terminal.backend().buffer();
        let right_row: String = (21..38).map(|x| buffer[(x, 2)].symbol()).collect();

        assert!(
            right_row.starts_with("ok"),
            "right pane lost fresh text: {right_row:?}"
        );
        assert!(
            !right_row.contains("left"),
            "left-pane text leaked into right pane row: {right_row:?}"
        );
    }

    #[test]
    fn redraw_replaces_cyrillic_line_without_leaving_old_suffix() {
        let backend = TestBackend::new(30, 8);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut first = TerminalScreen::new(3, 10, 0);
        first.process_bytes("я запускаю".as_bytes());
        render_frame(
            &mut terminal,
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![pane_from_screen(&first)],
                focused: true,
                modal: None,
            },
        )
        .unwrap();

        let mut second = TerminalScreen::new(3, 10, 0);
        second.process_bytes("я".as_bytes());
        render_frame(
            &mut terminal,
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![pane_from_screen(&second)],
                focused: true,
                modal: None,
            },
        )
        .unwrap();

        let buffer = terminal.backend().buffer();
        let row: String = (1..8).map(|x| buffer[(x, 2)].symbol()).collect();

        assert!(row.starts_with("я"), "new cyrillic prefix missing: {row:?}");
        assert!(
            !row.contains("запускаю"),
            "old cyrillic suffix remained after redraw: {row:?}"
        );
    }

    #[test]
    fn cursor_on_wide_char_continuation_is_drawn_on_leading_cell() {
        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![PaneView {
                    id: PaneId::new(1),
                    title: "pane".to_owned(),
                    area: Rect {
                        x: 0,
                        y: 0,
                        width: 12,
                        height: 5,
                    },
                    active: true,
                    lines: vec![ScreenLine {
                        cells: vec![
                            mtrm_terminal_screen::ScreenCell {
                                text: "界".to_owned(),
                                has_contents: true,
                                is_wide: true,
                                is_wide_continuation: false,
                                fg: ScreenColor::Default,
                                bg: ScreenColor::Default,
                                dim: false,
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            },
                            mtrm_terminal_screen::ScreenCell {
                                text: "".to_owned(),
                                has_contents: false,
                                is_wide: false,
                                is_wide_continuation: true,
                                fg: ScreenColor::Default,
                                bg: ScreenColor::Default,
                                dim: false,
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            },
                            mtrm_terminal_screen::ScreenCell {
                                text: "a".to_owned(),
                                has_contents: true,
                                is_wide: false,
                                is_wide_continuation: false,
                                fg: ScreenColor::Default,
                                bg: ScreenColor::Default,
                                dim: false,
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            },
                        ],
                    }],
                    selection: None,
                    cursor: Some((0, 1)),
                }],
                focused: true,
                modal: None,
            },
            20,
            8,
        );

        let buffer = terminal.backend().buffer();
        let leading = &buffer[(1, 2)];
        let continuation = &buffer[(2, 2)];

        assert_eq!(leading.symbol(), "界");
        assert_eq!(leading.style().bg, Some(Color::White));
        assert_eq!(continuation.symbol(), " ");
        assert_ne!(continuation.style().bg, Some(Color::White));
    }

    #[test]
    fn renders_terminal_background_color_from_screen_cells() {
        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![PaneView {
                    id: PaneId::new(1),
                    title: "pane".to_owned(),
                    area: Rect {
                        x: 0,
                        y: 0,
                        width: 12,
                        height: 5,
                    },
                    active: true,
                    lines: vec![ScreenLine {
                        cells: vec![mtrm_terminal_screen::ScreenCell {
                            text: "x".to_owned(),
                            has_contents: true,
                            is_wide: false,
                            is_wide_continuation: false,
                            fg: ScreenColor::Indexed(1),
                            bg: ScreenColor::Indexed(7),
                            dim: false,
                            bold: false,
                            italic: false,
                            underline: false,
                            inverse: false,
                        }],
                    }],
                    selection: None,
                    cursor: None,
                }],
                focused: true,
                modal: None,
            },
            20,
            8,
        );

        let buffer = terminal.backend().buffer();
        let cell = &buffer[(1, 2)];

        assert_eq!(cell.symbol(), "x");
        assert_eq!(cell.style().fg, Some(Color::Indexed(1)));
        assert_eq!(cell.style().bg, Some(Color::Indexed(7)));
    }

    #[test]
    fn renders_ansi_background_color_end_to_end_from_terminal_screen() {
        let mut screen = TerminalScreen::new(3, 10, 0);
        screen.process_bytes(b"\x1b[31;47mAB\x1b[m");

        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![PaneView {
                    id: PaneId::new(1),
                    title: "pane".to_owned(),
                    area: Rect {
                        x: 0,
                        y: 0,
                        width: 12,
                        height: 5,
                    },
                    active: true,
                    lines: screen.visible_lines(),
                    selection: None,
                    cursor: None,
                }],
                focused: true,
                modal: None,
            },
            20,
            8,
        );

        let buffer = terminal.backend().buffer();
        let first = &buffer[(1, 2)];
        let second = &buffer[(2, 2)];

        assert_eq!(first.symbol(), "A");
        assert_eq!(first.style().bg, Some(Color::Indexed(7)));
        assert_eq!(second.symbol(), "B");
        assert_eq!(second.style().bg, Some(Color::Indexed(7)));
    }

    #[test]
    fn inverse_ansi_swaps_terminal_cell_colors_end_to_end() {
        let mut screen = TerminalScreen::new(3, 10, 0);
        screen.process_bytes(b"\x1b[31;47;7mA\x1b[m");

        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![PaneView {
                    id: PaneId::new(1),
                    title: "pane".to_owned(),
                    area: Rect {
                        x: 0,
                        y: 0,
                        width: 12,
                        height: 5,
                    },
                    active: true,
                    lines: screen.visible_lines(),
                    selection: None,
                    cursor: None,
                }],
                focused: true,
                modal: None,
            },
            20,
            8,
        );

        let buffer = terminal.backend().buffer();
        let cell = &buffer[(1, 2)];

        assert_eq!(cell.symbol(), "A");
        assert_eq!(cell.style().fg, Some(Color::Indexed(7)));
        assert_eq!(cell.style().bg, Some(Color::Indexed(1)));
    }

    #[test]
    fn renders_ansi_dim_modifier_end_to_end_from_terminal_screen() {
        let mut screen = TerminalScreen::new(3, 10, 0);
        screen.process_bytes(b"\x1b[2mA\x1b[m");

        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![PaneView {
                    id: PaneId::new(1),
                    title: "pane".to_owned(),
                    area: Rect {
                        x: 0,
                        y: 0,
                        width: 12,
                        height: 5,
                    },
                    active: true,
                    lines: screen.visible_lines(),
                    selection: None,
                    cursor: None,
                }],
                focused: true,
                modal: None,
            },
            20,
            8,
        );

        let buffer = terminal.backend().buffer();
        let cell = &buffer[(1, 2)];

        assert_eq!(cell.symbol(), "A");
        assert!(
            cell.style().add_modifier.contains(Modifier::DIM),
            "expected DIM modifier, got {:?}",
            cell.style()
        );
    }

    #[test]
    fn renders_ansi_dimmed_background_highlight_end_to_end() {
        let mut screen = TerminalScreen::new(3, 10, 0);
        screen.process_bytes(b"\x1b[2;100mA\x1b[m");

        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![PaneView {
                    id: PaneId::new(1),
                    title: "pane".to_owned(),
                    area: Rect {
                        x: 0,
                        y: 0,
                        width: 12,
                        height: 5,
                    },
                    active: true,
                    lines: screen.visible_lines(),
                    selection: None,
                    cursor: None,
                }],
                focused: true,
                modal: None,
            },
            20,
            8,
        );

        let buffer = terminal.backend().buffer();
        let cell = &buffer[(1, 2)];

        assert_eq!(cell.symbol(), "A");
        assert_eq!(cell.style().bg, Some(Color::Indexed(8)));
        assert!(
            cell.style().add_modifier.contains(Modifier::DIM),
            "expected DIM modifier with background highlight, got {:?}",
            cell.style()
        );
    }

    #[test]
    fn renders_centered_modal_overlay() {
        let terminal = render(
            &FrameView {
                tabs: vec![TabView {
                    id: TabId::new(1),
                    title: "main".to_owned(),
                    active: true,
                }],
                panes: vec![],
                focused: true,
                modal: Some(ModalView {
                    title: "Rename Tab".to_owned(),
                    input: "build".to_owned(),
                    cursor: 2,
                    hint: "Enter apply, Esc cancel".to_owned(),
                }),
            },
            40,
            12,
        );

        let buffer = terminal.backend().buffer();
        let title_line: String = (0..40).map(|x| buffer[(x, 4)].symbol()).collect();
        let input_line: String = (1..39).map(|x| buffer[(x, 5)].symbol()).collect();
        let hint_line: String = (1..39).map(|x| buffer[(x, 6)].symbol()).collect();

        assert!(title_line.contains("Rename Tab"));
        assert!(input_line.contains("build"));
        assert!(hint_line.contains("Enter apply"));
        assert_eq!(buffer[(3, 5)].style().bg, Some(Color::Yellow));
    }

    #[test]
    fn visible_input_window_keeps_cursor_in_view() {
        let (visible, cursor) =
            visible_input_window("abcdefghijklmnopqrstuvwxyz0123456789", 36, 22);

        assert!(visible.ends_with("6789"));
        assert_eq!(cursor, 22);
    }
}
