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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
            modal: None,
        },
        30,
        8,
    );

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(4, 2)].symbol(), "б");
    assert_eq!(buffer[(5, 2)].style().bg, Some(Color::White));
}
