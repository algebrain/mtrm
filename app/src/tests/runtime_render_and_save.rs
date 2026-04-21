#[test]
fn plain_left_arrow_moves_shell_cursor_left() {
    let temp = tempdir().unwrap();
    let ok = {
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
        if !initial_output {
            false
        } else {
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
            app.handle_key_event(key_event(KeyCode::Left, KeyModifiers::NONE), &mut clipboard)
                .unwrap();
            app.handle_key_event(
                key_event(KeyCode::Char('X'), KeyModifiers::NONE),
                &mut clipboard,
            )
            .unwrap();

            wait_until(Duration::from_secs(3), || {
                app.refresh_all_panes_output().is_ok()
                    && app
                        .tabs
                        .active_pane_text()
                        .map(|text| text.contains("aXb"))
                        .unwrap_or(false)
            })
        }
    };
    assert!(ok, "left arrow must move shell cursor left before Enter");
}

#[test]
#[serial]
fn plain_home_moves_shell_cursor_to_start_of_line() {
    let temp = tempdir().unwrap();
    let ok = {
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
        if !initial_output {
            false
        } else {
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
            app.handle_key_event(key_event(KeyCode::Home, KeyModifiers::NONE), &mut clipboard)
                .unwrap();
            app.handle_key_event(
                key_event(KeyCode::Char('X'), KeyModifiers::NONE),
                &mut clipboard,
            )
            .unwrap();

            wait_until(Duration::from_secs(3), || {
                app.refresh_all_panes_output().is_ok()
                    && app
                        .tabs
                        .active_pane_text()
                        .map(|text| text.contains("Xabc"))
                        .unwrap_or(false)
            })
        }
    };
    assert!(
        ok,
        "home must move shell cursor to the beginning of the line"
    );
}

#[test]
#[serial]
fn plain_end_moves_shell_cursor_to_end_of_line_when_not_scrolled() {
    let temp = tempdir().unwrap();
    let ok = {
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
        if !initial_output {
            false
        } else {
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
            app.handle_key_event(key_event(KeyCode::Left, KeyModifiers::NONE), &mut clipboard)
                .unwrap();
            app.handle_key_event(key_event(KeyCode::Home, KeyModifiers::NONE), &mut clipboard)
                .unwrap();
            app.handle_key_event(
                key_event(KeyCode::Char('X'), KeyModifiers::NONE),
                &mut clipboard,
            )
            .unwrap();
            app.handle_key_event(key_event(KeyCode::End, KeyModifiers::NONE), &mut clipboard)
                .unwrap();
            app.handle_key_event(
                key_event(KeyCode::Char('Y'), KeyModifiers::NONE),
                &mut clipboard,
            )
            .unwrap();

            wait_until(Duration::from_secs(3), || {
                app.refresh_all_panes_output().is_ok()
                    && app
                        .tabs
                        .active_pane_text()
                        .map(|text| text.contains("XabcY"))
                        .unwrap_or(false)
            })
        }
    };
    assert!(
        ok,
        "end must move shell cursor to the end of the line when pane is not scrolled"
    );
}

#[test]
fn split_pane_shell_reports_actual_pane_size() {
    let temp = tempdir().unwrap();
    let shell = mtrm_process::ShellProcessConfig {
        program: PathBuf::from("/usr/bin/env"),
        args: vec![
            "-i".to_owned(),
            "TERM=xterm-256color".to_owned(),
            "PS1=".to_owned(),
            "bash".to_owned(),
            "--noprofile".to_owned(),
            "--norc".to_owned(),
            "-i".to_owned(),
        ],
        initial_cwd: temp.path().to_path_buf(),
        debug_log_path: None,
    };
    let mut app = App::new(shell.clone()).unwrap();
    let area = mtrm_layout::Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 20,
    };

    app.handle_layout_command(LayoutCommand::SplitFocused(
        mtrm_core::SplitDirection::Vertical,
    ))
    .unwrap();
    app.tabs.resize_active_tab(area).unwrap();
    let active_pane = app.tabs.active_pane_id();
    let placements = app.tabs.placements(area).unwrap();
    let active_rect = placements
        .into_iter()
        .find(|(pane_id, _, _)| *pane_id == active_pane)
        .map(|(_, rect, _)| rect)
        .unwrap();
    let expected_rows = active_rect.height.saturating_sub(2);
    let expected_cols = active_rect.width.saturating_sub(2);

    app.tabs.write_to_active_pane(b"stty size\n").unwrap();

    let resized = wait_until(Duration::from_secs(3), || {
        app.refresh_all_panes_output().is_ok()
            && app
                .tabs
                .active_pane_text()
                .map(|text| text.contains(&format!("{expected_rows} {expected_cols}")))
                .unwrap_or(false)
    });

    assert!(
        resized,
        "split pane shell must report its own size {expected_rows}x{expected_cols}, not full terminal size"
    );
}

#[test]
#[serial]
fn save_persists_state() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();

    let mut app = App::new(shell_config(home.clone())).unwrap();
    with_test_home(&home, || app.save()).unwrap();

    assert!(home.join(".mtrm").join("state.yaml").is_file());
}

#[test]
fn redraw_does_not_fail_on_minimal_state() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    app.redraw(&mut terminal).unwrap();
}

#[test]
fn redraw_uses_real_terminal_size_for_split_panes() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    app.handle_layout_command(LayoutCommand::SplitFocused(
        mtrm_core::SplitDirection::Vertical,
    ))
    .unwrap();

    let backend = TestBackend::new(20, 8);
    let mut terminal = Terminal::new(backend).unwrap();
    app.redraw(&mut terminal).unwrap();

    let buffer = terminal.backend().buffer();
    let visible_top_corners = (0..20).filter(|x| buffer[(*x, 1)].symbol() == "┌").count();

    assert!(
        visible_top_corners >= 2,
        "vertical split should render two visible panes within terminal width"
    );
}

#[test]
fn redraw_collects_output_from_inactive_pane() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    app.handle_layout_command(LayoutCommand::SplitFocused(
        mtrm_core::SplitDirection::Vertical,
    ))
    .unwrap();
    let inactive_pane = app.tabs.active_pane_id();
    app.tabs
        .write_to_active_pane(b"printf '__INACTIVE__\\n'\n")
        .unwrap();
    app.handle_layout_command(LayoutCommand::MoveFocus(FocusMoveDirection::Left))
        .unwrap();

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let ok = wait_until(Duration::from_secs(2), || {
        app.redraw(&mut terminal).is_ok()
            && app
                .tabs
                .pane_text(inactive_pane)
                .map(|text| text.contains("__INACTIVE__"))
                .unwrap_or(false)
    });

    assert!(
        ok,
        "inactive pane output must be collected without focusing it"
    );
}

#[test]
fn app_error_display_is_sanitized_but_debug_keeps_detail() {
    let error =
        AppError::State("failed to write /tmp/secret/state.yaml: permission denied".to_owned());

    let display = error.to_string();
    let debug = format!("{error:?}");

    assert!(!display.contains("/tmp/secret"));
    assert!(!display.contains("permission denied"));
    assert!(debug.contains("/tmp/secret"));
}

#[test]
#[serial]
fn quit_command_saves_state_before_exit() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let mut app = App::new(shell_config(home.clone())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    with_test_home(&home, || {
        app.handle_command(AppCommand::Quit, &mut clipboard)
    })
    .unwrap();

    assert!(app.should_quit);
    assert!(home.join(".mtrm").join("state.yaml").is_file());
}

#[test]
#[serial]
fn quit_command_does_not_exit_when_save_fails() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    fs::write(home.join(".mtrm"), b"not a directory").unwrap();
    let mut app = App::new(shell_config(home.clone())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    let result = with_test_home(&home, || {
        app.handle_command(AppCommand::Quit, &mut clipboard)
    });

    assert!(result.is_ok());
    assert!(!app.should_quit);
    let notice = app.clipboard_notice.as_ref().expect("clipboard notice");
    assert_eq!(notice.text, "Failed to save state");
}

#[test]
#[serial]
fn request_save_failure_sets_notice_and_does_not_fail() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    fs::write(home.join(".mtrm"), b"not a directory").unwrap();
    let mut app = App::new(shell_config(home.clone())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    let result = with_test_home(&home, || {
        app.handle_command(AppCommand::RequestSave, &mut clipboard)
    });

    assert!(result.is_ok());
    assert!(!app.should_quit);
    let notice = app.clipboard_notice.as_ref().expect("clipboard notice");
    assert_eq!(notice.text, "Failed to save state");
}

#[test]
#[serial]
fn startup_shows_initial_shell_output_for_default_shell_config() {
    let temp = tempdir().unwrap();
    let ok = with_env_var("SHELL", "bash", || {
        let mut shell = default_shell_config(None).unwrap();
        shell.initial_cwd = temp.path().to_path_buf();
        let mut app = App::new(shell).unwrap();

        wait_until(Duration::from_secs(3), || {
            app.refresh_all_panes_output().is_ok()
                && app
                    .tabs
                    .active_pane_text()
                    .map(|text| !text.trim().is_empty())
                    .unwrap_or(false)
        })
    });
    assert!(ok, "default shell startup must show visible shell output");
}
