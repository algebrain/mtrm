//! Отрисовка интерфейса `mtrm`.

use std::io;

use mtrm_core::{PaneId, TabId};
use mtrm_layout::Rect;
use mtrm_terminal_screen::ScreenLine;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::layout::Rect as TuiRect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};

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
    pub cursor: Option<(u16, u16)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameView {
    pub tabs: Vec<TabView>,
    pub panes: Vec<PaneView>,
}

pub fn render_frame<B: Backend>(
    terminal: &mut Terminal<B>,
    frame_view: &FrameView,
) -> io::Result<()> {
    terminal.draw(|frame| {
        let full = frame.area();

        render_tabs(frame, frame_view, full);
        for pane in &frame_view.panes {
            render_pane(frame, pane, full);
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
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::raw(" "))
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

fn render_pane(frame: &mut ratatui::Frame<'_>, pane: &PaneView, full_area: TuiRect) {
    let area = shift_and_clip_rect(pane.area, full_area);
    if area.width == 0 || area.height == 0 {
        return;
    }

    let border_style = if pane.active {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(pane.title.clone())
        .borders(Borders::ALL)
        .border_style(border_style);
    let paragraph = Paragraph::new(Text::from(render_lines(pane))).block(block);
    frame.render_widget(paragraph, area);
    render_cursor_overlay(frame, pane, area);
}

fn render_lines(pane: &PaneView) -> Vec<Line<'static>> {
    pane.lines
        .iter()
        .enumerate()
        .map(|(row_index, line)| {
            let spans: Vec<Span<'static>> = line
                .cells
                .iter()
                .enumerate()
                .map(|(col_index, cell)| {
                    let mut style = Style::default();
                    if cell.bold {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    if cell.italic {
                        style = style.add_modifier(Modifier::ITALIC);
                    }
                    if cell.underline {
                        style = style.add_modifier(Modifier::UNDERLINED);
                    }
                    if cell.inverse {
                        style = style
                            .fg(Color::Black)
                            .bg(Color::Gray)
                            .add_modifier(Modifier::REVERSED);
                    }

                    if pane.active && pane.cursor == Some((row_index as u16, col_index as u16)) {
                        let has_visible_text = !cell.text.trim().is_empty();
                        let cursor_bg = Color::White;
                        let cursor_fg = if has_visible_text {
                            Color::Black
                        } else {
                            Color::Blue
                        };
                        style = style
                            .bg(cursor_bg)
                            .fg(cursor_fg)
                            .add_modifier(Modifier::BOLD | Modifier::REVERSED);
                    }

                    Span::styled(cell.text.clone(), style)
                })
                .collect();
            Line::from(spans)
        })
        .collect()
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

fn render_cursor_overlay(frame: &mut ratatui::Frame<'_>, pane: &PaneView, area: TuiRect) {
    if !pane.active {
        return;
    }

    let Some((row, col)) = pane.cursor else {
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn render(frame_view: &FrameView, width: u16, height: u16) -> Terminal<TestBackend> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        render_frame(&mut terminal, frame_view).unwrap();
        terminal
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
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            })
                            .collect(),
                    }],
                    cursor: None,
                }],
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
                        cursor: None,
                    },
                ],
            },
            25,
            10,
        );

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(0, 1)].style().fg, Some(Color::Yellow));
        assert_eq!(buffer[(10, 1)].style().fg, Some(Color::DarkGray));
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
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            })
                            .collect(),
                    }],
                    cursor: None,
                }],
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
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            })
                            .collect(),
                    }],
                    cursor: Some((0, 1)),
                }],
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
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            },
                            mtrm_terminal_screen::ScreenCell {
                                text: " ".to_owned(),
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            },
                        ],
                    }],
                    cursor: Some((0, 1)),
                }],
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
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            })
                            .collect(),
                    }],
                    cursor: Some((0, 2)),
                }],
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
                                bold: false,
                                italic: false,
                                underline: false,
                                inverse: false,
                            })
                            .collect(),
                    }],
                    cursor: Some((0, 1)),
                }],
            },
            20,
            8,
        );

        let buffer = terminal.backend().buffer();
        let cell = &buffer[(2, 2)];
        assert_ne!(cell.style().bg, Some(Color::Yellow));
    }
}
