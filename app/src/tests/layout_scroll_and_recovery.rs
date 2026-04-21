#[test]
#[serial]
fn alt_x_preserves_interactive_backspace_and_arrow_editing_after_late_tty_corruption() {
    let temp = tempdir().unwrap();
    let shell = interactive_bash_config(temp.path().to_path_buf());
    let mut app = App::new(shell).unwrap();
    let mut clipboard = MemoryClipboard::new();

    let initial_output = wait_until(Duration::from_secs(3), || {
        app.refresh_all_panes_output().is_ok()
            && app
                .tabs
                .active_pane_text()
                .map(|text| !text.trim().is_empty())
                .unwrap_or(false)
    });
    assert!(
        initial_output,
        "interactive shell did not show initial output"
    );

    app.tabs
        .write_to_active_pane(
            b"sh -c 'trap \"(sleep 0.25; stty raw -echo </dev/tty >/dev/tty 2>/dev/tty) & exit 130\" INT; while :; do sleep 1; done'\n",
        )
        .unwrap();
    thread::sleep(Duration::from_millis(200));
    app.handle_key_event(
        key_event(KeyCode::Char('x'), KeyModifiers::ALT),
        &mut clipboard,
    )
    .unwrap();

    let prompt_returned = wait_until(Duration::from_secs(3), || {
        app.refresh_all_panes_output().is_ok()
            && app
                .tabs
                .active_pane_text()
                .map(|text| !text.trim().is_empty())
                .unwrap_or(false)
    });
    assert!(
        prompt_returned,
        "shell did not return visible prompt after Alt+X"
    );

    // Give the delayed tty-corruption path time to either fire or get cleaned up before
    // we assess interactive editing on the recovered shell prompt.
    thread::sleep(Duration::from_millis(450));
    let _ = app.refresh_all_panes_output();

    app.handle_key_event(
        key_event(KeyCode::Char('e'), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Char('c'), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Char('h'), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Char('o'), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Char(' '), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Char('a'), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Char('b'), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Char('c'), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Backspace, KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Char('d'), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Enter, KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();

    let backspace_ok = wait_until(Duration::from_secs(3), || {
        app.refresh_all_panes_output().is_ok()
            && app
                .tabs
                .active_pane_text()
                .map(|text| text.contains("abd") && !text.contains("^H"))
                .unwrap_or(false)
    });
    assert!(
        backspace_ok,
        "backspace editing degraded after Alt+X and late tty corruption; pane text was {:?}",
        app.tabs.active_pane_text().ok()
    );

    app.handle_key_event(
        key_event(KeyCode::Char('e'), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Char('c'), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Char('h'), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Char('o'), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Char(' '), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Char('a'), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Char('c'), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(key_event(KeyCode::Left, KeyModifiers::NONE), &mut clipboard)
        .unwrap();
    app.handle_key_event(key_event(KeyCode::Left, KeyModifiers::NONE), &mut clipboard)
        .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Char('X'), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Enter, KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();

    let ok = wait_until(Duration::from_secs(3), || {
        app.refresh_all_panes_output().is_ok()
            && app
                .tabs
                .active_pane_text()
                .map(|text| text.contains("Xac") && !text.contains("^[[D"))
                .unwrap_or(false)
    });
    assert!(
        ok,
        "left-arrow editing degraded after Alt+X and late tty corruption; pane text was {:?}",
        app.tabs.active_pane_text().ok()
    );
}

#[test]
#[serial]
fn handle_key_event_alt_minus_splits_active_pane() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    with_test_home(&home, || {
        app.handle_key_event(
            key_event(KeyCode::Char('-'), KeyModifiers::ALT),
            &mut clipboard,
        )
    })
    .unwrap();

    let placements = app
        .tabs
        .placements(mtrm_layout::Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 30,
        })
        .unwrap();
    assert_eq!(placements.len(), 2);
}

#[test]
#[serial]
fn handle_key_event_alt_t_creates_new_tab() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    with_test_home(&home, || {
        app.handle_key_event(
            key_event(KeyCode::Char('t'), KeyModifiers::ALT),
            &mut clipboard,
        )
    })
    .unwrap();

    assert_eq!(app.tabs.tab_ids().len(), 2);
}

#[test]
#[serial]
fn shift_up_scrolls_without_persisting_state() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    app.tabs
        .resize_active_tab(mtrm_layout::Rect {
            x: 0,
            y: 0,
            width: 20,
            height: 6,
        })
        .unwrap();
    app.tabs
        .write_to_active_pane(
            b"i=1; while [ \"$i\" -le 20 ]; do printf 'line%s\\n' \"$i\"; i=$((i+1)); done\n",
        )
        .unwrap();
    let loaded = wait_until(Duration::from_secs(2), || {
        app.refresh_all_panes_output().unwrap_or(false)
            && app
                .tabs
                .active_pane_text()
                .map(|text| text.contains("line20"))
                .unwrap_or(false)
    });
    assert!(loaded);

    let before = app.tabs.active_pane_text().unwrap();
    with_test_home(&home, || {
        app.handle_key_event(key_event(KeyCode::Up, KeyModifiers::SHIFT), &mut clipboard)
    })
    .unwrap();
    let after = app.tabs.active_pane_text().unwrap();

    assert_ne!(before, after);
    assert!(!home.join(".mtrm").join("state.yaml").exists());
}

#[test]
fn end_returns_scrolled_view_to_bottom() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    app.tabs
        .resize_active_tab(mtrm_layout::Rect {
            x: 0,
            y: 0,
            width: 20,
            height: 6,
        })
        .unwrap();
    app.tabs
        .write_to_active_pane(
            b"i=1; while [ \"$i\" -le 20 ]; do printf 'line%s\\n' \"$i\"; i=$((i+1)); done\n",
        )
        .unwrap();
    let loaded = wait_until(Duration::from_secs(2), || {
        app.refresh_all_panes_output().unwrap_or(false)
            && app
                .tabs
                .active_pane_text()
                .map(|text| text.contains("line20"))
                .unwrap_or(false)
    });
    assert!(loaded);

    app.handle_key_event(
        key_event(KeyCode::PageUp, KeyModifiers::SHIFT),
        &mut clipboard,
    )
    .unwrap();
    let scrolled = app.tabs.active_pane_text().unwrap();
    assert!(!scrolled.contains("line20"));

    app.handle_key_event(key_event(KeyCode::End, KeyModifiers::NONE), &mut clipboard)
        .unwrap();
    let reset = app.tabs.active_pane_text().unwrap();
    assert!(reset.contains("line20"));
}

#[test]
fn shift_up_scrolls_fullscreen_history_through_app_commands() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    app.tabs
        .write_to_active_pane(
            b"printf '\\033[?1049h\\033[2J\\033[Hframe1\\033[2J\\033[Hframe2\\033[2J\\033[Hframe3'\n",
        )
        .unwrap();

    let loaded = wait_until(Duration::from_secs(2), || {
        app.refresh_all_panes_output().unwrap_or(false)
            && app
                .tabs
                .active_pane_text()
                .map(|text| text.contains("frame3"))
                .unwrap_or(false)
    });
    assert!(loaded);

    app.handle_key_event(key_event(KeyCode::Up, KeyModifiers::SHIFT), &mut clipboard)
        .unwrap();
    let previous = app.tabs.active_pane_text().unwrap();
    assert!(previous.contains("frame2"));

    app.handle_key_event(key_event(KeyCode::End, KeyModifiers::NONE), &mut clipboard)
        .unwrap();
    let live = app.tabs.active_pane_text().unwrap();
    assert!(live.contains("frame3"));
}

#[test]
fn build_frame_view_hides_cursor_for_scrolled_fullscreen_history() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();
    let content_area = mtrm_layout::Rect {
        x: 0,
        y: 0,
        width: 40,
        height: 8,
    };

    app.tabs.resize_active_tab(content_area).unwrap();
    app.tabs
        .inject_bytes_into_active_pane_screen(b"\x1b[?1049h")
        .unwrap();
    app.tabs
        .inject_bytes_into_active_pane_screen(b"\x1b[2J\x1b[Hframe1")
        .unwrap();
    app.tabs
        .inject_bytes_into_active_pane_screen(b"\x1b[2J\x1b[Hframe2")
        .unwrap();
    app.tabs
        .inject_bytes_into_active_pane_screen(b"\x1b[2J\x1b[Hframe3")
        .unwrap();

    app.handle_key_event(key_event(KeyCode::Up, KeyModifiers::SHIFT), &mut clipboard)
        .unwrap();

    let frame = app.build_frame_view(content_area).unwrap();
    assert_eq!(frame.panes.len(), 1);
    let frame_text = frame.panes[0]
        .lines
        .iter()
        .map(|line| {
            line.cells
                .iter()
                .map(|cell| {
                    if cell.has_contents {
                        cell.text.clone()
                    } else {
                        " ".to_owned()
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(frame_text.contains("frame2"));
    assert_eq!(frame.panes[0].cursor, None);
}
