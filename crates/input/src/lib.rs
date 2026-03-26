//! Нормализация клавиатурного ввода и преобразование его в команды программы.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtrm_core::{AppCommand, ClipboardCommand, FocusMoveDirection, LayoutCommand};
use mtrm_keymap::Keymap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    Command(AppCommand),
    PtyBytes(Vec<u8>),
    Ignore,
}

fn is_one_of(code: KeyCode, chars: &[char]) -> bool {
    match code {
        KeyCode::Char(ch) => chars.contains(&ch),
        _ => false,
    }
}

fn matches_char(code: KeyCode, matcher: impl FnOnce(char) -> bool) -> bool {
    match code {
        KeyCode::Char(ch) => matcher(ch),
        _ => false,
    }
}

fn ctrl_char_byte(ch: char) -> Option<u8> {
    if ch.is_ascii() {
        let byte = ch as u8;
        match byte {
            b'@' | b' ' => Some(0x00),
            b'a'..=b'z' => Some(byte - b'a' + 1),
            b'A'..=b'Z' => Some(byte - b'A' + 1),
            b'[' => Some(0x1b),
            b'\\' => Some(0x1c),
            b']' => Some(0x1d),
            b'^' => Some(0x1e),
            b'_' => Some(0x1f),
            _ => None,
        }
    } else {
        None
    }
}

fn alt_char_bytes(ch: char) -> Vec<u8> {
    let mut bytes = vec![0x1b];
    bytes.extend_from_slice(ch.to_string().as_bytes());
    bytes
}

pub fn map_key_event(event: KeyEvent) -> InputAction {
    map_key_event_with_keymap(event, &Keymap::default())
}

pub fn map_key_event_with_keymap(event: KeyEvent, keymap: &Keymap) -> InputAction {
    if event.modifiers.contains(KeyModifiers::CONTROL)
        && event.modifiers.contains(KeyModifiers::SHIFT)
        && matches_char(event.code, |ch| keymap.matches_copy(ch))
    {
        return InputAction::Command(AppCommand::SendInterrupt);
    }

    if event.modifiers == KeyModifiers::CONTROL {
        return if matches_char(event.code, |ch| keymap.matches_copy(ch)) {
            InputAction::Command(AppCommand::Clipboard(ClipboardCommand::CopySelection))
        } else if matches_char(event.code, |ch| keymap.matches_paste(ch)) {
            InputAction::Command(AppCommand::Clipboard(ClipboardCommand::PasteFromSystem))
        } else if let KeyCode::Char(ch) = event.code {
            ctrl_char_byte(ch)
                .map(|byte| InputAction::PtyBytes(vec![byte]))
                .unwrap_or(InputAction::Ignore)
        } else {
            InputAction::Ignore
        };
    }

    if event.modifiers == KeyModifiers::ALT {
        return if matches_char(event.code, |ch| keymap.matches_interrupt(ch)) {
            InputAction::Command(AppCommand::SendInterrupt)
        } else if is_one_of(event.code, &['-', '_']) {
            InputAction::Command(AppCommand::Layout(LayoutCommand::SplitFocused(
                mtrm_core::SplitDirection::Vertical,
            )))
        } else if is_one_of(event.code, &['=', '+']) {
            InputAction::Command(AppCommand::Layout(LayoutCommand::SplitFocused(
                mtrm_core::SplitDirection::Horizontal,
            )))
        } else if matches_char(event.code, |ch| keymap.matches_close_pane(ch)) {
            InputAction::Command(AppCommand::Layout(LayoutCommand::CloseFocusedPane))
        } else if matches_char(event.code, |ch| keymap.matches_new_tab(ch)) {
            InputAction::Command(AppCommand::Tabs(mtrm_core::TabCommand::NewTab))
        } else if matches_char(event.code, |ch| keymap.matches_previous_tab(ch)) {
            InputAction::Command(AppCommand::Tabs(mtrm_core::TabCommand::PreviousTab))
        } else if matches_char(event.code, |ch| keymap.matches_next_tab(ch)) {
            InputAction::Command(AppCommand::Tabs(mtrm_core::TabCommand::NextTab))
        } else if matches_char(event.code, |ch| keymap.matches_close_tab(ch)) {
            InputAction::Command(AppCommand::Tabs(mtrm_core::TabCommand::CloseCurrentTab))
        } else {
            match event.code {
                KeyCode::Left => InputAction::Command(AppCommand::Layout(
                    LayoutCommand::MoveFocus(FocusMoveDirection::Left),
                )),
                KeyCode::Right => InputAction::Command(AppCommand::Layout(
                    LayoutCommand::MoveFocus(FocusMoveDirection::Right),
                )),
                KeyCode::Up => InputAction::Command(AppCommand::Layout(LayoutCommand::MoveFocus(
                    FocusMoveDirection::Up,
                ))),
                KeyCode::Down => InputAction::Command(AppCommand::Layout(
                    LayoutCommand::MoveFocus(FocusMoveDirection::Down),
                )),
                KeyCode::Char(ch) => InputAction::PtyBytes(alt_char_bytes(ch)),
                _ => InputAction::Ignore,
            }
        };
    }

    if event.modifiers == KeyModifiers::SHIFT {
        return match event.code {
            KeyCode::Up => {
                InputAction::Command(AppCommand::Layout(LayoutCommand::ScrollUpLines(1)))
            }
            KeyCode::Down => {
                InputAction::Command(AppCommand::Layout(LayoutCommand::ScrollDownLines(1)))
            }
            KeyCode::PageUp => {
                InputAction::Command(AppCommand::Layout(LayoutCommand::ScrollUpPages(1)))
            }
            KeyCode::PageDown => {
                InputAction::Command(AppCommand::Layout(LayoutCommand::ScrollDownPages(1)))
            }
            KeyCode::Char(ch) => InputAction::PtyBytes(ch.to_string().into_bytes()),
            _ => InputAction::Ignore,
        };
    }

    if event.modifiers == KeyModifiers::NONE && matches!(event.code, KeyCode::End) {
        return InputAction::Command(AppCommand::Layout(LayoutCommand::ScrollToBottom));
    }

    if event.modifiers == (KeyModifiers::ALT | KeyModifiers::SHIFT) {
        return if matches_char(event.code, |ch| keymap.matches_quit(ch)) {
            InputAction::Command(AppCommand::Quit)
        } else if let KeyCode::Char(ch) = event.code {
            InputAction::PtyBytes(alt_char_bytes(ch))
        } else {
            InputAction::Ignore
        };
    }

    if !event.modifiers.is_empty() {
        return InputAction::Ignore;
    }

    match event.code {
        KeyCode::Char(ch) => InputAction::PtyBytes(ch.to_string().into_bytes()),
        KeyCode::Enter => InputAction::PtyBytes(vec![b'\r']),
        KeyCode::Backspace => InputAction::PtyBytes(vec![0x08]),
        KeyCode::Tab => InputAction::PtyBytes(vec![b'\t']),
        KeyCode::Left => InputAction::PtyBytes(b"\x1b[D".to_vec()),
        KeyCode::Right => InputAction::PtyBytes(b"\x1b[C".to_vec()),
        KeyCode::Up => InputAction::PtyBytes(b"\x1b[A".to_vec()),
        KeyCode::Down => InputAction::PtyBytes(b"\x1b[B".to_vec()),
        _ => InputAction::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};
    use mtrm_keymap::Keymap;

    fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn maps_ctrl_c_to_copy_selection() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            InputAction::Command(AppCommand::Clipboard(ClipboardCommand::CopySelection))
        );
    }

    #[test]
    fn maps_ctrl_v_to_paste_from_system() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('v'), KeyModifiers::CONTROL)),
            InputAction::Command(AppCommand::Clipboard(ClipboardCommand::PasteFromSystem))
        );
    }

    #[test]
    fn maps_ctrl_shift_c_to_interrupt() {
        assert_eq!(
            map_key_event(key_event(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )),
            InputAction::Command(AppCommand::SendInterrupt)
        );
    }

    #[test]
    fn maps_alt_x_to_interrupt() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('x'), KeyModifiers::ALT)),
            InputAction::Command(AppCommand::SendInterrupt)
        );
    }

    #[test]
    fn maps_alt_arrows_to_focus_commands() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Left, KeyModifiers::ALT)),
            InputAction::Command(AppCommand::Layout(LayoutCommand::MoveFocus(
                FocusMoveDirection::Left
            )))
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Right, KeyModifiers::ALT)),
            InputAction::Command(AppCommand::Layout(LayoutCommand::MoveFocus(
                FocusMoveDirection::Right
            )))
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Up, KeyModifiers::ALT)),
            InputAction::Command(AppCommand::Layout(LayoutCommand::MoveFocus(
                FocusMoveDirection::Up
            )))
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Down, KeyModifiers::ALT)),
            InputAction::Command(AppCommand::Layout(LayoutCommand::MoveFocus(
                FocusMoveDirection::Down
            )))
        );
    }

    #[test]
    fn maps_shift_arrows_and_page_keys_to_scrollback_commands() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Up, KeyModifiers::SHIFT)),
            InputAction::Command(AppCommand::Layout(LayoutCommand::ScrollUpLines(1)))
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Down, KeyModifiers::SHIFT)),
            InputAction::Command(AppCommand::Layout(LayoutCommand::ScrollDownLines(1)))
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::PageUp, KeyModifiers::SHIFT)),
            InputAction::Command(AppCommand::Layout(LayoutCommand::ScrollUpPages(1)))
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::PageDown, KeyModifiers::SHIFT)),
            InputAction::Command(AppCommand::Layout(LayoutCommand::ScrollDownPages(1)))
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::End, KeyModifiers::NONE)),
            InputAction::Command(AppCommand::Layout(LayoutCommand::ScrollToBottom))
        );
    }

    #[test]
    fn maps_shift_printable_characters_to_utf8_bytes() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('A'), KeyModifiers::SHIFT)),
            InputAction::PtyBytes("A".as_bytes().to_vec())
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('Я'), KeyModifiers::SHIFT)),
            InputAction::PtyBytes("Я".as_bytes().to_vec())
        );
    }

    #[test]
    fn maps_alt_split_and_close_commands() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('-'), KeyModifiers::ALT)),
            InputAction::Command(AppCommand::Layout(LayoutCommand::SplitFocused(
                mtrm_core::SplitDirection::Vertical
            )))
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('='), KeyModifiers::ALT)),
            InputAction::Command(AppCommand::Layout(LayoutCommand::SplitFocused(
                mtrm_core::SplitDirection::Horizontal
            )))
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('q'), KeyModifiers::ALT)),
            InputAction::Command(AppCommand::Layout(LayoutCommand::CloseFocusedPane))
        );
    }

    #[test]
    fn maps_alt_tab_commands() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('t'), KeyModifiers::ALT)),
            InputAction::Command(AppCommand::Tabs(mtrm_core::TabCommand::NewTab))
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Char(','), KeyModifiers::ALT)),
            InputAction::Command(AppCommand::Tabs(mtrm_core::TabCommand::PreviousTab))
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('.'), KeyModifiers::ALT)),
            InputAction::Command(AppCommand::Tabs(mtrm_core::TabCommand::NextTab))
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('w'), KeyModifiers::ALT)),
            InputAction::Command(AppCommand::Tabs(mtrm_core::TabCommand::CloseCurrentTab))
        );
    }

    #[test]
    fn maps_alt_printable_characters_to_meta_prefixed_bytes_when_not_commands() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('b'), KeyModifiers::ALT)),
            InputAction::PtyBytes(vec![0x1b, b'b'])
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('Б'), KeyModifiers::ALT)),
            InputAction::PtyBytes({
                let mut bytes = vec![0x1b];
                bytes.extend_from_slice("Б".as_bytes());
                bytes
            })
        );
    }

    #[test]
    fn maps_alt_shift_q_to_quit() {
        assert_eq!(
            map_key_event(key_event(
                KeyCode::Char('Q'),
                KeyModifiers::ALT | KeyModifiers::SHIFT
            )),
            InputAction::Command(AppCommand::Quit)
        );
    }

    #[test]
    fn maps_alt_shift_printable_characters_to_meta_prefixed_bytes_when_not_quit() {
        assert_eq!(
            map_key_event(key_event(
                KeyCode::Char('B'),
                KeyModifiers::ALT | KeyModifiers::SHIFT
            )),
            InputAction::PtyBytes(vec![0x1b, b'B'])
        );
        assert_eq!(
            map_key_event(key_event(
                KeyCode::Char('Я'),
                KeyModifiers::ALT | KeyModifiers::SHIFT
            )),
            InputAction::PtyBytes({
                let mut bytes = vec![0x1b];
                bytes.extend_from_slice("Я".as_bytes());
                bytes
            })
        );
    }

    #[test]
    fn russian_layout_hotkeys_map_to_same_commands() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('е'), KeyModifiers::ALT)),
            InputAction::Command(AppCommand::Tabs(mtrm_core::TabCommand::NewTab))
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('й'), KeyModifiers::ALT)),
            InputAction::Command(AppCommand::Layout(LayoutCommand::CloseFocusedPane))
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('ц'), KeyModifiers::ALT)),
            InputAction::Command(AppCommand::Tabs(mtrm_core::TabCommand::CloseCurrentTab))
        );
        assert_eq!(
            map_key_event(key_event(
                KeyCode::Char('Й'),
                KeyModifiers::ALT | KeyModifiers::SHIFT
            )),
            InputAction::Command(AppCommand::Quit)
        );
    }

    #[test]
    fn default_keymap_supports_french_and_greek_symbols() {
        let keymap = Keymap::default();

        assert_eq!(
            map_key_event_with_keymap(key_event(KeyCode::Char('a'), KeyModifiers::ALT), &keymap),
            InputAction::Command(AppCommand::Layout(LayoutCommand::CloseFocusedPane))
        );
        assert_eq!(
            map_key_event_with_keymap(key_event(KeyCode::Char('z'), KeyModifiers::ALT), &keymap),
            InputAction::Command(AppCommand::Tabs(mtrm_core::TabCommand::CloseCurrentTab))
        );
        assert_eq!(
            map_key_event_with_keymap(
                key_event(KeyCode::Char('ψ'), KeyModifiers::CONTROL),
                &keymap
            ),
            InputAction::Command(AppCommand::Clipboard(ClipboardCommand::CopySelection))
        );
        assert_eq!(
            map_key_event_with_keymap(
                key_event(KeyCode::Char('ω'), KeyModifiers::CONTROL),
                &keymap
            ),
            InputAction::Command(AppCommand::Clipboard(ClipboardCommand::PasteFromSystem))
        );
        assert_eq!(
            map_key_event_with_keymap(key_event(KeyCode::Char('χ'), KeyModifiers::ALT), &keymap),
            InputAction::Command(AppCommand::SendInterrupt)
        );
        assert_eq!(
            map_key_event_with_keymap(key_event(KeyCode::Char('τ'), KeyModifiers::ALT), &keymap),
            InputAction::Command(AppCommand::Tabs(mtrm_core::TabCommand::NewTab))
        );
        assert_eq!(
            map_key_event_with_keymap(
                key_event(KeyCode::Char(':'), KeyModifiers::ALT | KeyModifiers::SHIFT),
                &keymap
            ),
            InputAction::Command(AppCommand::Quit)
        );
    }

    #[test]
    fn maps_ctrl_printable_characters_to_control_bytes_when_not_reserved() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('a'), KeyModifiers::CONTROL)),
            InputAction::PtyBytes(vec![0x01])
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('z'), KeyModifiers::CONTROL)),
            InputAction::PtyBytes(vec![0x1a])
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('['), KeyModifiers::CONTROL)),
            InputAction::PtyBytes(vec![0x1b])
        );
    }

    #[test]
    fn keymap_only_changes_reserved_command_combos_not_shift_printable_passthrough() {
        let keymap = Keymap::from_toml_str(
            r#"
[commands]
copy = ["λ"]
paste = ["π"]
interrupt = ["ι"]
close_pane = ["κ"]
new_tab = ["ν"]
close_tab = ["τ"]
quit = ["Ω"]
previous_tab = [","]
next_tab = ["."]
"#,
        )
        .unwrap();

        assert_eq!(
            map_key_event_with_keymap(key_event(KeyCode::Char('Я'), KeyModifiers::SHIFT), &keymap),
            InputAction::PtyBytes("Я".as_bytes().to_vec())
        );
        assert_eq!(
            map_key_event_with_keymap(
                key_event(KeyCode::Char('Ω'), KeyModifiers::ALT | KeyModifiers::SHIFT),
                &keymap
            ),
            InputAction::Command(AppCommand::Quit)
        );
        assert_eq!(
            map_key_event_with_keymap(
                key_event(KeyCode::Char('B'), KeyModifiers::ALT | KeyModifiers::SHIFT),
                &keymap
            ),
            InputAction::PtyBytes(vec![0x1b, b'B'])
        );
        assert_eq!(
            map_key_event_with_keymap(
                key_event(KeyCode::Char('a'), KeyModifiers::CONTROL),
                &keymap
            ),
            InputAction::PtyBytes(vec![0x01])
        );
    }

    #[test]
    fn custom_keymap_changes_letter_bindings() {
        let keymap = Keymap::from_toml_str(
            "[commands]\ncopy=['λ']\npaste=['π']\ninterrupt=['ι']\nclose_pane=['κ']\nnew_tab=['ν']\nclose_tab=['χ']\nquit=['Ω']\nprevious_tab=['<']\nnext_tab=['>']\n",
        )
        .unwrap();

        assert_eq!(
            map_key_event_with_keymap(key_event(KeyCode::Char('ν'), KeyModifiers::ALT), &keymap),
            InputAction::Command(AppCommand::Tabs(mtrm_core::TabCommand::NewTab))
        );
        assert_eq!(
            map_key_event_with_keymap(
                key_event(KeyCode::Char('λ'), KeyModifiers::CONTROL),
                &keymap
            ),
            InputAction::Command(AppCommand::Clipboard(ClipboardCommand::CopySelection))
        );
    }

    #[test]
    fn maps_ascii_character_to_utf8_bytes() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('a'), KeyModifiers::NONE)),
            InputAction::PtyBytes(vec![b'a'])
        );
    }

    #[test]
    fn maps_non_ascii_character_to_utf8_bytes() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('й'), KeyModifiers::NONE)),
            InputAction::PtyBytes("й".as_bytes().to_vec())
        );
    }

    #[test]
    fn maps_enter_backspace_and_tab_to_terminal_control_bytes() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Enter, KeyModifiers::NONE)),
            InputAction::PtyBytes(vec![b'\r'])
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Backspace, KeyModifiers::NONE)),
            InputAction::PtyBytes(vec![0x08])
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Tab, KeyModifiers::NONE)),
            InputAction::PtyBytes(vec![b'\t'])
        );
    }

    #[test]
    fn maps_plain_arrows_to_pty_escape_sequences() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Left, KeyModifiers::NONE)),
            InputAction::PtyBytes(b"\x1b[D".to_vec())
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Right, KeyModifiers::NONE)),
            InputAction::PtyBytes(b"\x1b[C".to_vec())
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Up, KeyModifiers::NONE)),
            InputAction::PtyBytes(b"\x1b[A".to_vec())
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Down, KeyModifiers::NONE)),
            InputAction::PtyBytes(b"\x1b[B".to_vec())
        );
    }

    #[test]
    fn unsupported_non_printable_events_are_ignored() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Esc, KeyModifiers::NONE)),
            InputAction::Ignore
        );
    }
}
