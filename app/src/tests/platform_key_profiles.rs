#[test]
fn help_text_for_macos_profile_mentions_new_bindings_and_not_old_ones() {
    let help = help_text_for_profile(PlatformKeyProfile::MacOs);

    assert!(help.contains("Ctrl+G           Open help overlay"));
    assert!(help.contains("Ctrl+S           Split active pane left/right"));
    assert!(help.contains("Ctrl+Shift+S     Split active pane top/bottom"));
    assert!(help.contains("Ctrl+B           Focus pane left"));
    assert!(help.contains("Ctrl+F           Focus pane right"));
    assert!(help.contains("Ctrl+P           Focus pane up"));
    assert!(help.contains("Ctrl+N           Focus pane down"));
    assert!(help.contains("Ctrl+Shift+X     Save state and quit"));
    assert!(!help.contains("Ctrl+/           Open help overlay"));
    assert!(!help.contains("Ctrl+Shift+Q     Save state and quit"));
}

#[test]
fn macos_profile_ctrl_g_toggles_help_overlay() {
    let temp = tempdir().unwrap();
    let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();

    assert!(crate::help::is_toggle_help_overlay_event_for_profile(
        shortcut_event(bindings_for_profile(PlatformKeyProfile::MacOs).open_help),
        PlatformKeyProfile::MacOs
    ));

    app.open_help_overlay();
    app.handle_help_key_event_for_profile(
        shortcut_event(bindings_for_profile(PlatformKeyProfile::MacOs).open_help),
        PlatformKeyProfile::MacOs,
    );
    assert!(app.help_overlay.is_none());
}

#[test]
fn macos_profile_rejects_old_problematic_help_and_quit_shortcuts() {
    assert!(!crate::help::is_toggle_help_overlay_event_for_profile(
        key_event(KeyCode::Char('/'), KeyModifiers::CONTROL),
        PlatformKeyProfile::MacOs
    ));
    assert!(!bindings_for_profile(PlatformKeyProfile::MacOs)
        .quit
        .matches(
            key_event(KeyCode::Char('Q'), KeyModifiers::CONTROL | KeyModifiers::SHIFT),
            |_| true
        ));
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
