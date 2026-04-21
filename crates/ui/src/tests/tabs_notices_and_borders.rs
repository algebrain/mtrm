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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
fn renders_clipboard_notice_at_right_edge_of_tab_bar_when_there_is_space() {
    let terminal = render(
        &FrameView {
            tabs: vec![TabView {
                id: TabId::new(1),
                title: "main".to_owned(),
                active: true,
            }],
            panes: vec![],
            focused: true,
            clipboard_notice: Some(ClipboardNoticeView {
                text: "Clipboard is unavailable".to_owned(),
            }),
            modal: None,
        },
        60,
        4,
    );

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(59, 0)].symbol(), "e");
}

#[test]
fn renders_clipboard_notice_as_overlay_when_tab_bar_is_too_narrow() {
    let terminal = render(
        &FrameView {
            tabs: vec![TabView {
                id: TabId::new(1),
                title: "very-wide-active-tab".to_owned(),
                active: true,
            }],
            panes: vec![PaneView {
                id: PaneId::new(1),
                title: "pane".to_owned(),
                area: Rect {
                    x: 0,
                    y: 0,
                    width: 20,
                    height: 5,
                },
                active: true,
                lines: vec![],
                selection: None,
                cursor: None,
            }],
            focused: true,
            clipboard_notice: Some(ClipboardNoticeView {
                text: "Clipboard is unavailable".to_owned(),
            }),
            modal: None,
        },
        20,
        8,
    );

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(1, 1)].symbol(), "C");
}

#[test]
fn renders_generic_error_notice_as_overlay_when_tab_bar_is_too_narrow() {
    let terminal = render(
        &FrameView {
            tabs: vec![TabView {
                id: TabId::new(1),
                title: "very-wide-active-tab".to_owned(),
                active: true,
            }],
            panes: vec![PaneView {
                id: PaneId::new(1),
                title: "pane".to_owned(),
                area: Rect {
                    x: 0,
                    y: 0,
                    width: 20,
                    height: 5,
                },
                active: true,
                lines: vec![],
                selection: None,
                cursor: None,
            }],
            focused: true,
            clipboard_notice: Some(ClipboardNoticeView {
                text: "Failed to save state".to_owned(),
            }),
            modal: None,
        },
        20,
        8,
    );

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(1, 1)].symbol(), "F");
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
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
            clipboard_notice: None,
            modal: None,
        },
        20,
        8,
    );

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 1)].style().fg, Some(Color::Red));
}
