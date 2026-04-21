use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformKeyProfile {
    Linux,
    MacOs,
    Windows,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutKey {
    Char(char),
    Left,
    Right,
    Up,
    Down,
    F(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub modifiers: KeyModifiers,
    pub key: ShortcutKey,
    pub label: &'static str,
}

impl Shortcut {
    pub fn matches(&self, event: KeyEvent) -> bool {
        event.modifiers == self.modifiers
            && match (self.key, event.code) {
                (ShortcutKey::Char(expected), KeyCode::Char(actual)) => expected == actual,
                (ShortcutKey::Left, KeyCode::Left) => true,
                (ShortcutKey::Right, KeyCode::Right) => true,
                (ShortcutKey::Up, KeyCode::Up) => true,
                (ShortcutKey::Down, KeyCode::Down) => true,
                (ShortcutKey::F(expected), KeyCode::F(actual)) => expected == actual,
                _ => false,
            }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModifiedCharBinding {
    pub modifiers: KeyModifiers,
    pub label: &'static str,
}

impl ModifiedCharBinding {
    pub fn matches(&self, event: KeyEvent, matcher: impl FnOnce(char) -> bool) -> bool {
        event.modifiers == self.modifiers && matches!(event.code, KeyCode::Char(ch) if matcher(ch))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformKeyBindings {
    pub interrupt: ModifiedCharBinding,
    pub close_pane: ModifiedCharBinding,
    pub new_tab: ModifiedCharBinding,
    pub previous_tab: ModifiedCharBinding,
    pub next_tab: ModifiedCharBinding,
    pub close_tab: ModifiedCharBinding,
    pub rename_tab: ModifiedCharBinding,
    pub rename_pane: ModifiedCharBinding,
    pub quit: ModifiedCharBinding,
    pub split_vertical: Shortcut,
    pub split_horizontal: Shortcut,
    pub open_help: Shortcut,
    pub focus_left: Shortcut,
    pub focus_right: Shortcut,
    pub focus_up: Shortcut,
    pub focus_down: Shortcut,
    pub resize_left: Shortcut,
    pub resize_right: Shortcut,
    pub resize_up: Shortcut,
    pub resize_down: Shortcut,
}

const ALT_SHIFT: KeyModifiers =
    KeyModifiers::from_bits_retain(KeyModifiers::ALT.bits() | KeyModifiers::SHIFT.bits());
const CONTROL_SHIFT: KeyModifiers =
    KeyModifiers::from_bits_retain(KeyModifiers::CONTROL.bits() | KeyModifiers::SHIFT.bits());

const LINUX_BINDINGS: PlatformKeyBindings = PlatformKeyBindings {
    interrupt: ModifiedCharBinding {
        modifiers: KeyModifiers::ALT,
        label: "Alt+X",
    },
    close_pane: ModifiedCharBinding {
        modifiers: KeyModifiers::ALT,
        label: "Alt+Q",
    },
    new_tab: ModifiedCharBinding {
        modifiers: KeyModifiers::ALT,
        label: "Alt+T",
    },
    previous_tab: ModifiedCharBinding {
        modifiers: KeyModifiers::ALT,
        label: "Alt+,",
    },
    next_tab: ModifiedCharBinding {
        modifiers: KeyModifiers::ALT,
        label: "Alt+.",
    },
    close_tab: ModifiedCharBinding {
        modifiers: KeyModifiers::ALT,
        label: "Alt+W",
    },
    rename_tab: ModifiedCharBinding {
        modifiers: ALT_SHIFT,
        label: "Alt+Shift+R",
    },
    rename_pane: ModifiedCharBinding {
        modifiers: ALT_SHIFT,
        label: "Alt+Shift+E",
    },
    quit: ModifiedCharBinding {
        modifiers: ALT_SHIFT,
        label: "Alt+Shift+Q",
    },
    split_vertical: Shortcut {
        modifiers: KeyModifiers::ALT,
        key: ShortcutKey::Char('-'),
        label: "Alt+-",
    },
    split_horizontal: Shortcut {
        modifiers: KeyModifiers::ALT,
        key: ShortcutKey::Char('='),
        label: "Alt+=",
    },
    open_help: Shortcut {
        modifiers: KeyModifiers::SHIFT,
        key: ShortcutKey::F(1),
        label: "Shift+F1",
    },
    focus_left: Shortcut {
        modifiers: KeyModifiers::ALT,
        key: ShortcutKey::Left,
        label: "Alt+Left",
    },
    focus_right: Shortcut {
        modifiers: KeyModifiers::ALT,
        key: ShortcutKey::Right,
        label: "Alt+Right",
    },
    focus_up: Shortcut {
        modifiers: KeyModifiers::ALT,
        key: ShortcutKey::Up,
        label: "Alt+Up",
    },
    focus_down: Shortcut {
        modifiers: KeyModifiers::ALT,
        key: ShortcutKey::Down,
        label: "Alt+Down",
    },
    resize_left: Shortcut {
        modifiers: ALT_SHIFT,
        key: ShortcutKey::Left,
        label: "Alt+Shift+Left",
    },
    resize_right: Shortcut {
        modifiers: ALT_SHIFT,
        key: ShortcutKey::Right,
        label: "Alt+Shift+Right",
    },
    resize_up: Shortcut {
        modifiers: ALT_SHIFT,
        key: ShortcutKey::Up,
        label: "Alt+Shift+Up",
    },
    resize_down: Shortcut {
        modifiers: ALT_SHIFT,
        key: ShortcutKey::Down,
        label: "Alt+Shift+Down",
    },
};

const MACOS_BINDINGS: PlatformKeyBindings = PlatformKeyBindings {
    interrupt: ModifiedCharBinding {
        modifiers: KeyModifiers::CONTROL,
        label: "Ctrl+X",
    },
    close_pane: ModifiedCharBinding {
        modifiers: KeyModifiers::CONTROL,
        label: "Ctrl+Q",
    },
    new_tab: ModifiedCharBinding {
        modifiers: KeyModifiers::CONTROL,
        label: "Ctrl+T",
    },
    previous_tab: ModifiedCharBinding {
        modifiers: KeyModifiers::CONTROL,
        label: "Ctrl+,",
    },
    next_tab: ModifiedCharBinding {
        modifiers: KeyModifiers::CONTROL,
        label: "Ctrl+.",
    },
    close_tab: ModifiedCharBinding {
        modifiers: KeyModifiers::CONTROL,
        label: "Ctrl+W",
    },
    rename_tab: ModifiedCharBinding {
        modifiers: CONTROL_SHIFT,
        label: "Ctrl+Shift+R",
    },
    rename_pane: ModifiedCharBinding {
        modifiers: CONTROL_SHIFT,
        label: "Ctrl+Shift+E",
    },
    quit: ModifiedCharBinding {
        modifiers: CONTROL_SHIFT,
        label: "Ctrl+Shift+Q",
    },
    split_vertical: Shortcut {
        modifiers: KeyModifiers::CONTROL,
        key: ShortcutKey::Char('\\'),
        label: "Ctrl+\\",
    },
    split_horizontal: Shortcut {
        modifiers: CONTROL_SHIFT,
        key: ShortcutKey::Char('\\'),
        label: "Ctrl+Shift+\\",
    },
    open_help: Shortcut {
        modifiers: KeyModifiers::CONTROL,
        key: ShortcutKey::Char('/'),
        label: "Ctrl+/",
    },
    focus_left: Shortcut {
        modifiers: KeyModifiers::CONTROL,
        key: ShortcutKey::Left,
        label: "Ctrl+Left",
    },
    focus_right: Shortcut {
        modifiers: KeyModifiers::CONTROL,
        key: ShortcutKey::Right,
        label: "Ctrl+Right",
    },
    focus_up: Shortcut {
        modifiers: KeyModifiers::CONTROL,
        key: ShortcutKey::Up,
        label: "Ctrl+Up",
    },
    focus_down: Shortcut {
        modifiers: KeyModifiers::CONTROL,
        key: ShortcutKey::Down,
        label: "Ctrl+Down",
    },
    resize_left: Shortcut {
        modifiers: CONTROL_SHIFT,
        key: ShortcutKey::Left,
        label: "Ctrl+Shift+Left",
    },
    resize_right: Shortcut {
        modifiers: CONTROL_SHIFT,
        key: ShortcutKey::Right,
        label: "Ctrl+Shift+Right",
    },
    resize_up: Shortcut {
        modifiers: CONTROL_SHIFT,
        key: ShortcutKey::Up,
        label: "Ctrl+Shift+Up",
    },
    resize_down: Shortcut {
        modifiers: CONTROL_SHIFT,
        key: ShortcutKey::Down,
        label: "Ctrl+Shift+Down",
    },
};

pub fn current_platform_key_profile() -> PlatformKeyProfile {
    #[cfg(target_os = "linux")]
    {
        return PlatformKeyProfile::Linux;
    }

    #[cfg(target_os = "macos")]
    {
        return PlatformKeyProfile::MacOs;
    }

    #[cfg(target_os = "windows")]
    {
        return PlatformKeyProfile::Windows;
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        PlatformKeyProfile::Other
    }
}

pub fn key_bindings_for_profile(profile: PlatformKeyProfile) -> &'static PlatformKeyBindings {
    match profile {
        PlatformKeyProfile::Linux => &LINUX_BINDINGS,
        PlatformKeyProfile::MacOs => &MACOS_BINDINGS,
        PlatformKeyProfile::Windows | PlatformKeyProfile::Other => &LINUX_BINDINGS,
    }
}

pub fn current_platform_key_bindings() -> &'static PlatformKeyBindings {
    key_bindings_for_profile(current_platform_key_profile())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
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
                .matches(key_event(KeyCode::Char('/'), KeyModifiers::CONTROL))
        );
        assert!(
            !bindings
                .open_help
                .matches(key_event(KeyCode::F(1), KeyModifiers::SHIFT))
        );
    }
}
