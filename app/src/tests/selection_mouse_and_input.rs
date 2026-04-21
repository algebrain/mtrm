#[test]
#[serial]
fn ctrl_c_copies_only_selected_text_from_split_pane() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();
    let content_area = mtrm_layout::Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 23,
    };

    app.handle_layout_command(LayoutCommand::SplitFocused(
        mtrm_core::SplitDirection::Vertical,
    ))
    .unwrap();
    let right_pane = app.tabs.active_pane_id();
    app.tabs
        .write_to_active_pane(b"printf 'right pane text\\n'\n")
        .unwrap();
    app.handle_layout_command(LayoutCommand::MoveFocus(FocusMoveDirection::Left))
        .unwrap();
    app.tabs
        .write_to_active_pane(b"printf 'left pane text\\n'\n")
        .unwrap();

    let loaded = wait_until(Duration::from_secs(2), || {
        app.refresh_all_panes_output().unwrap_or(false)
            && app
                .tabs
                .pane_text(right_pane)
                .map(|text| text.contains("right pane text"))
                .unwrap_or(false)
            && app
                .tabs
                .active_pane_text()
                .map(|text| text.contains("left pane text"))
                .unwrap_or(false)
    });
    assert!(loaded);

    let right_area = app
        .tabs
        .placements(content_area)
        .unwrap()
        .into_iter()
        .find(|(pane_id, _, _)| *pane_id == right_pane)
        .map(|(_, area, _)| area)
        .unwrap();
    let right_content = pane_content_rect(right_area).unwrap();
    let (text_row, text_col) = find_visible_text_position(&app, right_pane, "right");

    app.handle_mouse_event(
        mouse_event(
            MouseEventKind::Down(MouseButton::Left),
            right_content.x.saturating_add(text_col),
            right_content.y.saturating_add(text_row),
        ),
        content_area,
    )
    .unwrap();
    app.handle_mouse_event(
        mouse_event(
            MouseEventKind::Drag(MouseButton::Left),
            right_content.x.saturating_add(text_col).saturating_add(4),
            right_content.y.saturating_add(text_row),
        ),
        content_area,
    )
    .unwrap();
    app.handle_mouse_event(
        mouse_event(
            MouseEventKind::Up(MouseButton::Left),
            right_content.x.saturating_add(text_col).saturating_add(4),
            right_content.y.saturating_add(text_row),
        ),
        content_area,
    )
    .unwrap();

    with_test_home(&home, || {
        app.handle_key_event(
            key_event(KeyCode::Char('c'), KeyModifiers::CONTROL),
            &mut clipboard,
        )
    })
    .unwrap();

    assert_eq!(clipboard.get_text().unwrap(), "right");
}

#[test]
#[serial]
fn mouse_click_switches_focus_to_clicked_pane() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let content_area = mtrm_layout::Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 23,
    };

    app.handle_layout_command(LayoutCommand::SplitFocused(
        mtrm_core::SplitDirection::Vertical,
    ))
    .unwrap();
    let right_pane = app.tabs.active_pane_id();
    app.handle_layout_command(LayoutCommand::MoveFocus(FocusMoveDirection::Left))
        .unwrap();
    assert_ne!(app.tabs.active_pane_id(), right_pane);

    let right_area = app
        .tabs
        .placements(content_area)
        .unwrap()
        .into_iter()
        .find(|(pane_id, _, _)| *pane_id == right_pane)
        .map(|(_, area, _)| area)
        .unwrap();
    let right_content = pane_content_rect(right_area).unwrap();

    app.handle_mouse_event(
        mouse_event(
            MouseEventKind::Down(MouseButton::Left),
            right_content.x,
            right_content.y,
        ),
        content_area,
    )
    .unwrap();

    assert_eq!(app.tabs.active_pane_id(), right_pane);
}

#[test]
fn tab_hit_testing_returns_clicked_tab_only_inside_title_span() {
    let tabs = vec![
        mtrm_tabs::RuntimeTabSummary {
            id: mtrm_core::TabId::new(0),
            title: "One".to_owned(),
            active: true,
        },
        mtrm_tabs::RuntimeTabSummary {
            id: mtrm_core::TabId::new(1),
            title: "Two".to_owned(),
            active: false,
        },
    ];

    assert_eq!(
        tab_id_at_position(&tabs, 80, 0, 0),
        Some(mtrm_core::TabId::new(0))
    );
    assert_eq!(
        tab_id_at_position(&tabs, 80, 2, 0),
        Some(mtrm_core::TabId::new(0))
    );
    assert_eq!(tab_id_at_position(&tabs, 80, 3, 0), None);
    assert_eq!(tab_id_at_position(&tabs, 80, 4, 0), None);
    assert_eq!(tab_id_at_position(&tabs, 80, 5, 0), None);
    assert_eq!(tab_id_at_position(&tabs, 80, 6, 0), Some(mtrm_core::TabId::new(1)));
    assert_eq!(tab_id_at_position(&tabs, 80, 0, 1), None);
}

#[test]
#[serial]
fn mouse_click_on_tab_bar_switches_active_tab() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let content_area = mtrm_layout::Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 23,
    };

    let first = app.tabs.active_tab_id();
    app.handle_tab_command(TabCommand::NewTab).unwrap();
    let second = app.tabs.active_tab_id();
    assert_ne!(first, second);

    with_test_home(&home, || {
        app.handle_mouse_event(
            mouse_event(MouseEventKind::Down(MouseButton::Left), 0, 0),
            content_area,
        )
    })
    .unwrap();

    assert_eq!(app.tabs.active_tab_id(), first);

    with_test_home(&home, || {
        let summaries = app.tabs.tab_summaries();
        let second_x = (0..content_area.width)
            .find_map(|column| {
                tab_id_at_position(&summaries, content_area.width, column, 0)
                    .filter(|tab_id| *tab_id == second)
                    .map(|_| column)
            })
            .expect("expected to find clickable column for second tab");
        app.handle_mouse_event(
            mouse_event(MouseEventKind::Down(MouseButton::Left), second_x, 0),
            content_area,
        )
    })
    .unwrap();

    assert_eq!(app.tabs.active_tab_id(), second);
}

#[test]
#[serial]
fn handle_key_event_regular_char_sends_bytes() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    with_test_home(&home, || {
        app.handle_key_event(
            key_event(KeyCode::Char('p'), KeyModifiers::NONE),
            &mut clipboard,
        )
        .unwrap();
        app.handle_key_event(
            key_event(KeyCode::Char('w'), KeyModifiers::NONE),
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
    });

    let expected = temp.path().to_string_lossy().to_string();
    let ok = with_test_home(&home, || {
        wait_until(Duration::from_secs(2), || {
            app.refresh_all_panes_output().is_ok()
                && app
                    .tabs
                    .active_pane_text()
                    .map(|text| text.contains(&expected))
                    .unwrap_or(false)
        })
    });
    assert!(ok);
}

#[test]
fn closing_last_tab_sets_notice_and_does_not_fail() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    app.handle_command(
        AppCommand::Tabs(TabCommand::CloseCurrentTab),
        &mut clipboard,
    )
    .unwrap();

    let notice = app.clipboard_notice.as_ref().expect("clipboard notice");
    assert_eq!(notice.text, "Failed to update tabs");
}

#[test]
#[serial]
fn regular_input_does_not_persist_state_immediately() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    with_test_home(&home, || {
        app.handle_key_event(
            key_event(KeyCode::Char('x'), KeyModifiers::NONE),
            &mut clipboard,
        )
    })
    .unwrap();

    assert!(
        !home.join(".mtrm").join("state.yaml").exists(),
        "plain PTY input must not trigger state save"
    );
}

#[test]
fn handle_key_event_alt_x_sends_interrupt() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    app.tabs.write_to_active_pane(b"sleep 5\n").unwrap();
    thread::sleep(Duration::from_millis(150));
    app.handle_key_event(
        key_event(KeyCode::Char('x'), KeyModifiers::ALT),
        &mut clipboard,
    )
    .unwrap();
    app.tabs
        .write_to_active_pane(b"printf '__APP_INTERRUPT__\\n'\n")
        .unwrap();

    let ok = wait_until(Duration::from_secs(3), || {
        app.refresh_all_panes_output().is_ok()
            && app
                .tabs
                .active_pane_text()
                .map(|text| text.contains("__APP_INTERRUPT__"))
                .unwrap_or(false)
    });
    assert!(ok);
}

#[test]
fn handle_key_event_esc_prefix_russian_interrupt_sends_interrupt() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    app.tabs.write_to_active_pane(b"sleep 5\n").unwrap();
    thread::sleep(Duration::from_millis(150));
    app.handle_key_event(key_event(KeyCode::Esc, KeyModifiers::NONE), &mut clipboard)
        .unwrap();
    app.handle_key_event(
        key_event(KeyCode::Char('ч'), KeyModifiers::NONE),
        &mut clipboard,
    )
    .unwrap();
    app.tabs
        .write_to_active_pane(b"printf '__ESC_PREFIX_INTERRUPT__\\n'\n")
        .unwrap();

    let ok = wait_until(Duration::from_secs(3), || {
        app.refresh_all_panes_output().is_ok()
            && app
                .tabs
                .active_pane_text()
                .map(|text| text.contains("__ESC_PREFIX_INTERRUPT__"))
                .unwrap_or(false)
    });
    assert!(ok);
}
