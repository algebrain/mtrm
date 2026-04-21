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
    fn maps_alt_x_to_interrupt() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Char('x'), KeyModifiers::ALT)),
            InputAction::Command(AppCommand::SendInterrupt)
        );
    }

    #[test]
    fn ctrl_shift_c_is_not_reserved_for_interrupt() {
        assert_eq!(
            map_key_event(key_event(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )),
            InputAction::Ignore
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
    fn maps_alt_shift_arrows_to_resize_commands() {
        assert_eq!(
            map_key_event(key_event(KeyCode::Left, KeyModifiers::ALT | KeyModifiers::SHIFT)),
            InputAction::Command(AppCommand::Layout(LayoutCommand::ResizeFocused(
                ResizeDirection::Left
            )))
        );
        assert_eq!(
            map_key_event(key_event(
                KeyCode::Right,
                KeyModifiers::ALT | KeyModifiers::SHIFT
            )),
            InputAction::Command(AppCommand::Layout(LayoutCommand::ResizeFocused(
                ResizeDirection::Right
            )))
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Up, KeyModifiers::ALT | KeyModifiers::SHIFT)),
            InputAction::Command(AppCommand::Layout(LayoutCommand::ResizeFocused(
                ResizeDirection::Up
            )))
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::Down, KeyModifiers::ALT | KeyModifiers::SHIFT)),
            InputAction::Command(AppCommand::Layout(LayoutCommand::ResizeFocused(
                ResizeDirection::Down
            )))
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
    fn maps_enter_backspace_tab_and_escape_to_terminal_control_bytes() {
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
        assert_eq!(
            map_key_event(key_event(KeyCode::Esc, KeyModifiers::NONE)),
            InputAction::PtyBytes(vec![0x1b])
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
            map_key_event(key_event(KeyCode::Insert, KeyModifiers::NONE)),
            InputAction::Ignore
        );
    }

    #[test]
    fn maps_plain_function_keys_to_pty_escape_sequences() {
        assert_eq!(
            map_key_event(key_event(KeyCode::F(1), KeyModifiers::NONE)),
            InputAction::PtyBytes(b"\x1bOP".to_vec())
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::F(2), KeyModifiers::NONE)),
            InputAction::PtyBytes(b"\x1bOQ".to_vec())
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::F(3), KeyModifiers::NONE)),
            InputAction::PtyBytes(b"\x1bOR".to_vec())
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::F(4), KeyModifiers::NONE)),
            InputAction::PtyBytes(b"\x1bOS".to_vec())
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::F(5), KeyModifiers::NONE)),
            InputAction::PtyBytes(b"\x1b[15~".to_vec())
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::F(6), KeyModifiers::NONE)),
            InputAction::PtyBytes(b"\x1b[17~".to_vec())
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::F(7), KeyModifiers::NONE)),
            InputAction::PtyBytes(b"\x1b[18~".to_vec())
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::F(8), KeyModifiers::NONE)),
            InputAction::PtyBytes(b"\x1b[19~".to_vec())
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::F(9), KeyModifiers::NONE)),
            InputAction::PtyBytes(b"\x1b[20~".to_vec())
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::F(10), KeyModifiers::NONE)),
            InputAction::PtyBytes(b"\x1b[21~".to_vec())
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::F(11), KeyModifiers::NONE)),
            InputAction::PtyBytes(b"\x1b[23~".to_vec())
        );
        assert_eq!(
            map_key_event(key_event(KeyCode::F(12), KeyModifiers::NONE)),
            InputAction::PtyBytes(b"\x1b[24~".to_vec())
        );
    }
