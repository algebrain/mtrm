#[test]
fn parse_cli_args_defaults_to_run_without_flags() {
    let args = vec!["mtrm".to_owned()];
    let options = parse_cli_args(args).unwrap();
    assert_eq!(options.action, CliAction::Run);
    assert_eq!(options.debug_log_path, None);
    assert!(!options.disable_clipboard);
}

#[test]
fn parse_cli_args_supports_help_flags() {
    let short = vec!["mtrm".to_owned(), "-h".to_owned()];
    let long = vec!["mtrm".to_owned(), "--help".to_owned()];

    assert_eq!(parse_cli_args(short).unwrap().action, CliAction::PrintHelp);
    assert_eq!(parse_cli_args(long).unwrap().action, CliAction::PrintHelp);
}

#[test]
fn parse_cli_args_supports_version_flags() {
    let short = vec!["mtrm".to_owned(), "-v".to_owned()];
    let long = vec!["mtrm".to_owned(), "--version".to_owned()];

    assert_eq!(
        parse_cli_args(short).unwrap().action,
        CliAction::PrintVersion
    );
    assert_eq!(
        parse_cli_args(long).unwrap().action,
        CliAction::PrintVersion
    );
}

#[test]
fn parse_cli_args_supports_debug_log_path() {
    let args = vec![
        "mtrm".to_owned(),
        "--debug-log".to_owned(),
        "/tmp/mtrm-pty.log".to_owned(),
    ];
    let options = parse_cli_args(args).unwrap();

    assert_eq!(options.action, CliAction::Run);
    assert_eq!(
        options.debug_log_path,
        Some(PathBuf::from("/tmp/mtrm-pty.log"))
    );
}

#[test]
fn parse_cli_args_supports_version_with_debug_log_path() {
    let args = vec![
        "mtrm".to_owned(),
        "--debug-log".to_owned(),
        "/tmp/mtrm-pty.log".to_owned(),
        "--version".to_owned(),
    ];
    let options = parse_cli_args(args).unwrap();

    assert_eq!(options.action, CliAction::PrintVersion);
    assert_eq!(
        options.debug_log_path,
        Some(PathBuf::from("/tmp/mtrm-pty.log"))
    );
    assert!(!options.disable_clipboard);
}

#[test]
fn parse_cli_args_supports_no_clipboard_flag() {
    let args = vec!["mtrm".to_owned(), "--no-clipboard".to_owned()];
    let options = parse_cli_args(args).unwrap();

    assert_eq!(options.action, CliAction::Run);
    assert!(options.disable_clipboard);
}

#[test]
fn parse_cli_args_supports_no_clipboard_with_debug_log_path() {
    let args = vec![
        "mtrm".to_owned(),
        "--debug-log".to_owned(),
        "/tmp/mtrm-pty.log".to_owned(),
        "--no-clipboard".to_owned(),
    ];
    let options = parse_cli_args(args).unwrap();

    assert_eq!(options.action, CliAction::Run);
    assert_eq!(
        options.debug_log_path,
        Some(PathBuf::from("/tmp/mtrm-pty.log"))
    );
    assert!(options.disable_clipboard);
}

#[test]
fn scroll_command_writes_marker_into_debug_log() {
    let temp = tempdir().unwrap();
    let log_path = temp.path().join("mtrm-debug.log");
    let shell = ShellProcessConfig {
        program: PathBuf::from("/bin/sh"),
        args: vec![],
        initial_cwd: temp.path().to_path_buf(),
        debug_log_path: Some(log_path.clone()),
    };
    let mut app = App::new(shell).unwrap();

    app.handle_layout_command(LayoutCommand::ScrollUpLines(1))
        .unwrap();

    let log = fs::read_to_string(log_path).unwrap();
    assert!(log.contains("MTRM_EVENT SCROLL_UP_LINES lines=1"));
}

#[test]
fn parse_cli_args_rejects_unknown_flags() {
    let args = vec!["mtrm".to_owned(), "--wat".to_owned()];
    let error = parse_cli_args(args).unwrap_err();

    assert!(matches!(error, AppError::Config(_)));
    assert_eq!(error.to_string(), "configuration error");
}

#[test]
fn cli_version_string_uses_git_tag_and_build_timestamp() {
    let version = cli_version_string();
    let (tag, suffix) = version.split_once(' ').unwrap();

    assert!(tag.starts_with('v'));
    assert_eq!(suffix.len(), 13);
    assert_eq!(suffix.chars().nth(6), Some('-'));
    assert!(suffix
        .chars()
        .enumerate()
        .all(|(index, ch)| if index == 6 { ch == '-' } else { ch.is_ascii_digit() }));
}

#[test]
fn help_text_mentions_keybindings_and_keymap_file() {
    let help = help_text();

    assert!(help.contains("Keybindings:"));
    assert!(help.contains("--no-clipboard"));
    assert!(help.contains("Ctrl+C           Copy selection"));
    assert!(help.contains("Alt+T            New tab"));
    assert!(help.contains("Alt+Shift+R      Rename current tab"));
    assert!(help.contains("Alt+Shift+E      Rename current pane"));
    assert!(help.contains("Shift+F1         Open help overlay"));
    assert!(help.contains("Alt+Shift+Left   Resize pane left"));
    assert!(help.contains("Alt+Shift+Right  Resize pane right"));
    assert!(help.contains("Shift+PageUp     Scroll pane history up by one page"));
    assert!(help.contains("~/.mtrm/keymap.toml"));
}

fn mouse_event(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn wait_until<F>(timeout: Duration, mut predicate: F) -> bool
where
    F: FnMut() -> bool,
{
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

fn with_test_home<T>(home: &std::path::Path, f: impl FnOnce() -> T) -> T {
    let previous_home = std::env::var_os("HOME");
    unsafe {
        std::env::set_var("HOME", home);
    }
    let result = f();
    if let Some(previous_home) = previous_home {
        unsafe {
            std::env::set_var("HOME", previous_home);
        }
    } else {
        unsafe {
            std::env::remove_var("HOME");
        }
    }
    result
}

fn with_env_var<T>(name: &str, value: &str, f: impl FnOnce() -> T) -> T {
    let previous = std::env::var_os(name);
    unsafe {
        std::env::set_var(name, value);
    }
    let result = f();
    if let Some(previous) = previous {
        unsafe {
            std::env::set_var(name, previous);
        }
    } else {
        unsafe {
            std::env::remove_var(name);
        }
    }
    result
}

fn find_visible_text_position(
    app: &App,
    pane_id: mtrm_core::PaneId,
    needle: &str,
) -> (u16, u16) {
    let text = app.tabs.pane_text(pane_id).unwrap();
    for (row, line) in text.split('\n').enumerate() {
        if let Some(col) = line.find(needle) {
            return (row as u16, col as u16);
        }
    }
    panic!("could not find {needle:?} in pane text: {text:?}");
}

#[test]
#[serial]
fn restore_or_new_creates_new_state_when_missing() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();

    let app =
        with_test_home(&home, || App::restore_or_new(shell_config(home.clone()))).unwrap();

    assert_eq!(app.tabs.tab_ids(), vec![mtrm_core::TabId::new(0)]);
}

#[test]
#[serial]
fn restore_or_new_restores_saved_state() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let dir_a = home.join("a");
    let dir_b = home.join("b");
    fs::create_dir_all(&dir_a).unwrap();
    fs::create_dir_all(&dir_b).unwrap();

    let snapshot = mtrm_session::SessionSnapshot {
        tabs: vec![mtrm_session::TabSnapshot {
            id: mtrm_core::TabId::new(7),
            title: "restored".to_owned(),
            layout: {
                let mut layout = mtrm_layout::LayoutTree::new(mtrm_core::PaneId::new(10));
                layout.split_focused(
                    mtrm_core::SplitDirection::Vertical,
                    mtrm_core::PaneId::new(11),
                );
                layout.focus_pane(mtrm_core::PaneId::new(11)).unwrap();
                layout.to_snapshot()
            },
            panes: vec![
                mtrm_session::PaneSnapshot {
                    id: mtrm_core::PaneId::new(10),
                    cwd: dir_a,
                    title: "pane-10".to_owned(),
                },
                mtrm_session::PaneSnapshot {
                    id: mtrm_core::PaneId::new(11),
                    cwd: dir_b,
                    title: "pane-11".to_owned(),
                },
            ],
            active_pane: mtrm_core::PaneId::new(11),
        }],
        active_tab: mtrm_core::TabId::new(7),
    };

    with_test_home(&home, || save_state(&snapshot)).unwrap();
    let app =
        with_test_home(&home, || App::restore_or_new(shell_config(home.clone()))).unwrap();

    assert_eq!(app.tabs.active_tab_id(), mtrm_core::TabId::new(7));
    assert_eq!(app.tabs.active_pane_id(), mtrm_core::PaneId::new(11));
}

#[test]
#[serial]
fn restore_or_new_creates_default_keymap_file() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();

    let _app =
        with_test_home(&home, || App::restore_or_new(shell_config(home.clone()))).unwrap();

    assert!(
        home.join(".mtrm").join("keymap.toml").is_file(),
        "restore_or_new must create ~/.mtrm/keymap.toml when it is missing"
    );
}

#[test]
#[serial]
fn restore_or_new_uses_keymap_file_for_bindings() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    fs::create_dir(home.join(".mtrm")).unwrap();
    fs::write(
        home.join(".mtrm").join("keymap.toml"),
        "[commands]\ncopy=['λ']\npaste=['π']\ninterrupt=['ι']\nclose_pane=['κ']\nnew_tab=['ν']\nclose_tab=['χ']\nquit=['Ω']\nprevious_tab=['<']\nnext_tab=['>']\n",
    )
    .unwrap();

    let mut app =
        with_test_home(&home, || App::restore_or_new(shell_config(home.clone()))).unwrap();
    let mut clipboard = MemoryClipboard::new();

    with_test_home(&home, || {
        app.handle_key_event(
            key_event(KeyCode::Char('ν'), KeyModifiers::ALT),
            &mut clipboard,
        )
    })
    .unwrap();

    assert_eq!(app.tabs.tab_ids().len(), 2);
}

#[test]
fn alt_shift_r_opens_rename_tab_modal() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    app.handle_key_event(
        key_event(KeyCode::Char('R'), KeyModifiers::ALT | KeyModifiers::SHIFT),
        &mut clipboard,
    )
    .unwrap();

    assert_eq!(
        app.rename,
        Some(RenameState {
            target: RenameTarget::Tab(mtrm_core::TabId::new(0)),
            input: "Tab 1".to_owned(),
            cursor: 5,
        })
    );
}

#[test]
fn alt_shift_russian_ka_opens_rename_tab_modal() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    app.handle_key_event(
        key_event(KeyCode::Char('К'), KeyModifiers::ALT | KeyModifiers::SHIFT),
        &mut clipboard,
    )
    .unwrap();

    assert!(app.rename.is_some());
}

#[test]
fn rename_tab_modal_consumes_text_input_without_sending_to_pty() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
    let mut clipboard = MemoryClipboard::new();

    app.open_rename_tab_modal();
    app.handle_key_event(key_event(KeyCode::Char('x'), KeyModifiers::NONE), &mut clipboard)
        .unwrap();

    assert_eq!(app.rename.as_ref().unwrap().input, "Tab 1x");
    let text = app.tabs.active_pane_text().unwrap();
    assert!(!text.contains("x"), "rename modal input must not reach the PTY");
}
