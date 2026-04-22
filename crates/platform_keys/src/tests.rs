use super::*;
use crossterm::event::{KeyEventKind, KeyEventState};
use std::collections::BTreeMap;

fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn shortcut_signature(shortcut: Shortcut) -> String {
    format!("{:?}:{:?}", shortcut.modifiers, shortcut.key)
}

fn binding_signature(binding: ModifiedCharBinding, label: &str) -> String {
    if binding.chars.is_empty() {
        format!("{:?}:char:{label}", binding.modifiers)
    } else {
        format!("{:?}:chars:{:?}", binding.modifiers, binding.chars)
    }
}

fn duplicate_shortcuts(bindings: &PlatformKeyBindings) -> Vec<String> {
    let mut seen = BTreeMap::new();
    let entries = [
        (
            "interrupt",
            binding_signature(bindings.interrupt, bindings.interrupt.label),
        ),
        (
            "close_pane",
            binding_signature(bindings.close_pane, bindings.close_pane.label),
        ),
        (
            "new_tab",
            binding_signature(bindings.new_tab, bindings.new_tab.label),
        ),
        (
            "previous_tab",
            binding_signature(bindings.previous_tab, bindings.previous_tab.label),
        ),
        (
            "next_tab",
            binding_signature(bindings.next_tab, bindings.next_tab.label),
        ),
        (
            "close_tab",
            binding_signature(bindings.close_tab, bindings.close_tab.label),
        ),
        (
            "rename_tab",
            binding_signature(bindings.rename_tab, bindings.rename_tab.label),
        ),
        (
            "rename_pane",
            binding_signature(bindings.rename_pane, bindings.rename_pane.label),
        ),
        (
            "quit",
            binding_signature(bindings.quit, bindings.quit.label),
        ),
        (
            "split_vertical",
            shortcut_signature(bindings.split_vertical),
        ),
        (
            "split_horizontal",
            shortcut_signature(bindings.split_horizontal),
        ),
        ("open_help", shortcut_signature(bindings.open_help)),
        ("focus_left", shortcut_signature(bindings.focus_left)),
        ("focus_right", shortcut_signature(bindings.focus_right)),
        ("focus_up", shortcut_signature(bindings.focus_up)),
        ("focus_down", shortcut_signature(bindings.focus_down)),
        ("resize_left", shortcut_signature(bindings.resize_left)),
        ("resize_right", shortcut_signature(bindings.resize_right)),
        ("resize_up", shortcut_signature(bindings.resize_up)),
        ("resize_down", shortcut_signature(bindings.resize_down)),
    ];

    let mut duplicates = Vec::new();
    for (name, signature) in entries {
        if let Some(previous) = seen.insert(signature.clone(), name) {
            duplicates.push(format!("{previous} conflicts with {name} via {signature}"));
        }
    }
    duplicates
}

#[test]
fn linux_and_macos_profiles_are_distinct() {
    let linux = key_bindings_for_profile(PlatformKeyProfile::Linux);
    let macos = key_bindings_for_profile(PlatformKeyProfile::MacOs);

    assert_ne!(linux.interrupt.label, macos.interrupt.label);
    assert_ne!(linux.open_help.label, macos.open_help.label);
}

#[test]
fn shortcut_matches_expected_key_event() {
    let bindings = key_bindings_for_profile(PlatformKeyProfile::MacOs);
    assert!(
        bindings
            .open_help
            .matches(key_event(KeyCode::Char('g'), KeyModifiers::CONTROL))
    );
    assert!(
        !bindings
            .open_help
            .matches(key_event(KeyCode::Char('/'), KeyModifiers::CONTROL))
    );
}

#[test]
fn linux_profile_has_no_duplicate_shortcuts() {
    let duplicates = duplicate_shortcuts(key_bindings_for_profile(PlatformKeyProfile::Linux));
    assert!(duplicates.is_empty(), "linux duplicates: {duplicates:?}");
}

#[test]
fn macos_profile_has_no_duplicate_shortcuts() {
    let duplicates = duplicate_shortcuts(key_bindings_for_profile(PlatformKeyProfile::MacOs));
    assert!(duplicates.is_empty(), "macos duplicates: {duplicates:?}");
}

#[test]
fn macos_profile_uses_conservative_terminal_labels() {
    let bindings = key_bindings_for_profile(PlatformKeyProfile::MacOs);

    assert_eq!(bindings.open_help.label, "Ctrl+G");
    assert_eq!(bindings.split_vertical.label, "Ctrl+S");
    assert_eq!(bindings.split_horizontal.label, "Ctrl+Shift+S");
    assert_eq!(bindings.focus_left.label, "Ctrl+B");
    assert_eq!(bindings.focus_right.label, "Ctrl+F");
    assert_eq!(bindings.focus_up.label, "Ctrl+P");
    assert_eq!(bindings.focus_down.label, "Ctrl+N");
    assert_eq!(bindings.resize_left.label, "Ctrl+Shift+B");
    assert_eq!(bindings.resize_right.label, "Ctrl+Shift+F");
    assert_eq!(bindings.resize_up.label, "Ctrl+Shift+P");
    assert_eq!(bindings.resize_down.label, "Ctrl+Shift+N");
    assert_eq!(bindings.quit.label, "Ctrl+Shift+X");
}

#[test]
fn macos_profile_does_not_use_old_problematic_labels() {
    let bindings = key_bindings_for_profile(PlatformKeyProfile::MacOs);

    assert_ne!(bindings.open_help.label, "Ctrl+/");
    assert_ne!(bindings.split_vertical.label, "Ctrl+\\");
    assert_ne!(bindings.split_horizontal.label, "Ctrl+Shift+\\");
    assert_ne!(bindings.focus_left.label, "Ctrl+Left");
    assert_ne!(bindings.resize_right.label, "Ctrl+Shift+Right");
    assert_ne!(bindings.quit.label, "Ctrl+Shift+Q");
}
