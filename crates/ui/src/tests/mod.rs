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

include!("tabs_notices_and_borders.rs");
include!("selection_and_cursor.rs");
include!("redraw_and_wide_chars.rs");
include!("colors_and_modal.rs");
