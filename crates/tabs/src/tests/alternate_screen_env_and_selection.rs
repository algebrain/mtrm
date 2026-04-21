#[test]
fn active_pane_screen_tracks_alternate_mode_for_fullscreen_sequences() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
    let pane_id = manager.active_pane_id();

    {
        let pane = manager.active_tab_mut().panes.get_mut(&pane_id).unwrap();
        pane.screen.process_bytes(b"shell$ prompt");
        pane.screen
            .process_bytes(b"\x1b[?1049h\x1b[2J\x1b[Hcodex frame");
    }

    let pane = manager.find_pane(pane_id).unwrap();
    assert_eq!(
        pane.screen.screen_mode(),
        mtrm_terminal_screen::ScreenMode::Alternate
    );
    assert!(pane.screen.visible_rows()[0].contains("codex frame"));

    {
        let pane = manager.active_tab_mut().panes.get_mut(&pane_id).unwrap();
        pane.screen.process_bytes(b"\x1b[?1049l");
    }

    let pane = manager.find_pane(pane_id).unwrap();
    assert_eq!(
        pane.screen.screen_mode(),
        mtrm_terminal_screen::ScreenMode::Normal
    );
    assert!(pane.screen.visible_rows()[0].contains("shell$ prompt"));
}

#[test]
fn scrolling_fullscreen_pane_uses_alternate_snapshot_history() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
    let pane_id = manager.active_pane_id();

    {
        let pane = manager.active_tab_mut().panes.get_mut(&pane_id).unwrap();
        pane.screen.process_bytes(b"\x1b[?1049h");
        pane.screen.process_bytes(b"\x1b[2J\x1b[Hframe1");
        pane.screen.process_bytes(b"\x1b[2J\x1b[Hframe2");
        pane.screen.process_bytes(b"\x1b[2J\x1b[Hframe3");
    }

    let live = manager.active_pane_text().unwrap();
    assert!(live.contains("frame3"));

    manager.scroll_active_pane_up_lines(1).unwrap();
    let previous = manager.active_pane_text().unwrap();
    assert!(previous.contains("frame2"));

    manager.scroll_active_pane_to_bottom().unwrap();
    let bottom = manager.active_pane_text().unwrap();
    assert!(bottom.contains("frame3"));
}

#[test]
fn active_pane_scrollback_state_tracks_alternate_history_snapshot() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
    let pane_id = manager.active_pane_id();

    {
        let pane = manager.active_tab_mut().panes.get_mut(&pane_id).unwrap();
        pane.screen.process_bytes(b"\x1b[?1049h");
        pane.screen.process_bytes(b"\x1b[2J\x1b[Hframe1");
        pane.screen.process_bytes(b"\x1b[2J\x1b[Hframe2");
    }

    assert!(!manager.active_pane_is_scrolled_back().unwrap());

    manager.scroll_active_pane_up_lines(1).unwrap();

    assert!(manager.active_pane_is_scrolled_back().unwrap());
    assert!(manager.active_pane_text().unwrap().contains("frame1"));
}

#[test]
fn pane_cursor_is_hidden_when_alternate_history_snapshot_is_visible() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
    let pane_id = manager.active_pane_id();

    {
        let pane = manager.active_tab_mut().panes.get_mut(&pane_id).unwrap();
        pane.screen.process_bytes(b"\x1b[?1049h");
        pane.screen.process_bytes(b"\x1b[2J\x1b[Hframe1");
        pane.screen.process_bytes(b"\x1b[2J\x1b[Hframe2");
    }

    assert!(manager.pane_cursor(pane_id).unwrap().is_some());

    manager.scroll_active_pane_up_lines(1).unwrap();

    assert!(manager.active_pane_text().unwrap().contains("frame1"));
    assert_eq!(manager.pane_cursor(pane_id).unwrap(), None);
}

#[test]
fn scrolling_normal_screen_decstbm_history_shows_previous_frame_instead_of_mixed_rows() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
    let pane_id = manager.active_pane_id();

    let frame = |frame_label: &str, footer_label: &str| {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\x1b[2J\x1b[H");
        bytes.extend_from_slice(b"hist1\r\nhist2\r\nhist3\r\nhist4\r\n");
        bytes.extend_from_slice(footer_label.as_bytes());
        bytes.extend_from_slice(b"\x1b[1;4r");
        bytes.extend_from_slice(b"\x1b[4;1H\r\n");
        bytes.extend_from_slice(frame_label.as_bytes());
        bytes.extend_from_slice(b"\x1b[r");
        bytes.extend_from_slice(b"\x1b[6;1H");
        bytes.extend_from_slice(footer_label.as_bytes());
        bytes
    };

    {
        let pane = manager.active_tab_mut().panes.get_mut(&pane_id).unwrap();
        pane.screen.process_bytes(&frame("frame1", "footer1"));
        pane.screen.process_bytes(&frame("frame2", "footer2"));
        pane.screen.process_bytes(&frame("frame3", "footer3"));
    }

    let live = manager.active_pane_text().unwrap();
    assert!(live.contains("frame3"));
    assert!(live.contains("footer3"));

    manager.scroll_active_pane_up_lines(1).unwrap();
    let previous = manager.active_pane_text().unwrap();
    assert!(previous.contains("frame2"), "previous text:\n{previous}");
    assert!(previous.contains("footer2"), "previous text:\n{previous}");
    assert!(!previous.contains("footer3"), "previous text:\n{previous}");
}

#[test]
fn active_pane_shell_receives_truecolor_hint_env() {
    let temp = tempdir().unwrap();
    let has_truecolor_hint = with_env_var_removed("COLORTERM", || {
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

        manager
            .write_to_active_pane(b"printf '__COLORTERM__%s\\n' \"${COLORTERM:-missing}\"\n")
            .unwrap();

        wait_until(Duration::from_secs(3), || {
            manager.refresh_all_panes().unwrap_or(false)
                && manager
                    .active_pane_text()
                    .map(|text| text.contains("__COLORTERM__truecolor"))
                    .unwrap_or(false)
        })
    });

    assert!(
        has_truecolor_hint,
        "interactive apps inside mtrm should receive COLORTERM=truecolor so they can enable richer terminal styling"
    );
}

#[test]
fn active_pane_shell_receives_terminal_program_identity() {
    let temp = tempdir().unwrap();
    let has_program_identity = with_env_var_removed("TERM_PROGRAM", || {
        let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();

        manager
            .write_to_active_pane(
                b"printf '__TERM_PROGRAM__%s\\n' \"${TERM_PROGRAM:-missing}\"\n",
            )
            .unwrap();

        wait_until(Duration::from_secs(3), || {
            manager.refresh_all_panes().unwrap_or(false)
                && manager
                    .active_pane_text()
                    .map(|text| text.contains("__TERM_PROGRAM__mtrm"))
                    .unwrap_or(false)
        })
    });

    assert!(
        has_program_identity,
        "interactive apps inside mtrm should be able to detect that they are running under mtrm via TERM_PROGRAM=mtrm"
    );
}

#[test]
fn pane_selection_text_extracts_single_line_range() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
    manager
        .write_to_active_pane(b"printf 'hello world'\n")
        .unwrap();

    let loaded = wait_until(Duration::from_secs(2), || {
        manager.refresh_all_panes().unwrap_or(false)
            && manager
                .active_pane_text()
                .map(|text| text.contains("hello world"))
                .unwrap_or(false)
    });
    assert!(loaded);
    let (row, col) = find_visible_text_position(&manager, manager.active_pane_id(), "hello");

    let selected = manager
        .pane_selection_text(manager.active_pane_id(), (row, col), (row, col + 4))
        .unwrap();
    assert_eq!(selected, "hello");
}

#[test]
fn pane_selection_text_preserves_internal_spaces_and_wide_chars() {
    let temp = tempdir().unwrap();
    let mut manager = TabManager::new(&shell_config(temp.path().to_path_buf())).unwrap();
    manager.write_to_active_pane("界 a".as_bytes()).unwrap();

    let loaded = wait_until(Duration::from_secs(2), || {
        manager.refresh_all_panes().unwrap_or(false)
            && manager
                .active_pane_text()
                .map(|text| text.contains("界 a"))
                .unwrap_or(false)
    });
    assert!(loaded);
    let (row, col) = find_visible_text_position(&manager, manager.active_pane_id(), "界 a");

    let selected = manager
        .pane_selection_text(manager.active_pane_id(), (row, col), (row, col + 3))
        .unwrap();
    assert_eq!(selected, "界 a");
}
