#[test]
#[serial]
fn startup_shell_echoes_typed_characters_before_enter() {
    let temp = tempdir().unwrap();
    let ok = with_env_var("SHELL", "bash", || {
        let mut shell = default_shell_config(None).unwrap();
        shell.initial_cwd = temp.path().to_path_buf();
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
            return false;
        }

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

        wait_until(Duration::from_secs(3), || {
            app.refresh_all_panes_output().is_ok()
                && app
                    .tabs
                    .active_pane_text()
                    .map(|text| text.contains("ec"))
                    .unwrap_or(false)
        })
    });
    assert!(
        ok,
        "typed characters must be visible before Enter in interactive shell"
    );
}

#[test]
#[serial]
fn scenario_split_save_restore_preserves_layout_and_cwd() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let pane_dir = home.join("pane");
    fs::create_dir_all(&pane_dir).unwrap();
    let home = fs::canonicalize(home).unwrap();
    let pane_dir = fs::canonicalize(pane_dir).unwrap();

    let mut app = App::new(shell_config(home.clone())).unwrap();
    app.handle_layout_command(LayoutCommand::SplitFocused(
        mtrm_core::SplitDirection::Vertical,
    ))
    .unwrap();
    app.handle_layout_command(LayoutCommand::MoveFocus(FocusMoveDirection::Right))
        .unwrap();
    app.tabs
        .write_to_active_pane(format!("cd '{}'\n", pane_dir.display()).as_bytes())
        .unwrap();

    {
        let changed = wait_until(Duration::from_secs(2), || {
            app.tabs
                .active_pane_cwd()
                .map(|cwd| cwd == pane_dir)
                .unwrap_or(false)
        });
        assert!(changed);
    }

    with_test_home(&home, || app.save()).unwrap();
    let restored =
        with_test_home(&home, || App::restore_or_new(shell_config(home.clone()))).unwrap();
    let placements = restored
        .tabs
        .placements(mtrm_layout::Rect {
            x: 0,
            y: 0,
            width: 120,
            height: 40,
        })
        .unwrap();

    assert_eq!(placements.len(), 2);

    assert_eq!(restored.tabs.active_pane_cwd().unwrap(), pane_dir);
}

#[cfg(target_os = "macos")]
#[test]
#[serial]
fn scenario_split_save_restore_canonicalizes_alias_temp_paths_on_macos() {
    let temp = tempdir().unwrap();
    let home = fs::canonicalize(temp.path()).unwrap();
    let canonical_text = home.to_string_lossy();
    let alias_text = canonical_text.replacen("/private/var/", "/var/", 1);

    assert_ne!(
        alias_text, canonical_text,
        "test requires a canonical /private/var/... path on macOS"
    );

    let alias_home = PathBuf::from(alias_text);
    assert!(
        alias_home.exists(),
        "alias temp path must exist: {:?}",
        alias_home
    );

    let mut app = App::new(shell_config(alias_home.clone())).unwrap();

    with_test_home(&alias_home, || app.save()).unwrap();
    let restored = with_test_home(&alias_home, || {
        App::restore_or_new(shell_config(alias_home.clone()))
    })
    .unwrap();

    assert_eq!(restored.tabs.active_pane_cwd().unwrap(), home);
}
