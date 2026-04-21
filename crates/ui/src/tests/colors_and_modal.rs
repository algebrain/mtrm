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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
