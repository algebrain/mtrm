//! Нормализация клавиатурного ввода и преобразование его в команды программы.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtrm_core::{AppCommand, ClipboardCommand, FocusMoveDirection, LayoutCommand, ResizeDirection};
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

fn function_key_bytes(number: u8) -> Option<Vec<u8>> {
    let bytes = match number {
        1 => b"\x1bOP".as_slice(),
        2 => b"\x1bOQ".as_slice(),
        3 => b"\x1bOR".as_slice(),
        4 => b"\x1bOS".as_slice(),
        5 => b"\x1b[15~".as_slice(),
        6 => b"\x1b[17~".as_slice(),
        7 => b"\x1b[18~".as_slice(),
        8 => b"\x1b[19~".as_slice(),
        9 => b"\x1b[20~".as_slice(),
        10 => b"\x1b[21~".as_slice(),
        11 => b"\x1b[23~".as_slice(),
        12 => b"\x1b[24~".as_slice(),
        _ => return None,
    };

    Some(bytes.to_vec())
}

pub fn map_key_event(event: KeyEvent) -> InputAction {
    map_key_event_with_keymap(event, &Keymap::default())
}

pub fn map_key_event_with_keymap(event: KeyEvent, keymap: &Keymap) -> InputAction {
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
        return match event.code {
            KeyCode::Left => InputAction::Command(AppCommand::Layout(
                LayoutCommand::ResizeFocused(ResizeDirection::Left),
            )),
            KeyCode::Right => InputAction::Command(AppCommand::Layout(
                LayoutCommand::ResizeFocused(ResizeDirection::Right),
            )),
            KeyCode::Up => InputAction::Command(AppCommand::Layout(
                LayoutCommand::ResizeFocused(ResizeDirection::Up),
            )),
            KeyCode::Down => InputAction::Command(AppCommand::Layout(
                LayoutCommand::ResizeFocused(ResizeDirection::Down),
            )),
            _ if matches_char(event.code, |ch| keymap.matches_quit(ch)) => {
            InputAction::Command(AppCommand::Quit)
            }
            KeyCode::Char(ch) => InputAction::PtyBytes(alt_char_bytes(ch)),
            _ => InputAction::Ignore,
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
        KeyCode::Esc => InputAction::PtyBytes(vec![0x1b]),
        KeyCode::Left => InputAction::PtyBytes(b"\x1b[D".to_vec()),
        KeyCode::Right => InputAction::PtyBytes(b"\x1b[C".to_vec()),
        KeyCode::Up => InputAction::PtyBytes(b"\x1b[A".to_vec()),
        KeyCode::Down => InputAction::PtyBytes(b"\x1b[B".to_vec()),
        KeyCode::F(number) => function_key_bytes(number)
            .map(InputAction::PtyBytes)
            .unwrap_or(InputAction::Ignore),
        _ => InputAction::Ignore,
    }
}

#[cfg(test)]
mod tests;
