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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
