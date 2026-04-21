use super::*;
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use mtrm_clipboard::{ClipboardBackend, ClipboardError, MemoryClipboard, UnavailableClipboard};
use mtrm_core::{AppCommand, FocusMoveDirection, LayoutCommand, TabCommand};
use mtrm_keymap::Keymap;
use mtrm_process::ShellProcessConfig;
use mtrm_state::save_state;
use ratatui::backend::TestBackend;
use serial_test::serial;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;

use crate::app::DEFAULT_CONTENT_AREA;
use crate::cli::{CliAction, help_text};
use crate::rename::{RenameState, RenameTarget};
use crate::selection::{SelectionPoint, SelectionState, pane_content_rect, tab_id_at_position};

fn shell_config(initial_cwd: PathBuf) -> ShellProcessConfig {
    ShellProcessConfig {
        program: PathBuf::from("/bin/sh"),
        args: vec![],
        initial_cwd,
        debug_log_path: None,
    }
}

fn interactive_bash_config(initial_cwd: PathBuf) -> ShellProcessConfig {
    ShellProcessConfig {
        program: PathBuf::from("bash"),
        args: vec!["-i".to_owned()],
        initial_cwd,
        debug_log_path: None,
    }
}

fn key_event(code: crossterm::event::KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[derive(Debug)]
struct FailingClipboard {
    read_error: Option<ClipboardError>,
    write_error: Option<ClipboardError>,
}

impl ClipboardBackend for FailingClipboard {
    fn get_text(&mut self) -> Result<String, ClipboardError> {
        Err(self
            .read_error
            .take()
            .unwrap_or_else(|| ClipboardError::Read("clipboard read failed".to_owned())))
    }

    fn set_text(&mut self, _text: &str) -> Result<(), ClipboardError> {
        Err(self
            .write_error
            .take()
            .unwrap_or_else(|| ClipboardError::Write("clipboard write failed".to_owned())))
    }
}

include!("cli_restore_and_rename_entry.rs");
include!("rename_and_clipboard.rs");
include!("selection_mouse_and_input.rs");
include!("layout_scroll_and_recovery.rs");
include!("runtime_render_and_save.rs");
include!("startup_and_restore_scenario.rs");
