#[test]
#[serial]
fn rename_tab_modal_applies_title_and_persists_it() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let mut app = App::new(shell_config(home.clone())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    with_test_home(&home, || {
        app.handle_key_event(
            modified_char_event('R', current_platform_bindings().rename_tab),
            &mut clipboard,
        )
    })
    .unwrap();
    for _ in 0..5 {
        with_test_home(&home, || {
            app.handle_key_event(key_event(KeyCode::Backspace, KeyModifiers::NONE), &mut clipboard)
        })
        .unwrap();
    }
    with_test_home(&home, || {
        app.handle_key_event(key_event(KeyCode::Char('b'), KeyModifiers::NONE), &mut clipboard)
    })
    .unwrap();
    with_test_home(&home, || {
        app.handle_key_event(key_event(KeyCode::Char('u'), KeyModifiers::NONE), &mut clipboard)
    })
    .unwrap();
    with_test_home(&home, || {
        app.handle_key_event(key_event(KeyCode::Char('i'), KeyModifiers::NONE), &mut clipboard)
    })
    .unwrap();
    with_test_home(&home, || {
        app.handle_key_event(key_event(KeyCode::Char('l'), KeyModifiers::NONE), &mut clipboard)
    })
    .unwrap();
    with_test_home(&home, || {
        app.handle_key_event(key_event(KeyCode::Char('d'), KeyModifiers::NONE), &mut clipboard)
    })
    .unwrap();
    with_test_home(&home, || {
        app.handle_key_event(key_event(KeyCode::Enter, KeyModifiers::NONE), &mut clipboard)
    })
    .unwrap();

    assert_eq!(app.tabs.active_tab_title(), "build");
    assert!(app.rename.is_none());

    let restored =
        with_test_home(&home, || App::restore_or_new(shell_config(home.clone()))).unwrap();
    assert_eq!(restored.tabs.active_tab_title(), "build");
}

#[test]
fn rename_tab_modal_esc_cancels_changes() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    app.open_rename_tab_modal();
    app.handle_key_event(key_event(KeyCode::Char('x'), KeyModifiers::NONE), &mut clipboard)
        .unwrap();
    app.handle_key_event(key_event(KeyCode::Esc, KeyModifiers::NONE), &mut clipboard)
        .unwrap();

    assert!(app.rename.is_none());
    assert_eq!(app.tabs.active_tab_title(), "Tab 1");
}

#[test]
fn shift_f1_opens_help_overlay_and_escape_closes_it() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    app.handle_key_event(shortcut_event(current_platform_bindings().open_help), &mut clipboard)
        .unwrap();

    let help = app.help_overlay.clone().expect("help overlay");
    assert!(help.scroll_row > 0, "help should open near keybindings");

    app.handle_key_event(key_event(KeyCode::Esc, KeyModifiers::NONE), &mut clipboard)
        .unwrap();

    assert!(app.help_overlay.is_none());
}

#[test]
fn macos_profile_ctrl_slash_toggles_help_overlay() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();

    assert!(crate::help::is_toggle_help_overlay_event_for_profile(
        key_event(KeyCode::Char('/'), KeyModifiers::CONTROL),
        PlatformKeyProfile::MacOs
    ));

    app.open_help_overlay();
    app.handle_help_key_event_for_profile(
        key_event(KeyCode::Char('/'), KeyModifiers::CONTROL),
        PlatformKeyProfile::MacOs,
    );
    assert!(app.help_overlay.is_none());
}

#[test]
fn macos_profile_rename_shortcuts_are_detected() {
    let keymap = Keymap::default();

    assert!(is_start_rename_tab_event_for_profile(
        key_event(KeyCode::Char('R'), KeyModifiers::CONTROL | KeyModifiers::SHIFT),
        &keymap,
        PlatformKeyProfile::MacOs
    ));
    assert!(is_start_rename_pane_event_for_profile(
        key_event(KeyCode::Char('E'), KeyModifiers::CONTROL | KeyModifiers::SHIFT),
        &keymap,
        PlatformKeyProfile::MacOs
    ));
}

#[test]
fn help_overlay_consumes_plain_text_input_instead_of_sending_it_to_pty() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    app.open_help_overlay();
    app.handle_key_event(key_event(KeyCode::Char('x'), KeyModifiers::NONE), &mut clipboard)
        .unwrap();

    assert!(app.help_overlay.is_some());
    assert!(!app.tabs.active_pane_text().unwrap_or_default().contains('x'));
}

#[test]
fn help_overlay_arrow_keys_scroll_text() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();
    app.last_content_area.height = 40;

    app.open_help_overlay();

    let initial = app.help_overlay.clone().unwrap();
    app.handle_key_event(key_event(KeyCode::Down, KeyModifiers::NONE), &mut clipboard)
        .unwrap();
    app.handle_key_event(key_event(KeyCode::Right, KeyModifiers::NONE), &mut clipboard)
        .unwrap();

    let updated = app.help_overlay.clone().unwrap();
    assert_eq!(updated.scroll_row, initial.scroll_row + 1);
    assert_eq!(updated.scroll_col, initial.scroll_col + 1);
}

#[test]
fn help_overlay_down_key_keeps_scrolling_on_tall_terminal() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();
    app.last_content_area.height = 40;

    app.open_help_overlay();
    let initial = app.help_overlay.clone().unwrap();

    app.handle_key_event(key_event(KeyCode::Down, KeyModifiers::NONE), &mut clipboard)
        .unwrap();
    app.handle_key_event(key_event(KeyCode::Down, KeyModifiers::NONE), &mut clipboard)
        .unwrap();

    let updated = app.help_overlay.clone().unwrap();
    assert_eq!(updated.scroll_row, initial.scroll_row + 2);
}

#[test]
fn help_overlay_mouse_wheel_scrolls_text() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();

    app.open_help_overlay();
    let initial = app.help_overlay.clone().unwrap();

    app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        },
        DEFAULT_CONTENT_AREA,
    )
    .unwrap();

    let updated = app.help_overlay.clone().unwrap();
    assert_eq!(updated.scroll_row, initial.scroll_row + 1);
}

#[test]
fn alt_shift_e_opens_rename_pane_modal() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    app.handle_key_event(
        modified_char_event('E', current_platform_bindings().rename_pane),
        &mut clipboard,
    )
    .unwrap();

    assert_eq!(
        app.rename,
        Some(RenameState {
            target: RenameTarget::Pane(mtrm_core::PaneId::new(0)),
            input: "pane-0".to_owned(),
            cursor: 6,
        })
    );
}

#[test]
fn alt_shift_russian_u_opens_rename_pane_modal() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    app.handle_key_event(
        modified_char_event('У', current_platform_bindings().rename_pane),
        &mut clipboard,
    )
    .unwrap();

    assert!(matches!(
        app.rename,
        Some(RenameState {
            target: RenameTarget::Pane(_),
            ..
        })
    ));
}

#[test]
#[serial]
fn rename_pane_modal_applies_title_and_persists_it() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let mut app = App::new(shell_config(home.clone())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    with_test_home(&home, || {
        app.handle_key_event(
            modified_char_event('E', current_platform_bindings().rename_pane),
            &mut clipboard,
        )
    })
    .unwrap();
    for _ in 0..6 {
        with_test_home(&home, || {
            app.handle_key_event(
                key_event(KeyCode::Backspace, KeyModifiers::NONE),
                &mut clipboard,
            )
        })
        .unwrap();
    }
    for ch in ['e', 'd', 'i', 't', 'o', 'r'] {
        with_test_home(&home, || {
            app.handle_key_event(key_event(KeyCode::Char(ch), KeyModifiers::NONE), &mut clipboard)
        })
        .unwrap();
    }
    with_test_home(&home, || {
        app.handle_key_event(key_event(KeyCode::Enter, KeyModifiers::NONE), &mut clipboard)
    })
    .unwrap();

    assert_eq!(app.tabs.active_pane_title().unwrap(), "editor");

    let restored =
        with_test_home(&home, || App::restore_or_new(shell_config(home.clone()))).unwrap();
    assert_eq!(restored.tabs.active_pane_title().unwrap(), "editor");
}

#[test]
fn rename_pane_modal_esc_cancels_changes() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    app.open_rename_pane_modal();
    app.handle_key_event(key_event(KeyCode::Char('x'), KeyModifiers::NONE), &mut clipboard)
        .unwrap();
    app.handle_key_event(key_event(KeyCode::Esc, KeyModifiers::NONE), &mut clipboard)
        .unwrap();

    assert!(app.rename.is_none());
    assert_eq!(app.tabs.active_pane_title().unwrap(), "pane-0");
}

#[test]
fn build_frame_view_uses_pane_title_from_snapshot_data() {
    let temp = tempdir().unwrap();
    let dir = temp.path().join("pane");
    fs::create_dir(&dir).unwrap();

    let snapshot = mtrm_session::SessionSnapshot {
        tabs: vec![mtrm_session::TabSnapshot {
            id: mtrm_core::TabId::new(1),
            title: "main".to_owned(),
            layout: mtrm_layout::LayoutTree::new(mtrm_core::PaneId::new(10)).to_snapshot(),
            panes: vec![mtrm_session::PaneSnapshot {
                id: mtrm_core::PaneId::new(10),
                cwd: dir,
                title: "editor".to_owned(),
            }],
            active_pane: mtrm_core::PaneId::new(10),
        }],
        active_tab: mtrm_core::TabId::new(1),
    };

    let mut app = App {
        shell: shell_config(temp.path().to_path_buf()),
        keymap: Keymap::default(),
        tabs: mtrm_tabs::TabManager::from_snapshot(snapshot, &shell_config(temp.path().to_path_buf())).unwrap(),
        selection: None,
        should_quit: false,
        ui_dirty: true,
        window_focused: true,
        pending_alt_prefix_started_at: None,
        rename: None,
        help_overlay: None,
        clipboard_notice: None,
        last_content_area: DEFAULT_CONTENT_AREA,
    };

    let frame = app
        .build_frame_view(mtrm_layout::Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        })
        .unwrap();

    assert_eq!(frame.panes[0].title, "editor");
}

#[test]
#[serial]
fn handle_key_event_paste_reads_clipboard_and_sends_to_shell() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();
    clipboard.set_text("printf '__PASTE_OK__\\n'\n").unwrap();

    with_test_home(&home, || {
        app.handle_key_event(
            key_event(KeyCode::Char('v'), KeyModifiers::CONTROL),
            &mut clipboard,
        )
    })
    .unwrap();

    let ok = with_test_home(&home, || {
        wait_until(Duration::from_secs(2), || {
            app.refresh_all_panes_output().is_ok()
                && app
                    .tabs
                    .active_pane_text()
                    .map(|text| text.contains("__PASTE_OK__"))
                    .unwrap_or(false)
        })
    });
    assert!(ok);
}

#[test]
#[serial]
fn paste_with_unavailable_clipboard_sets_notice_and_does_not_fail() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = UnavailableClipboard;

    with_test_home(&home, || {
        app.handle_key_event(
            key_event(KeyCode::Char('v'), KeyModifiers::CONTROL),
            &mut clipboard,
        )
    })
    .unwrap();

    let notice = app.clipboard_notice.as_ref().expect("clipboard notice");
    assert_eq!(notice.text, "Clipboard is unavailable");
}

#[test]
#[serial]
fn paste_with_clipboard_read_error_sets_notice_and_does_not_fail() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = FailingClipboard {
        read_error: Some(ClipboardError::Read("content is not available".to_owned())),
        write_error: None,
    };

    let result = with_test_home(&home, || {
        app.handle_key_event(
            key_event(KeyCode::Char('v'), KeyModifiers::CONTROL),
            &mut clipboard,
        )
    });

    assert!(result.is_ok());
    let notice = app.clipboard_notice.as_ref().expect("clipboard notice");
    assert_eq!(notice.text, "Failed to read from clipboard");
}

#[test]
fn copy_with_unavailable_clipboard_sets_notice_and_does_not_fail() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let pane_id = app.tabs.active_pane_id();
    let mut clipboard = UnavailableClipboard;

    app.tabs
        .inject_bytes_into_active_pane_screen(b"copy text")
        .unwrap();
    app.selection = Some(SelectionState {
        pane_id,
        anchor: SelectionPoint { row: 0, col: 0 },
        focus: SelectionPoint { row: 0, col: 4 },
    });

    app.handle_key_event(
        key_event(KeyCode::Char('c'), KeyModifiers::CONTROL),
        &mut clipboard,
    )
    .unwrap();

    let notice = app.clipboard_notice.as_ref().expect("clipboard notice");
    assert_eq!(notice.text, "Clipboard is unavailable");
}

#[test]
fn copy_with_clipboard_write_error_sets_notice_and_does_not_fail() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let pane_id = app.tabs.active_pane_id();
    let mut clipboard = FailingClipboard {
        read_error: None,
        write_error: Some(ClipboardError::Write("clipboard write failed".to_owned())),
    };

    app.tabs
        .inject_bytes_into_active_pane_screen(b"copy text")
        .unwrap();
    app.selection = Some(SelectionState {
        pane_id,
        anchor: SelectionPoint { row: 0, col: 0 },
        focus: SelectionPoint { row: 0, col: 4 },
    });

    app.handle_key_event(
        key_event(KeyCode::Char('c'), KeyModifiers::CONTROL),
        &mut clipboard,
    )
    .unwrap();

    let notice = app.clipboard_notice.as_ref().expect("clipboard notice");
    assert_eq!(notice.text, "Failed to write to clipboard");
}

#[test]
#[serial]
fn ctrl_c_without_selection_does_not_copy_whole_pane() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    app.tabs
        .write_to_active_pane(b"printf 'copy me?\\n'\n")
        .unwrap();
    let loaded = wait_until(Duration::from_secs(2), || {
        app.refresh_all_panes_output().unwrap_or(false)
            && app
                .tabs
                .active_pane_text()
                .map(|text| text.contains("copy me?"))
                .unwrap_or(false)
    });
    assert!(loaded);

    with_test_home(&home, || {
        app.handle_key_event(
            key_event(KeyCode::Char('c'), KeyModifiers::CONTROL),
            &mut clipboard,
        )
    })
    .unwrap();

    assert_eq!(clipboard.get_text().unwrap(), "");
}
