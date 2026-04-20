use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;
use std::time::UNIX_EPOCH;

use crossterm::ExecutableCommand;
use crossterm::event::{
    self, DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture, Event,
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use mtrm_clipboard::{ClipboardBackend, ClipboardError, SystemClipboard, UnavailableClipboard};
use mtrm_core::{AppCommand, ClipboardCommand, LayoutCommand, TabCommand};
use mtrm_input::{InputAction, map_key_event_with_keymap};
use mtrm_keymap::{Keymap, load_keymap};
use mtrm_process::ShellProcessConfig;
use mtrm_state::{load_state, save_state};
use mtrm_tabs::TabManager;
use mtrm_ui::{
    ClipboardNoticeView, FrameView, ModalView, PaneSelectionView, PaneView, TAB_DIVIDER, TabView,
    render_frame,
};
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::backend::CrosstermBackend;
use thiserror::Error;

const ALT_PREFIX_TIMEOUT: Duration = Duration::from_millis(80);
const CLIPBOARD_NOTICE_TEXT: &str = "Буфер обмена недоступен";
const CLIPBOARD_NOTICE_TTL: Duration = Duration::from_secs(3);

pub struct App {
    shell: ShellProcessConfig,
    keymap: Keymap,
    tabs: TabManager,
    selection: Option<SelectionState>,
    should_quit: bool,
    ui_dirty: bool,
    window_focused: bool,
    pending_alt_prefix_started_at: Option<Instant>,
    rename: Option<RenameState>,
    clipboard_notice: Option<UiNotice>,
    last_content_area: mtrm_layout::Rect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CliAction {
    Run,
    PrintHelp,
    PrintVersion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    action: CliAction,
    debug_log_path: Option<PathBuf>,
    disable_clipboard: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectionPoint {
    row: u16,
    col: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectionState {
    pane_id: mtrm_core::PaneId,
    anchor: SelectionPoint,
    focus: SelectionPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PaneSelectionTarget {
    pane_id: mtrm_core::PaneId,
    point: SelectionPoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RenameTarget {
    Tab(mtrm_core::TabId),
    Pane(mtrm_core::PaneId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenameState {
    target: RenameTarget,
    input: String,
    cursor: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LayoutCommandResult {
    persist: bool,
    ui_dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UiNotice {
    text: String,
    shown_at: Instant,
}

const DEFAULT_CONTENT_AREA: mtrm_layout::Rect = mtrm_layout::Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 23,
};

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
            selection: None,
            should_quit: false,
            ui_dirty: true,
            window_focused: true,
            pending_alt_prefix_started_at: None,
            rename: None,
            clipboard_notice: None,
            last_content_area: DEFAULT_CONTENT_AREA,
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
                    selection: None,
                    should_quit: false,
                    ui_dirty: true,
                    window_focused: true,
                    pending_alt_prefix_started_at: None,
                    rename: None,
                    clipboard_notice: None,
                    last_content_area: DEFAULT_CONTENT_AREA,
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
        if self.rename.is_some() {
            return self.handle_rename_key_event(event);
        }

        let Some(event) = self.resolve_alt_prefixed_key_event(event)? else {
            return Ok(());
        };

        if is_start_rename_tab_event(event, &self.keymap) {
            self.open_rename_tab_modal();
            return Ok(());
        }
        if is_start_rename_pane_event(event, &self.keymap) {
            self.open_rename_pane_modal();
            return Ok(());
        }

        if self.handle_terminal_navigation_key(event)? {
            return Ok(());
        }

        match map_key_event_with_keymap(event, &self.keymap) {
            InputAction::Ignore => {}
            InputAction::PtyBytes(bytes) => {
                self.clear_selection();
                self.tabs.write_to_active_pane(&bytes).map_err(tabs_error)?;
                self.ui_dirty |= self.refresh_all_panes_output().map_err(tabs_error)?;
            }
            InputAction::Command(command) => {
                self.handle_command(command, clipboard)?;
            }
        }

        Ok(())
    }

    fn handle_mouse_event(
        &mut self,
        event: MouseEvent,
        content_area: mtrm_layout::Rect,
    ) -> Result<(), AppError> {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(tab_id) = self.tab_id_at_mouse_column(content_area.width, event) {
                    self.clear_selection();
                    self.tabs.activate_tab(tab_id).map_err(tabs_error)?;
                    self.ui_dirty = true;
                    self.save()?;
                } else if let Some(target) = self.selection_target_at(content_area, event) {
                    self.tabs.focus_pane(target.pane_id).map_err(tabs_error)?;
                    self.selection = Some(SelectionState {
                        pane_id: target.pane_id,
                        anchor: target.point,
                        focus: target.point,
                    });
                } else {
                    self.clear_selection();
                }
                self.ui_dirty = true;
            }
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) => {
                if let Some(target) = self.drag_selection_target(content_area, event) {
                    if let Some(selection) = &mut self.selection {
                        selection.focus = target.point;
                        self.ui_dirty = true;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_terminal_navigation_key(&mut self, event: KeyEvent) -> Result<bool, AppError> {
        if event.modifiers != KeyModifiers::NONE {
            return Ok(false);
        }

        match event.code {
            KeyCode::Home => {
                self.tabs
                    .write_to_active_pane(b"\x1b[H")
                    .map_err(tabs_error)?;
                self.ui_dirty |= self.refresh_all_panes_output().map_err(tabs_error)?;
                Ok(true)
            }
            KeyCode::End => {
                if self
                    .tabs
                    .active_pane_is_scrolled_back()
                    .map_err(tabs_error)?
                {
                    self.tabs
                        .scroll_active_pane_to_bottom()
                        .map_err(tabs_error)?;
                    self.ui_dirty = true;
                } else {
                    self.tabs
                        .write_to_active_pane(b"\x1b[F")
                        .map_err(tabs_error)?;
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
        self.last_content_area = content_area;
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
            self.flush_pending_alt_prefix_if_expired()?;
            self.ui_dirty |= self.refresh_all_panes_output().map_err(tabs_error)?;
            if self.ui_dirty {
                self.redraw(terminal)?;
                self.ui_dirty = false;
            }

            if event::poll(Duration::from_millis(50)).map_err(terminal_io_error)? {
                match event::read().map_err(terminal_io_error)? {
                    Event::Key(key_event) => self.handle_key_event(key_event, clipboard)?,
                    Event::Mouse(mouse_event) => {
                        let content_area = terminal_content_area(terminal)?;
                        self.handle_mouse_event(mouse_event, content_area)?;
                    }
                    Event::Resize(cols, rows) => {
                        self.clear_selection();
                        self.tabs
                            .resize_active_tab(mtrm_layout::Rect {
                                x: 0,
                                y: 0,
                                width: cols,
                                height: rows.saturating_sub(1),
                            })
                            .map_err(tabs_error)?;
                        self.last_content_area = mtrm_layout::Rect {
                            x: 0,
                            y: 0,
                            width: cols,
                            height: rows.saturating_sub(1),
                        };
                        self.ui_dirty = true;
                    }
                    Event::FocusGained => {
                        self.window_focused = true;
                        self.ui_dirty = true;
                    }
                    Event::FocusLost => {
                        self.window_focused = false;
                        self.ui_dirty = true;
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn resolve_alt_prefixed_key_event(
        &mut self,
        event: KeyEvent,
    ) -> Result<Option<KeyEvent>, AppError> {
        if let Some(started_at) = self.pending_alt_prefix_started_at.take() {
            if started_at.elapsed() <= ALT_PREFIX_TIMEOUT {
                if let Some(synthetic) = synthesize_alt_prefixed_key_event(event) {
                    return Ok(Some(synthetic));
                }
            }

            self.tabs
                .write_to_active_pane(b"\x1b")
                .map_err(tabs_error)?;
            self.ui_dirty |= self.refresh_all_panes_output().map_err(tabs_error)?;
        }

        if event.modifiers == KeyModifiers::NONE && matches!(event.code, KeyCode::Esc) {
            self.pending_alt_prefix_started_at = Some(Instant::now());
            return Ok(None);
        }

        Ok(Some(event))
    }

    fn flush_pending_alt_prefix_if_expired(&mut self) -> Result<(), AppError> {
        let Some(started_at) = self.pending_alt_prefix_started_at else {
            return Ok(());
        };
        if started_at.elapsed() <= ALT_PREFIX_TIMEOUT {
            return Ok(());
        }

        self.pending_alt_prefix_started_at = None;
        self.tabs
            .write_to_active_pane(b"\x1b")
            .map_err(tabs_error)?;
        self.ui_dirty |= self.refresh_all_panes_output().map_err(tabs_error)?;
        Ok(())
    }

    fn handle_command(
        &mut self,
        command: AppCommand,
        clipboard: &mut dyn ClipboardBackend,
    ) -> Result<(), AppError> {
        match command {
            AppCommand::Clipboard(ClipboardCommand::CopySelection) => {
                if let Some(selection) = self.selection.filter(|selection| !selection.is_empty()) {
                    let text = self
                        .tabs
                        .pane_selection_text(
                            selection.pane_id,
                            (selection.anchor.row, selection.anchor.col),
                            (selection.focus.row, selection.focus.col),
                        )
                        .map_err(tabs_error)?;
                    match clipboard.set_text(&text) {
                        Ok(()) => {}
                        Err(ClipboardError::Unavailable) => {
                            self.show_clipboard_unavailable_notice();
                        }
                        Err(error) => return Err(clipboard_error(error)),
                    }
                }
            }
            AppCommand::Clipboard(ClipboardCommand::PasteFromSystem) => {
                self.clear_selection();
                let text = match clipboard.get_text() {
                    Ok(text) => text,
                    Err(ClipboardError::Unavailable) => {
                        self.show_clipboard_unavailable_notice();
                        return Ok(());
                    }
                    Err(error) => return Err(clipboard_error(error)),
                };
                self.tabs
                    .write_to_active_pane(text.as_bytes())
                    .map_err(tabs_error)?;
                self.ui_dirty |= self.refresh_all_panes_output().map_err(tabs_error)?;
                self.save()?;
            }
            AppCommand::Layout(layout_command) => {
                self.clear_selection();
                let result = self.handle_layout_command(layout_command)?;
                self.ui_dirty |= result.ui_dirty;
                if result.persist {
                    self.save()?;
                }
            }
            AppCommand::Tabs(tab_command) => {
                self.clear_selection();
                self.handle_tab_command(tab_command)?;
                self.ui_dirty = true;
                self.save()?;
            }
            AppCommand::SendInterrupt => {
                self.clear_selection();
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

    fn show_clipboard_unavailable_notice(&mut self) {
        let now = Instant::now();
        let should_refresh = self
            .clipboard_notice
            .as_ref()
            .map(|notice| now.duration_since(notice.shown_at) >= CLIPBOARD_NOTICE_TTL)
            .unwrap_or(true);

        if should_refresh {
            self.clipboard_notice = Some(UiNotice {
                text: CLIPBOARD_NOTICE_TEXT.to_owned(),
                shown_at: now,
            });
        }
        self.ui_dirty = true;
    }

    fn handle_layout_command(&mut self, command: LayoutCommand) -> Result<LayoutCommandResult, AppError> {
        let result = match command {
            LayoutCommand::SplitFocused(direction) => {
                let _ = self
                    .tabs
                    .split_active_pane(direction, &self.shell)
                    .map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: true,
                    ui_dirty: true,
                }
            }
            LayoutCommand::CloseFocusedPane => {
                let _ = self.tabs.close_active_pane().map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: true,
                    ui_dirty: true,
                }
            }
            LayoutCommand::MoveFocus(direction) => {
                self.tabs.move_focus(direction).map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: true,
                    ui_dirty: true,
                }
            }
            LayoutCommand::ResizeFocused(direction) => {
                let changed = self
                    .tabs
                    .resize_active_pane(direction, self.last_content_area)
                    .map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: changed,
                    ui_dirty: changed,
                }
            }
            LayoutCommand::ScrollUpLines(lines) => {
                self.append_debug_log_event(&format!("SCROLL_UP_LINES lines={lines}"));
                self.tabs
                    .scroll_active_pane_up_lines(lines)
                    .map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: false,
                    ui_dirty: true,
                }
            }
            LayoutCommand::ScrollDownLines(lines) => {
                self.append_debug_log_event(&format!("SCROLL_DOWN_LINES lines={lines}"));
                self.tabs
                    .scroll_active_pane_down_lines(lines)
                    .map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: false,
                    ui_dirty: true,
                }
            }
            LayoutCommand::ScrollUpPages(pages) => {
                self.append_debug_log_event(&format!("SCROLL_UP_PAGES pages={pages}"));
                self.tabs
                    .scroll_active_pane_up_pages(pages)
                    .map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: false,
                    ui_dirty: true,
                }
            }
            LayoutCommand::ScrollDownPages(pages) => {
                self.append_debug_log_event(&format!("SCROLL_DOWN_PAGES pages={pages}"));
                self.tabs
                    .scroll_active_pane_down_pages(pages)
                    .map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: false,
                    ui_dirty: true,
                }
            }
            LayoutCommand::ScrollToBottom => {
                self.append_debug_log_event("SCROLL_TO_BOTTOM");
                self.tabs
                    .scroll_active_pane_to_bottom()
                    .map_err(tabs_error)?;
                LayoutCommandResult {
                    persist: false,
                    ui_dirty: true,
                }
            }
        };
        Ok(result)
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

    fn open_rename_tab_modal(&mut self) {
        let input = self.tabs.active_tab_title().to_owned();
        let cursor = input.chars().count();
        self.rename = Some(RenameState {
            target: RenameTarget::Tab(self.tabs.active_tab_id()),
            input,
            cursor,
        });
        self.ui_dirty = true;
    }

    fn open_rename_pane_modal(&mut self) {
        let input = self
            .tabs
            .active_pane_title()
            .map(|title| title.to_owned())
            .unwrap_or_else(|_| format!("pane-{}", self.tabs.active_pane_id().get()));
        let cursor = input.chars().count();
        self.rename = Some(RenameState {
            target: RenameTarget::Pane(self.tabs.active_pane_id()),
            input,
            cursor,
        });
        self.ui_dirty = true;
    }

    fn handle_rename_key_event(&mut self, event: KeyEvent) -> Result<(), AppError> {
        let Some(state) = &mut self.rename else {
            return Ok(());
        };

        match event.code {
            KeyCode::Esc => {
                self.rename = None;
                self.ui_dirty = true;
            }
            KeyCode::Enter => {
                let target = state.target.clone();
                let title = match target {
                    RenameTarget::Tab(tab_id) => normalized_tab_title(tab_id, &state.input),
                    RenameTarget::Pane(pane_id) => normalized_pane_title(pane_id, &state.input),
                };
                match target {
                    RenameTarget::Tab(tab_id) => {
                        self.tabs.rename_tab(tab_id, title).map_err(tabs_error)?;
                    }
                    RenameTarget::Pane(pane_id) => {
                        self.tabs.rename_pane(pane_id, title).map_err(tabs_error)?;
                    }
                }
                self.rename = None;
                self.ui_dirty = true;
                self.save()?;
            }
            KeyCode::Backspace => {
                if state.cursor > 0 {
                    let remove_at = state.cursor - 1;
                    state.input = remove_char_at(&state.input, remove_at);
                    state.cursor -= 1;
                    self.ui_dirty = true;
                }
            }
            KeyCode::Left => {
                state.cursor = state.cursor.saturating_sub(1);
                self.ui_dirty = true;
            }
            KeyCode::Right => {
                let len = state.input.chars().count();
                state.cursor = (state.cursor + 1).min(len);
                self.ui_dirty = true;
            }
            KeyCode::Home => {
                state.cursor = 0;
                self.ui_dirty = true;
            }
            KeyCode::End => {
                state.cursor = state.input.chars().count();
                self.ui_dirty = true;
            }
            KeyCode::Char(ch) if !event.modifiers.contains(KeyModifiers::CONTROL) => {
                state.input = insert_char_at(&state.input, state.cursor, ch);
                state.cursor += 1;
                self.ui_dirty = true;
            }
            _ => {}
        }

        Ok(())
    }

    fn append_debug_log_event(&self, event: &str) {
        let Some(path) = &self.shell.debug_log_path else {
            return;
        };

        let timestamp = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);

        let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        else {
            return;
        };

        let _ = writeln!(file, "[{timestamp}] MTRM_EVENT {event}");
        let _ = file.flush();
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
                title: self
                    .tabs
                    .pane_title(id)
                    .map(|title| title.to_owned())
                    .map_err(tabs_error)
                    .unwrap_or_else(|_| format!("pane-{}", id.get())),
                area,
                active: focused,
                lines: self
                    .tabs
                    .pane_lines(id)
                    .map_err(tabs_error)
                    .unwrap_or_default(),
                selection: self.selection.and_then(|selection| selection.view_for(id)),
                cursor: self.tabs.pane_cursor(id).ok().flatten(),
            })
            .collect();

        let modal = self.rename.as_ref().map(|rename| ModalView {
            title: match rename.target {
                RenameTarget::Tab(_) => "Rename Tab".to_owned(),
                RenameTarget::Pane(_) => "Rename Pane".to_owned(),
            },
            input: rename.input.clone(),
            cursor: rename.cursor,
            hint: "Enter apply, Esc cancel".to_owned(),
        });
        let clipboard_notice = self
            .clipboard_notice
            .as_ref()
            .filter(|notice| notice.shown_at.elapsed() < CLIPBOARD_NOTICE_TTL)
            .map(|notice| ClipboardNoticeView {
                text: notice.text.clone(),
            });

        let _ = active_tab_snapshot;
        Ok(FrameView {
            tabs,
            panes,
            focused: self.window_focused,
            clipboard_notice,
            modal,
        })
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
    let cli = parse_cli_args(std::env::args())?;

    match cli.action {
        CliAction::Run => {}
        CliAction::PrintHelp => {
            print_help();
            return Ok(());
        }
        CliAction::PrintVersion => {
            println!("{}", cli_version_string());
            return Ok(());
        }
    }

    let shell = default_shell_config(cli.debug_log_path)
        .map_err(|error| AppError::Config(error.to_string()))?;

    enable_raw_mode().map_err(terminal_io_error)?;
    let mut stdout = io::stdout();
    stdout
        .execute(EnterAlternateScreen)
        .map_err(terminal_io_error)?;
    stdout
        .execute(EnableFocusChange)
        .map_err(terminal_io_error)?;
    stdout
        .execute(EnableMouseCapture)
        .map_err(terminal_io_error)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(terminal_io_error)?;
    let mut clipboard = build_clipboard(cli.disable_clipboard);

    let result = (|| {
        let mut app = App::restore_or_new(shell)?;
        app.run(&mut terminal, &mut *clipboard)
    })();

    let _ = disable_raw_mode();
    let _ = terminal.backend_mut().execute(DisableFocusChange);
    let _ = terminal.backend_mut().execute(DisableMouseCapture);
    let _ = terminal.backend_mut().execute(LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result
}

fn is_start_rename_tab_event(event: KeyEvent, keymap: &Keymap) -> bool {
    event.modifiers == (KeyModifiers::ALT | KeyModifiers::SHIFT)
        && matches!(event.code, KeyCode::Char(ch) if keymap.matches_rename_tab(ch))
}

fn is_start_rename_pane_event(event: KeyEvent, keymap: &Keymap) -> bool {
    event.modifiers == (KeyModifiers::ALT | KeyModifiers::SHIFT)
        && matches!(event.code, KeyCode::Char(ch) if keymap.matches_rename_pane(ch))
}

fn remove_char_at(input: &str, index: usize) -> String {
    input
        .chars()
        .enumerate()
        .filter_map(|(i, ch)| (i != index).then_some(ch))
        .collect()
}

fn insert_char_at(input: &str, index: usize, ch: char) -> String {
    let mut result = String::new();
    let mut inserted = false;
    for (i, existing) in input.chars().enumerate() {
        if i == index {
            result.push(ch);
            inserted = true;
        }
        result.push(existing);
    }
    if !inserted {
        result.push(ch);
    }
    result
}

fn normalized_tab_title(tab_id: mtrm_core::TabId, input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        format!("Tab {}", tab_id.get() + 1)
    } else {
        trimmed.to_owned()
    }
}

fn normalized_pane_title(pane_id: mtrm_core::PaneId, input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        format!("pane-{}", pane_id.get())
    } else {
        trimmed.to_owned()
    }
}

fn default_shell_config(debug_log_path: Option<PathBuf>) -> Result<ShellProcessConfig, io::Error> {
    let program = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned());
    let initial_cwd = std::env::current_dir()?;
    Ok(ShellProcessConfig {
        program: PathBuf::from(program),
        args: vec!["-i".to_owned()],
        initial_cwd,
        debug_log_path,
    })
}

fn parse_cli_args<I>(args: I) -> Result<CliOptions, AppError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let _ = args.next();

    let mut action = CliAction::Run;
    let mut debug_log_path = None;
    let mut disable_clipboard = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => action = CliAction::PrintHelp,
            "-v" | "--version" => action = CliAction::PrintVersion,
            "--no-clipboard" => disable_clipboard = true,
            "--debug-log" => {
                let path = args
                    .next()
                    .ok_or_else(|| AppError::Config("missing value for --debug-log".to_owned()))?;
                debug_log_path = Some(PathBuf::from(path));
            }
            _ => {
                return Err(AppError::Config(format!("unknown argument: {arg}")));
            }
        }
    }

    Ok(CliOptions {
        action,
        debug_log_path,
        disable_clipboard,
    })
}

fn print_help() {
    println!("{}", help_text());
}

fn help_text() -> String {
    "\
mtrm

Usage:
  mtrm
  mtrm -h | --help
  mtrm -v | --version
  mtrm [--no-clipboard]
  mtrm [--debug-log PATH]

Options:
  -h, --help       Print this help and exit
  -v, --version    Print version and exit
  --no-clipboard   Disable system clipboard integration
  --debug-log PATH Append raw PTY output chunks to PATH for debugging

Keybindings:
  Ctrl+C           Copy selection
  Ctrl+V           Paste from system clipboard
  Alt+X            Send interrupt to active process
  Alt+-            Split active pane left/right
  Alt+=            Split active pane top/bottom
  Alt+Q            Close active pane
  Alt+T            New tab
  Alt+,            Previous tab
  Alt+.            Next tab
  Alt+W            Close current tab
  Alt+Shift+R      Rename current tab
  Alt+Shift+E      Rename current pane
  Alt+Shift+Left   Resize pane left
  Alt+Shift+Right  Resize pane right
  Alt+Shift+Up     Resize pane up
  Alt+Shift+Down   Resize pane down
  Alt+Shift+Q      Save state and quit
  Alt+Left         Focus pane left
  Alt+Right        Focus pane right
  Alt+Up           Focus pane up
  Alt+Down         Focus pane down
  Shift+Up         Scroll pane history up
  Shift+Down       Scroll pane history down
  Shift+PageUp     Scroll pane history up by one page
  Shift+PageDown   Scroll pane history down by one page
  End              Return scrollback to live bottom

Notes:
  Letter-based Alt shortcuts come from ~/.mtrm/keymap.toml.
  Arrow and scrollback bindings are built in.
"
    .to_owned()
}

fn cli_version_string() -> String {
    // Версию для CLI считаем так:
    // 1. `app/build.rs` берет последний git tag через `git describe --tags --abbrev=0`.
    // 2. Суффикс через пробел считается во время запуска как mtime текущего исполняемого файла.
    // 3. Поэтому после реальной переустановки бинаря секунда должна меняться вместе с файлом.
    // 4. Если mtime получить не удалось, fallback идет на `0`.
    format!("{} {}", env!("MTRM_GIT_TAG"), executable_mtime_secs())
}

fn executable_mtime_secs() -> u64 {
    std::env::current_exe()
        .ok()
        .and_then(|path| std::fs::metadata(path).ok())
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
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

fn build_clipboard(disable_clipboard: bool) -> Box<dyn ClipboardBackend> {
    if disable_clipboard {
        return Box::new(UnavailableClipboard);
    }

    match SystemClipboard::new() {
        Ok(clipboard) => Box::new(clipboard),
        Err(_) => Box::new(UnavailableClipboard),
    }
}

fn keymap_error(error: impl ToString) -> AppError {
    AppError::Config(error.to_string())
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

impl SelectionState {
    fn is_empty(&self) -> bool {
        self.anchor == self.focus
    }

    fn normalized(&self) -> (SelectionPoint, SelectionPoint) {
        if (self.anchor.row, self.anchor.col) <= (self.focus.row, self.focus.col) {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }

    fn view_for(&self, pane_id: mtrm_core::PaneId) -> Option<PaneSelectionView> {
        if self.pane_id != pane_id || self.is_empty() {
            return None;
        }
        let (start, end) = self.normalized();
        Some(PaneSelectionView {
            start: (start.row, start.col),
            end: (end.row, end.col),
        })
    }
}

impl App {
    fn clear_selection(&mut self) {
        self.selection = None;
    }

    fn selection_target_at(
        &self,
        content_area: mtrm_layout::Rect,
        event: MouseEvent,
    ) -> Option<PaneSelectionTarget> {
        self.tabs
            .placements(content_area)
            .ok()?
            .into_iter()
            .find_map(|(pane_id, area, _)| {
                point_in_pane_content(area, pane_id, event.column, event.row)
            })
    }

    fn drag_selection_target(
        &self,
        content_area: mtrm_layout::Rect,
        event: MouseEvent,
    ) -> Option<PaneSelectionTarget> {
        let selection = self.selection?;
        let (_, pane_area, _) = self
            .tabs
            .placements(content_area)
            .ok()?
            .into_iter()
            .find(|(pane_id, _, _)| *pane_id == selection.pane_id)?;
        point_in_or_clamped_to_pane_content(pane_area, selection.pane_id, event.column, event.row)
    }

    fn tab_id_at_mouse_column(
        &self,
        terminal_width: u16,
        event: MouseEvent,
    ) -> Option<mtrm_core::TabId> {
        tab_id_at_position(
            &self.tabs.tab_summaries(),
            terminal_width,
            event.column,
            event.row,
        )
    }
}

fn point_in_pane_content(
    area: mtrm_layout::Rect,
    pane_id: mtrm_core::PaneId,
    column: u16,
    row: u16,
) -> Option<PaneSelectionTarget> {
    let rect = pane_content_rect(area)?;
    if column < rect.x || column >= rect.x.saturating_add(rect.width) {
        return None;
    }
    if row < rect.y || row >= rect.y.saturating_add(rect.height) {
        return None;
    }
    Some(PaneSelectionTarget {
        pane_id,
        point: SelectionPoint {
            row: row.saturating_sub(rect.y),
            col: column.saturating_sub(rect.x),
        },
    })
}

fn point_in_or_clamped_to_pane_content(
    area: mtrm_layout::Rect,
    pane_id: mtrm_core::PaneId,
    column: u16,
    row: u16,
) -> Option<PaneSelectionTarget> {
    let rect = pane_content_rect(area)?;
    let max_x = rect.x.saturating_add(rect.width.saturating_sub(1));
    let max_y = rect.y.saturating_add(rect.height.saturating_sub(1));
    let clamped_col = column.clamp(rect.x, max_x);
    let clamped_row = row.clamp(rect.y, max_y);
    Some(PaneSelectionTarget {
        pane_id,
        point: SelectionPoint {
            row: clamped_row.saturating_sub(rect.y),
            col: clamped_col.saturating_sub(rect.x),
        },
    })
}

fn pane_content_rect(area: mtrm_layout::Rect) -> Option<mtrm_layout::Rect> {
    if area.width <= 2 || area.height <= 2 {
        return None;
    }
    Some(mtrm_layout::Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(2),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    })
}

fn tab_id_at_position(
    tabs: &[mtrm_tabs::RuntimeTabSummary],
    terminal_width: u16,
    column: u16,
    row: u16,
) -> Option<mtrm_core::TabId> {
    if row != 0 || tabs.is_empty() || column >= terminal_width {
        return None;
    }

    let divider_width = TAB_DIVIDER.chars().count().min(u16::MAX as usize) as u16;
    let mut x = 0_u16;
    for (index, tab) in tabs.iter().enumerate() {
        let title_width = tab.title.chars().count().min(u16::MAX as usize) as u16;
        let end = x.saturating_add(title_width);
        if column >= x && column < end {
            return Some(tab.id);
        }

        x = end;
        if index + 1 < tabs.len() {
            if column >= x && column < x.saturating_add(divider_width) {
                return None;
            }
            x = x.saturating_add(divider_width);
        }
    }

    None
}

fn synthesize_alt_prefixed_key_event(event: KeyEvent) -> Option<KeyEvent> {
    let modifiers = match event.modifiers {
        KeyModifiers::NONE => KeyModifiers::ALT,
        KeyModifiers::SHIFT => KeyModifiers::ALT | KeyModifiers::SHIFT,
        _ => return None,
    };

    match event.code {
        KeyCode::Char(_) | KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
            Some(KeyEvent {
                code: event.code,
                modifiers,
                kind: event.kind,
                state: event.state,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{
        KeyCode, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEventKind,
    };
    use mtrm_clipboard::{MemoryClipboard, UnavailableClipboard};
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
    fn cli_version_string_uses_git_tag_and_build_suffix() {
        let version = cli_version_string();
        let (tag, suffix) = version.split_once(' ').unwrap();

        assert!(tag.starts_with('v'));
        assert!(!suffix.is_empty());
        assert!(suffix.chars().all(|ch| ch.is_ascii_digit()));
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

    #[test]
    #[serial]
    fn rename_tab_modal_applies_title_and_persists_it() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let mut app = App::new(shell_config(home.clone())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        with_test_home(&home, || {
            app.handle_key_event(
                key_event(KeyCode::Char('R'), KeyModifiers::ALT | KeyModifiers::SHIFT),
                &mut clipboard,
            )
        })
        .unwrap();
        for _ in 0..5 {
            with_test_home(&home, || {
                app.handle_key_event(key_event(KeyCode::Backspace, KeyModifiers::NONE), &mut clipboard)
            })
            .unwrap();
        }
        with_test_home(&home, || {
            app.handle_key_event(key_event(KeyCode::Char('b'), KeyModifiers::NONE), &mut clipboard)
        })
        .unwrap();
        with_test_home(&home, || {
            app.handle_key_event(key_event(KeyCode::Char('u'), KeyModifiers::NONE), &mut clipboard)
        })
        .unwrap();
        with_test_home(&home, || {
            app.handle_key_event(key_event(KeyCode::Char('i'), KeyModifiers::NONE), &mut clipboard)
        })
        .unwrap();
        with_test_home(&home, || {
            app.handle_key_event(key_event(KeyCode::Char('l'), KeyModifiers::NONE), &mut clipboard)
        })
        .unwrap();
        with_test_home(&home, || {
            app.handle_key_event(key_event(KeyCode::Char('d'), KeyModifiers::NONE), &mut clipboard)
        })
        .unwrap();
        with_test_home(&home, || {
            app.handle_key_event(key_event(KeyCode::Enter, KeyModifiers::NONE), &mut clipboard)
        })
        .unwrap();

        assert_eq!(app.tabs.active_tab_title(), "build");
        assert!(app.rename.is_none());

        let restored =
            with_test_home(&home, || App::restore_or_new(shell_config(home.clone()))).unwrap();
        assert_eq!(restored.tabs.active_tab_title(), "build");
    }

    #[test]
    fn rename_tab_modal_esc_cancels_changes() {
        let temp = tempdir().unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        app.open_rename_tab_modal();
        app.handle_key_event(key_event(KeyCode::Char('x'), KeyModifiers::NONE), &mut clipboard)
            .unwrap();
        app.handle_key_event(key_event(KeyCode::Esc, KeyModifiers::NONE), &mut clipboard)
            .unwrap();

        assert!(app.rename.is_none());
        assert_eq!(app.tabs.active_tab_title(), "Tab 1");
    }

    #[test]
    fn alt_shift_e_opens_rename_pane_modal() {
        let temp = tempdir().unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        app.handle_key_event(
            key_event(KeyCode::Char('E'), KeyModifiers::ALT | KeyModifiers::SHIFT),
            &mut clipboard,
        )
        .unwrap();

        assert_eq!(
            app.rename,
            Some(RenameState {
                target: RenameTarget::Pane(mtrm_core::PaneId::new(0)),
                input: "pane-0".to_owned(),
                cursor: 6,
            })
        );
    }

    #[test]
    fn alt_shift_russian_u_opens_rename_pane_modal() {
        let temp = tempdir().unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        app.handle_key_event(
            key_event(KeyCode::Char('У'), KeyModifiers::ALT | KeyModifiers::SHIFT),
            &mut clipboard,
        )
        .unwrap();

        assert!(matches!(
            app.rename,
            Some(RenameState {
                target: RenameTarget::Pane(_),
                ..
            })
        ));
    }

    #[test]
    #[serial]
    fn rename_pane_modal_applies_title_and_persists_it() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let mut app = App::new(shell_config(home.clone())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        with_test_home(&home, || {
            app.handle_key_event(
                key_event(KeyCode::Char('E'), KeyModifiers::ALT | KeyModifiers::SHIFT),
                &mut clipboard,
            )
        })
        .unwrap();
        for _ in 0..6 {
            with_test_home(&home, || {
                app.handle_key_event(
                    key_event(KeyCode::Backspace, KeyModifiers::NONE),
                    &mut clipboard,
                )
            })
            .unwrap();
        }
        for ch in ['e', 'd', 'i', 't', 'o', 'r'] {
            with_test_home(&home, || {
                app.handle_key_event(key_event(KeyCode::Char(ch), KeyModifiers::NONE), &mut clipboard)
            })
            .unwrap();
        }
        with_test_home(&home, || {
            app.handle_key_event(key_event(KeyCode::Enter, KeyModifiers::NONE), &mut clipboard)
        })
        .unwrap();

        assert_eq!(app.tabs.active_pane_title().unwrap(), "editor");

        let restored =
            with_test_home(&home, || App::restore_or_new(shell_config(home.clone()))).unwrap();
        assert_eq!(restored.tabs.active_pane_title().unwrap(), "editor");
    }

    #[test]
    fn rename_pane_modal_esc_cancels_changes() {
        let temp = tempdir().unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        app.open_rename_pane_modal();
        app.handle_key_event(key_event(KeyCode::Char('x'), KeyModifiers::NONE), &mut clipboard)
            .unwrap();
        app.handle_key_event(key_event(KeyCode::Esc, KeyModifiers::NONE), &mut clipboard)
            .unwrap();

        assert!(app.rename.is_none());
        assert_eq!(app.tabs.active_pane_title().unwrap(), "pane-0");
    }

    #[test]
    fn build_frame_view_uses_pane_title_from_snapshot_data() {
        let temp = tempdir().unwrap();
        let dir = temp.path().join("pane");
        fs::create_dir(&dir).unwrap();

        let snapshot = mtrm_session::SessionSnapshot {
            tabs: vec![mtrm_session::TabSnapshot {
                id: mtrm_core::TabId::new(1),
                title: "main".to_owned(),
                layout: mtrm_layout::LayoutTree::new(mtrm_core::PaneId::new(10)).to_snapshot(),
                panes: vec![mtrm_session::PaneSnapshot {
                    id: mtrm_core::PaneId::new(10),
                    cwd: dir,
                    title: "editor".to_owned(),
                }],
                active_pane: mtrm_core::PaneId::new(10),
            }],
            active_tab: mtrm_core::TabId::new(1),
        };

        let mut app = App {
            shell: shell_config(temp.path().to_path_buf()),
            keymap: Keymap::default(),
            tabs: mtrm_tabs::TabManager::from_snapshot(snapshot, &shell_config(temp.path().to_path_buf())).unwrap(),
            selection: None,
            should_quit: false,
            ui_dirty: true,
            window_focused: true,
            pending_alt_prefix_started_at: None,
            rename: None,
            clipboard_notice: None,
            last_content_area: DEFAULT_CONTENT_AREA,
        };

        let frame = app
            .build_frame_view(mtrm_layout::Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            })
            .unwrap();

        assert_eq!(frame.panes[0].title, "editor");
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
    fn paste_with_unavailable_clipboard_sets_notice_and_does_not_fail() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = UnavailableClipboard;

        with_test_home(&home, || {
            app.handle_key_event(
                key_event(KeyCode::Char('v'), KeyModifiers::CONTROL),
                &mut clipboard,
            )
        })
        .unwrap();

        let notice = app.clipboard_notice.as_ref().expect("clipboard notice");
        assert_eq!(notice.text, "Буфер обмена недоступен");
    }

    #[test]
    #[serial]
    fn ctrl_c_without_selection_does_not_copy_whole_pane() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        app.tabs
            .write_to_active_pane(b"printf 'copy me?\\n'\n")
            .unwrap();
        let loaded = wait_until(Duration::from_secs(2), || {
            app.refresh_all_panes_output().unwrap_or(false)
                && app
                    .tabs
                    .active_pane_text()
                    .map(|text| text.contains("copy me?"))
                    .unwrap_or(false)
        });
        assert!(loaded);

        with_test_home(&home, || {
            app.handle_key_event(
                key_event(KeyCode::Char('c'), KeyModifiers::CONTROL),
                &mut clipboard,
            )
        })
        .unwrap();

        assert_eq!(clipboard.get_text().unwrap(), "");
    }

    #[test]
    #[serial]
    fn ctrl_c_copies_only_selected_text_from_split_pane() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();
        let content_area = mtrm_layout::Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 23,
        };

        app.handle_layout_command(LayoutCommand::SplitFocused(
            mtrm_core::SplitDirection::Vertical,
        ))
        .unwrap();
        let right_pane = app.tabs.active_pane_id();
        app.tabs
            .write_to_active_pane(b"printf 'right pane text\\n'\n")
            .unwrap();
        app.handle_layout_command(LayoutCommand::MoveFocus(FocusMoveDirection::Left))
            .unwrap();
        app.tabs
            .write_to_active_pane(b"printf 'left pane text\\n'\n")
            .unwrap();

        let loaded = wait_until(Duration::from_secs(2), || {
            app.refresh_all_panes_output().unwrap_or(false)
                && app
                    .tabs
                    .pane_text(right_pane)
                    .map(|text| text.contains("right pane text"))
                    .unwrap_or(false)
                && app
                    .tabs
                    .active_pane_text()
                    .map(|text| text.contains("left pane text"))
                    .unwrap_or(false)
        });
        assert!(loaded);

        let right_area = app
            .tabs
            .placements(content_area)
            .unwrap()
            .into_iter()
            .find(|(pane_id, _, _)| *pane_id == right_pane)
            .map(|(_, area, _)| area)
            .unwrap();
        let right_content = pane_content_rect(right_area).unwrap();
        let (text_row, text_col) = find_visible_text_position(&app, right_pane, "right");

        app.handle_mouse_event(
            mouse_event(
                MouseEventKind::Down(MouseButton::Left),
                right_content.x.saturating_add(text_col),
                right_content.y.saturating_add(text_row),
            ),
            content_area,
        )
        .unwrap();
        app.handle_mouse_event(
            mouse_event(
                MouseEventKind::Drag(MouseButton::Left),
                right_content.x.saturating_add(text_col).saturating_add(4),
                right_content.y.saturating_add(text_row),
            ),
            content_area,
        )
        .unwrap();
        app.handle_mouse_event(
            mouse_event(
                MouseEventKind::Up(MouseButton::Left),
                right_content.x.saturating_add(text_col).saturating_add(4),
                right_content.y.saturating_add(text_row),
            ),
            content_area,
        )
        .unwrap();

        with_test_home(&home, || {
            app.handle_key_event(
                key_event(KeyCode::Char('c'), KeyModifiers::CONTROL),
                &mut clipboard,
            )
        })
        .unwrap();

        assert_eq!(clipboard.get_text().unwrap(), "right");
    }

    #[test]
    #[serial]
    fn mouse_click_switches_focus_to_clicked_pane() {
        let temp = tempdir().unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let content_area = mtrm_layout::Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 23,
        };

        app.handle_layout_command(LayoutCommand::SplitFocused(
            mtrm_core::SplitDirection::Vertical,
        ))
        .unwrap();
        let right_pane = app.tabs.active_pane_id();
        app.handle_layout_command(LayoutCommand::MoveFocus(FocusMoveDirection::Left))
            .unwrap();
        assert_ne!(app.tabs.active_pane_id(), right_pane);

        let right_area = app
            .tabs
            .placements(content_area)
            .unwrap()
            .into_iter()
            .find(|(pane_id, _, _)| *pane_id == right_pane)
            .map(|(_, area, _)| area)
            .unwrap();
        let right_content = pane_content_rect(right_area).unwrap();

        app.handle_mouse_event(
            mouse_event(
                MouseEventKind::Down(MouseButton::Left),
                right_content.x,
                right_content.y,
            ),
            content_area,
        )
        .unwrap();

        assert_eq!(app.tabs.active_pane_id(), right_pane);
    }

    #[test]
    fn tab_hit_testing_returns_clicked_tab_only_inside_title_span() {
        let tabs = vec![
            mtrm_tabs::RuntimeTabSummary {
                id: mtrm_core::TabId::new(0),
                title: "One".to_owned(),
                active: true,
            },
            mtrm_tabs::RuntimeTabSummary {
                id: mtrm_core::TabId::new(1),
                title: "Two".to_owned(),
                active: false,
            },
        ];

        assert_eq!(
            tab_id_at_position(&tabs, 80, 0, 0),
            Some(mtrm_core::TabId::new(0))
        );
        assert_eq!(
            tab_id_at_position(&tabs, 80, 2, 0),
            Some(mtrm_core::TabId::new(0))
        );
        assert_eq!(tab_id_at_position(&tabs, 80, 3, 0), None);
        assert_eq!(tab_id_at_position(&tabs, 80, 4, 0), None);
        assert_eq!(tab_id_at_position(&tabs, 80, 5, 0), None);
        assert_eq!(tab_id_at_position(&tabs, 80, 6, 0), Some(mtrm_core::TabId::new(1)));
        assert_eq!(tab_id_at_position(&tabs, 80, 0, 1), None);
    }

    #[test]
    #[serial]
    fn mouse_click_on_tab_bar_switches_active_tab() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let content_area = mtrm_layout::Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 23,
        };

        let first = app.tabs.active_tab_id();
        app.handle_tab_command(TabCommand::NewTab).unwrap();
        let second = app.tabs.active_tab_id();
        assert_ne!(first, second);

        with_test_home(&home, || {
            app.handle_mouse_event(
                mouse_event(MouseEventKind::Down(MouseButton::Left), 0, 0),
                content_area,
            )
        })
        .unwrap();

        assert_eq!(app.tabs.active_tab_id(), first);

        with_test_home(&home, || {
            let summaries = app.tabs.tab_summaries();
            let second_x = (0..content_area.width)
                .find_map(|column| {
                    tab_id_at_position(&summaries, content_area.width, column, 0)
                        .filter(|tab_id| *tab_id == second)
                        .map(|_| column)
                })
                .expect("expected to find clickable column for second tab");
            app.handle_mouse_event(
                mouse_event(MouseEventKind::Down(MouseButton::Left), second_x, 0),
                content_area,
            )
        })
        .unwrap();

        assert_eq!(app.tabs.active_tab_id(), second);
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
            !home.join(".mtrm").join("state.yaml").exists(),
            "plain PTY input must not trigger state save"
        );
    }

    #[test]
    fn handle_key_event_alt_x_sends_interrupt() {
        let temp = tempdir().unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        app.tabs.write_to_active_pane(b"sleep 5\n").unwrap();
        thread::sleep(Duration::from_millis(150));
        app.handle_key_event(
            key_event(KeyCode::Char('x'), KeyModifiers::ALT),
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
    fn handle_key_event_esc_prefix_russian_interrupt_sends_interrupt() {
        let temp = tempdir().unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        app.tabs.write_to_active_pane(b"sleep 5\n").unwrap();
        thread::sleep(Duration::from_millis(150));
        app.handle_key_event(key_event(KeyCode::Esc, KeyModifiers::NONE), &mut clipboard)
            .unwrap();
        app.handle_key_event(
            key_event(KeyCode::Char('ч'), KeyModifiers::NONE),
            &mut clipboard,
        )
        .unwrap();
        app.tabs
            .write_to_active_pane(b"printf '__ESC_PREFIX_INTERRUPT__\\n'\n")
            .unwrap();

        let ok = wait_until(Duration::from_secs(3), || {
            app.refresh_all_panes_output().is_ok()
                && app
                    .tabs
                    .active_pane_text()
                    .map(|text| text.contains("__ESC_PREFIX_INTERRUPT__"))
                    .unwrap_or(false)
        });
        assert!(ok);
    }

    #[test]
    #[serial]
    fn alt_x_preserves_interactive_backspace_and_arrow_editing_after_late_tty_corruption() {
        let temp = tempdir().unwrap();
        let shell = interactive_bash_config(temp.path().to_path_buf());
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
        assert!(
            initial_output,
            "interactive shell did not show initial output"
        );

        app.tabs
            .write_to_active_pane(
                b"sh -c 'trap \"(sleep 0.25; stty raw -echo </dev/tty >/dev/tty) & exit 130\" INT; while :; do sleep 1; done'\n",
            )
            .unwrap();
        thread::sleep(Duration::from_millis(200));
        app.handle_key_event(
            key_event(KeyCode::Char('x'), KeyModifiers::ALT),
            &mut clipboard,
        )
        .unwrap();

        let prompt_returned = wait_until(Duration::from_secs(3), || {
            app.refresh_all_panes_output().is_ok()
                && app
                    .tabs
                    .active_pane_text()
                    .map(|text| !text.trim().is_empty())
                    .unwrap_or(false)
        });
        assert!(
            prompt_returned,
            "shell did not return visible prompt after Alt+X"
        );

        // Give the delayed tty-corruption path time to either fire or get cleaned up before
        // we assess interactive editing on the recovered shell prompt.
        thread::sleep(Duration::from_millis(450));
        let _ = app.refresh_all_panes_output();

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
        app.handle_key_event(
            key_event(KeyCode::Char('h'), KeyModifiers::NONE),
            &mut clipboard,
        )
        .unwrap();
        app.handle_key_event(
            key_event(KeyCode::Char('o'), KeyModifiers::NONE),
            &mut clipboard,
        )
        .unwrap();
        app.handle_key_event(
            key_event(KeyCode::Char(' '), KeyModifiers::NONE),
            &mut clipboard,
        )
        .unwrap();
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
        app.handle_key_event(
            key_event(KeyCode::Backspace, KeyModifiers::NONE),
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

        let backspace_ok = wait_until(Duration::from_secs(3), || {
            app.refresh_all_panes_output().is_ok()
                && app
                    .tabs
                    .active_pane_text()
                    .map(|text| text.contains("abd") && !text.contains("^H"))
                    .unwrap_or(false)
        });
        assert!(
            backspace_ok,
            "backspace editing degraded after Alt+X and late tty corruption; pane text was {:?}",
            app.tabs.active_pane_text().ok()
        );

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
        app.handle_key_event(
            key_event(KeyCode::Char('h'), KeyModifiers::NONE),
            &mut clipboard,
        )
        .unwrap();
        app.handle_key_event(
            key_event(KeyCode::Char('o'), KeyModifiers::NONE),
            &mut clipboard,
        )
        .unwrap();
        app.handle_key_event(
            key_event(KeyCode::Char(' '), KeyModifiers::NONE),
            &mut clipboard,
        )
        .unwrap();
        app.handle_key_event(
            key_event(KeyCode::Char('a'), KeyModifiers::NONE),
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
        app.handle_key_event(key_event(KeyCode::Left, KeyModifiers::NONE), &mut clipboard)
            .unwrap();
        app.handle_key_event(
            key_event(KeyCode::Char('X'), KeyModifiers::NONE),
            &mut clipboard,
        )
        .unwrap();
        app.handle_key_event(
            key_event(KeyCode::Enter, KeyModifiers::NONE),
            &mut clipboard,
        )
        .unwrap();

        let ok = wait_until(Duration::from_secs(3), || {
            app.refresh_all_panes_output().is_ok()
                && app
                    .tabs
                    .active_pane_text()
                    .map(|text| text.contains("Xac") && !text.contains("^[[D"))
                    .unwrap_or(false)
        });
        assert!(
            ok,
            "left-arrow editing degraded after Alt+X and late tty corruption; pane text was {:?}",
            app.tabs.active_pane_text().ok()
        );
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
        assert!(!home.join(".mtrm").join("state.yaml").exists());
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
    fn shift_up_scrolls_fullscreen_history_through_app_commands() {
        let temp = tempdir().unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();

        app.tabs
            .write_to_active_pane(
                b"printf '\\033[?1049h\\033[2J\\033[Hframe1\\033[2J\\033[Hframe2\\033[2J\\033[Hframe3'\n",
            )
            .unwrap();

        let loaded = wait_until(Duration::from_secs(2), || {
            app.refresh_all_panes_output().unwrap_or(false)
                && app
                    .tabs
                    .active_pane_text()
                    .map(|text| text.contains("frame3"))
                    .unwrap_or(false)
        });
        assert!(loaded);

        app.handle_key_event(key_event(KeyCode::Up, KeyModifiers::SHIFT), &mut clipboard)
            .unwrap();
        let previous = app.tabs.active_pane_text().unwrap();
        assert!(previous.contains("frame2"));

        app.handle_key_event(key_event(KeyCode::End, KeyModifiers::NONE), &mut clipboard)
            .unwrap();
        let live = app.tabs.active_pane_text().unwrap();
        assert!(live.contains("frame3"));
    }

    #[test]
    fn build_frame_view_hides_cursor_for_scrolled_fullscreen_history() {
        let temp = tempdir().unwrap();
        let mut app = App::new(shell_config(temp.path().to_path_buf())).unwrap();
        let mut clipboard = MemoryClipboard::new();
        let content_area = mtrm_layout::Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 8,
        };

        app.tabs.resize_active_tab(content_area).unwrap();
        app.tabs
            .write_to_active_pane(
                b"printf '\\033[?1049h\\033[2J\\033[Hframe1\\033[2J\\033[Hframe2\\033[2J\\033[Hframe3'\n",
            )
            .unwrap();

        let loaded = wait_until(Duration::from_secs(2), || {
            app.refresh_all_panes_output().unwrap_or(false)
                && app
                    .tabs
                    .active_pane_text()
                    .map(|text| text.contains("frame3"))
                    .unwrap_or(false)
        });
        assert!(loaded);

        app.handle_key_event(key_event(KeyCode::Up, KeyModifiers::SHIFT), &mut clipboard)
            .unwrap();

        let frame = app.build_frame_view(content_area).unwrap();
        assert_eq!(frame.panes.len(), 1);
        let frame_text = frame.panes[0]
            .lines
            .iter()
            .map(|line| {
                line.cells
                    .iter()
                    .map(|cell| {
                        if cell.has_contents {
                            cell.text.clone()
                        } else {
                            " ".to_owned()
                        }
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(frame_text.contains("frame2"));
        assert_eq!(frame.panes[0].cursor, None);
    }

    #[test]
    fn plain_left_arrow_moves_shell_cursor_left() {
        let temp = tempdir().unwrap();
        let ok = {
            let shell = interactive_bash_config(temp.path().to_path_buf());
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
                false
            } else {
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
            }
        };
        assert!(ok, "left arrow must move shell cursor left before Enter");
    }

    #[test]
    #[serial]
    fn plain_home_moves_shell_cursor_to_start_of_line() {
        let temp = tempdir().unwrap();
        let ok = {
            let shell = interactive_bash_config(temp.path().to_path_buf());
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
                false
            } else {
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
            }
        };
        assert!(
            ok,
            "home must move shell cursor to the beginning of the line"
        );
    }

    #[test]
    #[serial]
    fn plain_end_moves_shell_cursor_to_end_of_line_when_not_scrolled() {
        let temp = tempdir().unwrap();
        let ok = {
            let shell = interactive_bash_config(temp.path().to_path_buf());
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
                false
            } else {
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
            }
        };
        assert!(
            ok,
            "end must move shell cursor to the end of the line when pane is not scrolled"
        );
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
            debug_log_path: None,
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

        assert!(home.join(".mtrm").join("state.yaml").is_file());
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
            AppError::State("failed to write /tmp/secret/state.yaml: permission denied".to_owned());

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
        assert!(home.join(".mtrm").join("state.yaml").is_file());
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
        let ok = with_env_var("SHELL", "bash", || {
            let mut shell = default_shell_config(None).unwrap();
            shell.initial_cwd = temp.path().to_path_buf();
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
        assert!(ok, "default shell startup must show visible shell output");
    }

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
}
