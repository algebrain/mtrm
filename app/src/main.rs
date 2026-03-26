use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use mtrm_clipboard::{ClipboardBackend, ClipboardError, SystemClipboard};
use mtrm_core::{AppCommand, ClipboardCommand, LayoutCommand, TabCommand};
use mtrm_input::{InputAction, map_key_event_with_keymap};
use mtrm_keymap::{Keymap, load_keymap};
use mtrm_process::ShellProcessConfig;
use mtrm_state::{load_state, save_state};
use mtrm_tabs::TabManager;
use mtrm_ui::{FrameView, PaneView, TabView, render_frame};
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::backend::CrosstermBackend;
use thiserror::Error;

pub struct App {
    shell: ShellProcessConfig,
    keymap: Keymap,
    tabs: TabManager,
    should_quit: bool,
    ui_dirty: bool,
}

impl App {
    pub fn new(shell: ShellProcessConfig) -> Result<Self, AppError> {
        Self::new_with_keymap(shell, Keymap::default())
    }

    fn new_with_keymap(shell: ShellProcessConfig, keymap: Keymap) -> Result<Self, AppError> {
        let tabs = TabManager::new(&shell).map_err(tabs_error)?;
        Ok(Self {
            shell,
            keymap,
            tabs,
            should_quit: false,
            ui_dirty: true,
        })
    }

    pub fn restore_or_new(shell: ShellProcessConfig) -> Result<Self, AppError> {
        let keymap = load_keymap().map_err(keymap_error)?;
        match load_state().map_err(state_error)? {
            Some(snapshot) => {
                let tabs = TabManager::from_snapshot(snapshot, &shell).map_err(tabs_error)?;
                Ok(Self {
                    shell,
                    keymap,
                    tabs,
                    should_quit: false,
                    ui_dirty: true,
                })
            }
            None => Self::new_with_keymap(shell, keymap),
        }
    }

    pub fn handle_key_event(
        &mut self,
        event: KeyEvent,
        clipboard: &mut dyn ClipboardBackend,
    ) -> Result<(), AppError> {
        if self.handle_terminal_navigation_key(event)? {
            return Ok(());
        }

        match map_key_event_with_keymap(event, &self.keymap) {
            InputAction::Ignore => {}
            InputAction::PtyBytes(bytes) => {
                self.tabs.write_to_active_pane(&bytes).map_err(tabs_error)?;
                self.ui_dirty |= self.refresh_all_panes_output().map_err(tabs_error)?;
            }
            InputAction::Command(command) => {
                self.handle_command(command, clipboard)?;
            }
        }

        Ok(())
    }

    fn handle_terminal_navigation_key(&mut self, event: KeyEvent) -> Result<bool, AppError> {
        if event.modifiers != KeyModifiers::NONE {
            return Ok(false);
        }

        match event.code {
            KeyCode::Home => {
                self.tabs.write_to_active_pane(b"\x1b[H").map_err(tabs_error)?;
                self.ui_dirty |= self.refresh_all_panes_output().map_err(tabs_error)?;
                Ok(true)
            }
            KeyCode::End => {
                if self.tabs.active_pane_is_scrolled_back().map_err(tabs_error)? {
                    self.tabs
                        .scroll_active_pane_to_bottom()
                        .map_err(tabs_error)?;
                    self.ui_dirty = true;
                } else {
                    self.tabs.write_to_active_pane(b"\x1b[F").map_err(tabs_error)?;
                    self.ui_dirty |= self.refresh_all_panes_output().map_err(tabs_error)?;
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn redraw<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), AppError> {
        self.ui_dirty |= self.refresh_all_panes_output().map_err(tabs_error)?;
        let content_area = terminal_content_area(terminal)?;
        self.tabs
            .resize_active_tab(content_area)
            .map_err(tabs_error)?;
        let frame_view = self.build_frame_view(content_area)?;
        render_frame(terminal, &frame_view).map_err(terminal_io_error)
    }

    pub fn save(&mut self) -> Result<(), AppError> {
        let snapshot = self.tabs.snapshot().map_err(tabs_error)?;
        save_state(&snapshot).map_err(state_error)
    }

    pub fn run<B: Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
        clipboard: &mut dyn ClipboardBackend,
    ) -> Result<(), AppError> {
        while !self.should_quit {
            self.ui_dirty |= self.refresh_all_panes_output().map_err(tabs_error)?;
            if self.ui_dirty {
                self.redraw(terminal)?;
                self.ui_dirty = false;
            }

            if event::poll(Duration::from_millis(50)).map_err(terminal_io_error)? {
                match event::read().map_err(terminal_io_error)? {
                    Event::Key(key_event) => self.handle_key_event(key_event, clipboard)?,
                    Event::Resize(cols, rows) => {
                        self.tabs
                            .resize_active_tab(mtrm_layout::Rect {
                                x: 0,
                                y: 0,
                                width: cols,
                                height: rows.saturating_sub(1),
                            })
                            .map_err(tabs_error)?;
                        self.ui_dirty = true;
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn handle_command(
        &mut self,
        command: AppCommand,
        clipboard: &mut dyn ClipboardBackend,
    ) -> Result<(), AppError> {
        match command {
            AppCommand::Clipboard(ClipboardCommand::CopySelection) => {
                let text = self.tabs.active_pane_text().map_err(tabs_error)?;
                clipboard.set_text(&text).map_err(clipboard_error)?;
            }
            AppCommand::Clipboard(ClipboardCommand::PasteFromSystem) => {
                let text = clipboard.get_text().map_err(clipboard_error)?;
                self.tabs
                    .write_to_active_pane(text.as_bytes())
                    .map_err(tabs_error)?;
                self.ui_dirty |= self.refresh_all_panes_output().map_err(tabs_error)?;
                self.save()?;
            }
            AppCommand::Layout(layout_command) => {
                let persist_state = should_persist_layout_command(&layout_command);
                self.handle_layout_command(layout_command)?;
                self.ui_dirty = true;
                if persist_state {
                    self.save()?;
                }
            }
            AppCommand::Tabs(tab_command) => {
                self.handle_tab_command(tab_command)?;
                self.ui_dirty = true;
                self.save()?;
            }
            AppCommand::SendInterrupt => {
                self.tabs
                    .send_interrupt_to_active_pane()
                    .map_err(tabs_error)?;
                self.ui_dirty = true;
            }
            AppCommand::RequestSave => {
                self.save()?;
            }
            AppCommand::Quit => {
                self.save()?;
                self.should_quit = true;
            }
        }

        Ok(())
    }

    fn handle_layout_command(&mut self, command: LayoutCommand) -> Result<(), AppError> {
        match command {
            LayoutCommand::SplitFocused(direction) => {
                let _ = self
                    .tabs
                    .split_active_pane(direction, &self.shell)
                    .map_err(tabs_error)?;
            }
            LayoutCommand::CloseFocusedPane => {
                let _ = self.tabs.close_active_pane().map_err(tabs_error)?;
            }
            LayoutCommand::MoveFocus(direction) => {
                self.tabs.move_focus(direction).map_err(tabs_error)?;
            }
            LayoutCommand::ScrollUpLines(lines) => {
                self.tabs
                    .scroll_active_pane_up_lines(lines)
                    .map_err(tabs_error)?;
            }
            LayoutCommand::ScrollDownLines(lines) => {
                self.tabs
                    .scroll_active_pane_down_lines(lines)
                    .map_err(tabs_error)?;
            }
            LayoutCommand::ScrollUpPages(pages) => {
                self.tabs
                    .scroll_active_pane_up_pages(pages)
                    .map_err(tabs_error)?;
            }
            LayoutCommand::ScrollDownPages(pages) => {
                self.tabs
                    .scroll_active_pane_down_pages(pages)
                    .map_err(tabs_error)?;
            }
            LayoutCommand::ScrollToBottom => {
                self.tabs
                    .scroll_active_pane_to_bottom()
                    .map_err(tabs_error)?;
            }
        }
        Ok(())
    }

    fn handle_tab_command(&mut self, command: TabCommand) -> Result<(), AppError> {
        match command {
            TabCommand::NewTab => {
                let _ = self.tabs.new_tab(&self.shell).map_err(tabs_error)?;
            }
            TabCommand::CloseCurrentTab => {
                self.tabs.close_active_tab().map_err(tabs_error)?;
            }
            TabCommand::NextTab => {
                let ids = self.tabs.tab_ids();
                let current = self.tabs.active_tab_id();
                if let Some(index) = ids.iter().position(|id| *id == current) {
                    let next = ids[(index + 1) % ids.len()];
                    self.tabs.activate_tab(next).map_err(tabs_error)?;
                }
            }
            TabCommand::PreviousTab => {
                let ids = self.tabs.tab_ids();
                let current = self.tabs.active_tab_id();
                if let Some(index) = ids.iter().position(|id| *id == current) {
                    let previous = ids[(index + ids.len() - 1) % ids.len()];
                    self.tabs.activate_tab(previous).map_err(tabs_error)?;
                }
            }
            TabCommand::Activate(tab_id) => {
                self.tabs.activate_tab(tab_id).map_err(tabs_error)?;
            }
        }
        Ok(())
    }

    fn refresh_all_panes_output(&mut self) -> Result<bool, mtrm_tabs::TabsError> {
        self.tabs.refresh_all_panes()
    }

    fn build_frame_view(&mut self, content_area: mtrm_layout::Rect) -> Result<FrameView, AppError> {
        let snapshot = self.tabs.snapshot().map_err(tabs_error)?;
        let active_tab = snapshot.active_tab;
        let active_tab_snapshot = snapshot
            .tabs
            .iter()
            .find(|tab| tab.id == active_tab)
            .ok_or_else(|| AppError::Tabs("active tab missing in snapshot".to_owned()))?;

        let tabs = snapshot
            .tabs
            .iter()
            .map(|tab| TabView {
                id: tab.id,
                title: tab.title.clone(),
                active: tab.id == active_tab,
            })
            .collect();

        let placements = self.tabs.placements(content_area).map_err(tabs_error)?;
        let panes = placements
            .into_iter()
            .map(|(id, area, focused)| PaneView {
                id,
                title: format!("pane-{}", id.get()),
                area,
                active: focused,
                lines: self
                    .tabs
                    .pane_lines(id)
                    .map_err(tabs_error)
                    .unwrap_or_default(),
                cursor: self.tabs.pane_cursor(id).ok().flatten(),
            })
            .collect();

        let _ = active_tab_snapshot;
        Ok(FrameView { tabs, panes })
    }
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("configuration error")]
    Config(String),
    #[error("state error")]
    State(String),
    #[error("tabs error")]
    Tabs(String),
    #[error("clipboard error")]
    Clipboard(String),
    #[error("terminal io error")]
    TerminalIo(String),
}

fn main() -> Result<(), AppError> {
    let shell = default_shell_config().map_err(|error| AppError::Config(error.to_string()))?;

    enable_raw_mode().map_err(terminal_io_error)?;
    let mut stdout = io::stdout();
    stdout
        .execute(EnterAlternateScreen)
        .map_err(terminal_io_error)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(terminal_io_error)?;
    let mut clipboard = SystemClipboard::new().map_err(clipboard_error)?;

    let result = (|| {
        let mut app = App::restore_or_new(shell)?;
        app.run(&mut terminal, &mut clipboard)
    })();

    let _ = disable_raw_mode();
    let _ = terminal.backend_mut().execute(LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result
}

fn default_shell_config() -> Result<ShellProcessConfig, io::Error> {
    let program = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned());
    let initial_cwd = std::env::current_dir()?;
    Ok(ShellProcessConfig {
        program: PathBuf::from(program),
        args: vec!["-i".to_owned()],
        initial_cwd,
    })
}

fn tabs_error(error: impl ToString) -> AppError {
    AppError::Tabs(error.to_string())
}

fn state_error(error: impl ToString) -> AppError {
    AppError::State(error.to_string())
}

fn clipboard_error(error: ClipboardError) -> AppError {
    AppError::Clipboard(error.to_string())
}

fn keymap_error(error: impl ToString) -> AppError {
    AppError::Config(error.to_string())
}

fn should_persist_layout_command(command: &LayoutCommand) -> bool {
    matches!(
        command,
        LayoutCommand::SplitFocused(_)
            | LayoutCommand::CloseFocusedPane
            | LayoutCommand::MoveFocus(_)
    )
}

fn terminal_io_error(error: impl ToString) -> AppError {
    AppError::TerminalIo(error.to_string())
}

fn terminal_content_area<B: Backend>(
    terminal: &Terminal<B>,
) -> Result<mtrm_layout::Rect, AppError> {
    let size = terminal.size().map_err(terminal_io_error)?;
    Ok(mtrm_layout::Rect {
        x: 0,
        y: 0,
        width: size.width,
        height: size.height.saturating_sub(1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEventKind, KeyEventState, KeyModifiers};
    use mtrm_clipboard::MemoryClipboard;
    use mtrm_core::{FocusMoveDirection, LayoutCommand};
    use ratatui::backend::TestBackend;
    use serial_test::serial;
    use std::fs;
    use std::thread;
    use std::time::Instant;
    use tempfile::tempdir;

    fn shell_config(initial_cwd: PathBuf) -> ShellProcessConfig {
        ShellProcessConfig {
            program: PathBuf::from("/bin/sh"),
            args: vec![],
            initial_cwd,
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
                    },
                    mtrm_session::PaneSnapshot {
                        id: mtrm_core::PaneId::new(11),
                        cwd: dir_b,
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
    #[serial]
    fn handle_key_event_paste_reads_clipboard_and_sends_to_shell() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();
        clipboard.set_text("printf '__PASTE_OK__\\n'\n").unwrap();

        with_test_home(&home, || {
            app.handle_key_event(
                key_event(KeyCode::Char('v'), KeyModifiers::CONTROL),
                &mut clipboard,
            )
        })
        .unwrap();

        let ok = with_test_home(&home, || {
            wait_until(Duration::from_secs(2), || {
                app.refresh_all_panes_output().is_ok()
                    && app
                        .tabs
                        .active_pane_text()
                        .map(|text| text.contains("__PASTE_OK__"))
                        .unwrap_or(false)
            })
        });
        assert!(ok);
    }

    #[test]
    #[serial]
    fn handle_key_event_regular_char_sends_bytes() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        with_test_home(&home, || {
            app.handle_key_event(
                key_event(KeyCode::Char('p'), KeyModifiers::NONE),
                &mut clipboard,
            )
            .unwrap();
            app.handle_key_event(
                key_event(KeyCode::Char('w'), KeyModifiers::NONE),
                &mut clipboard,
            )
            .unwrap();
            app.handle_key_event(
                key_event(KeyCode::Char('d'), KeyModifiers::NONE),
                &mut clipboard,
            )
            .unwrap();
            app.handle_key_event(
                key_event(KeyCode::Enter, KeyModifiers::NONE),
                &mut clipboard,
            )
            .unwrap();
        });

        let expected = temp.path().to_string_lossy().to_string();
        let ok = with_test_home(&home, || {
            wait_until(Duration::from_secs(2), || {
                app.refresh_all_panes_output().is_ok()
                    && app
                        .tabs
                        .active_pane_text()
                        .map(|text| text.contains(&expected))
                        .unwrap_or(false)
            })
        });
        assert!(ok);
    }

    #[test]
    #[serial]
    fn regular_input_does_not_persist_state_immediately() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        with_test_home(&home, || {
            app.handle_key_event(
                key_event(KeyCode::Char('x'), KeyModifiers::NONE),
                &mut clipboard,
            )
        })
        .unwrap();

        assert!(
            !home.join(".mtrm").join("state.toml").exists(),
            "plain PTY input must not trigger state save"
        );
    }

    #[test]
    fn handle_key_event_ctrl_shift_c_sends_interrupt() {
        let temp = tempdir().unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        app.tabs.write_to_active_pane(b"sleep 5\n").unwrap();
        thread::sleep(Duration::from_millis(150));
        app.handle_key_event(
            key_event(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ),
            &mut clipboard,
        )
        .unwrap();
        app.tabs
            .write_to_active_pane(b"printf '__APP_INTERRUPT__\\n'\n")
            .unwrap();

        let ok = wait_until(Duration::from_secs(3), || {
            app.refresh_all_panes_output().is_ok()
                && app
                    .tabs
                    .active_pane_text()
                    .map(|text| text.contains("__APP_INTERRUPT__"))
                    .unwrap_or(false)
        });
        assert!(ok);
    }

    #[test]
    #[serial]
    fn handle_key_event_alt_minus_splits_active_pane() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        with_test_home(&home, || {
            app.handle_key_event(
                key_event(KeyCode::Char('-'), KeyModifiers::ALT),
                &mut clipboard,
            )
        })
        .unwrap();

        let placements = app
            .tabs
            .placements(mtrm_layout::Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 30,
            })
            .unwrap();
        assert_eq!(placements.len(), 2);
    }

    #[test]
    #[serial]
    fn handle_key_event_alt_t_creates_new_tab() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        with_test_home(&home, || {
            app.handle_key_event(
                key_event(KeyCode::Char('t'), KeyModifiers::ALT),
                &mut clipboard,
            )
        })
        .unwrap();

        assert_eq!(app.tabs.tab_ids().len(), 2);
    }

    #[test]
    #[serial]
    fn shift_up_scrolls_without_persisting_state() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        app.tabs
            .resize_active_tab(mtrm_layout::Rect {
                x: 0,
                y: 0,
                width: 20,
                height: 6,
            })
            .unwrap();
        app.tabs
            .write_to_active_pane(
                b"i=1; while [ \"$i\" -le 20 ]; do printf 'line%s\\n' \"$i\"; i=$((i+1)); done\n",
            )
            .unwrap();
        let loaded = wait_until(Duration::from_secs(2), || {
            app.refresh_all_panes_output().unwrap_or(false)
                && app
                    .tabs
                    .active_pane_text()
                    .map(|text| text.contains("line20"))
                    .unwrap_or(false)
        });
        assert!(loaded);

        let before = app.tabs.active_pane_text().unwrap();
        with_test_home(&home, || {
            app.handle_key_event(key_event(KeyCode::Up, KeyModifiers::SHIFT), &mut clipboard)
        })
        .unwrap();
        let after = app.tabs.active_pane_text().unwrap();

        assert_ne!(before, after);
        assert!(!home.join(".mtrm").join("state.toml").exists());
    }

    #[test]
    fn end_returns_scrolled_view_to_bottom() {
        let temp = tempdir().unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        app.tabs
            .resize_active_tab(mtrm_layout::Rect {
                x: 0,
                y: 0,
                width: 20,
                height: 6,
            })
            .unwrap();
        app.tabs
            .write_to_active_pane(
                b"i=1; while [ \"$i\" -le 20 ]; do printf 'line%s\\n' \"$i\"; i=$((i+1)); done\n",
            )
            .unwrap();
        let loaded = wait_until(Duration::from_secs(2), || {
            app.refresh_all_panes_output().unwrap_or(false)
                && app
                    .tabs
                    .active_pane_text()
                    .map(|text| text.contains("line20"))
                    .unwrap_or(false)
        });
        assert!(loaded);

        app.handle_key_event(
            key_event(KeyCode::PageUp, KeyModifiers::SHIFT),
            &mut clipboard,
        )
        .unwrap();
        let scrolled = app.tabs.active_pane_text().unwrap();
        assert!(!scrolled.contains("line20"));

        app.handle_key_event(key_event(KeyCode::End, KeyModifiers::NONE), &mut clipboard)
            .unwrap();
        let reset = app.tabs.active_pane_text().unwrap();
        assert!(reset.contains("line20"));
    }

    #[test]
    fn plain_left_arrow_moves_shell_cursor_left() {
        let temp = tempdir().unwrap();
        let previous_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let ok = with_env_var("SHELL", "/bin/bash", || {
            let shell = default_shell_config().unwrap();
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
        });

        std::env::set_current_dir(previous_dir).unwrap();
        assert!(ok, "left arrow must move shell cursor left before Enter");
    }

    #[test]
    #[serial]
    fn plain_home_moves_shell_cursor_to_start_of_line() {
        let temp = tempdir().unwrap();
        let previous_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let ok = with_env_var("SHELL", "/bin/bash", || {
            let shell = default_shell_config().unwrap();
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
        });

        std::env::set_current_dir(previous_dir).unwrap();
        assert!(ok, "home must move shell cursor to the beginning of the line");
    }

    #[test]
    #[serial]
    fn plain_end_moves_shell_cursor_to_end_of_line_when_not_scrolled() {
        let temp = tempdir().unwrap();
        let previous_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let ok = with_env_var("SHELL", "/bin/bash", || {
            let shell = default_shell_config().unwrap();
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
        });

        std::env::set_current_dir(previous_dir).unwrap();
        assert!(ok, "end must move shell cursor to the end of the line when pane is not scrolled");
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

        assert!(home.join(".mtrm").join("state.toml").is_file());
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
            AppError::State("failed to write /tmp/secret/state.toml: permission denied".to_owned());

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
        assert!(home.join(".mtrm").join("state.toml").is_file());
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

        assert!(result.is_err());
        assert!(!app.should_quit);
    }

    #[test]
    #[serial]
    fn startup_shows_initial_shell_output_for_default_shell_config() {
        let temp = tempdir().unwrap();
        let previous_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let ok = with_env_var("SHELL", "/bin/bash", || {
            let shell = default_shell_config().unwrap();
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

        std::env::set_current_dir(previous_dir).unwrap();
        assert!(ok, "default shell startup must show visible shell output");
    }

    #[test]
    #[serial]
    fn startup_shell_echoes_typed_characters_before_enter() {
        let temp = tempdir().unwrap();
        let previous_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let ok = with_env_var("SHELL", "/bin/bash", || {
            let shell = default_shell_config().unwrap();
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

        std::env::set_current_dir(previous_dir).unwrap();
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
        let changed = wait_until(Duration::from_secs(2), || {
            app.tabs
                .active_pane_cwd()
                .map(|cwd| cwd == pane_dir)
                .unwrap_or(false)
        });
        assert!(changed);

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
}
