use std::io;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use mtrm_clipboard::{ClipboardBackend, ClipboardError, SystemClipboard, UnavailableClipboard};
use mtrm_platform_keys::{
    PlatformKeyProfile, current_platform_key_profile, key_bindings_for_profile,
};
use mtrm_process::ShellProcessConfig;
use ratatui::Terminal;
use ratatui::backend::Backend;

use crate::app::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CliAction {
    Run,
    PrintHelp,
    PrintVersion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliOptions {
    pub(crate) action: CliAction,
    pub(crate) debug_log_path: Option<PathBuf>,
    pub(crate) disable_clipboard: bool,
}

pub(crate) fn default_shell_config(
    debug_log_path: Option<PathBuf>,
) -> Result<ShellProcessConfig, io::Error> {
    let program = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned());
    let initial_cwd = std::env::current_dir()?;
    Ok(ShellProcessConfig {
        program: PathBuf::from(program),
        args: vec!["-i".to_owned()],
        initial_cwd,
        debug_log_path,
    })
}

pub(crate) fn parse_cli_args<I>(args: I) -> Result<CliOptions, AppError>
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

pub(crate) fn print_help() {
    println!("{}", help_text());
}

pub(crate) fn help_text() -> String {
    help_text_for_profile(current_platform_key_profile())
}

pub(crate) fn help_text_for_profile(profile: PlatformKeyProfile) -> String {
    let bindings = key_bindings_for_profile(profile);
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
"
    .to_owned()
        + &format!(
            "  {:<16} Send interrupt to active process
  {:<16} Split active pane left/right
  {:<16} Split active pane top/bottom
  {:<16} Close active pane
  {:<16} New tab
  {:<16} Previous tab
  {:<16} Next tab
  {:<16} Close current tab
  {:<16} Rename current tab
  {:<16} Rename current pane
  {:<16} Open help overlay
  {:<16} Resize pane left
  {:<16} Resize pane right
  {:<16} Resize pane up
  {:<16} Resize pane down
  {:<16} Save state and quit
  {:<16} Focus pane left
  {:<16} Focus pane right
  {:<16} Focus pane up
  {:<16} Focus pane down
  Shift+Up         Scroll pane history up
  Shift+Down       Scroll pane history down
  Shift+PageUp     Scroll pane history up by one page
  Shift+PageDown   Scroll pane history down by one page
  End              Return scrollback to live bottom

Notes:
  Letter-based command keys come from ~/.mtrm/keymap.toml.
  Arrow and scrollback bindings are built in.
",
            bindings.interrupt.label,
            bindings.split_vertical.label,
            bindings.split_horizontal.label,
            bindings.close_pane.label,
            bindings.new_tab.label,
            bindings.previous_tab.label,
            bindings.next_tab.label,
            bindings.close_tab.label,
            bindings.rename_tab.label,
            bindings.rename_pane.label,
            bindings.open_help.label,
            bindings.resize_left.label,
            bindings.resize_right.label,
            bindings.resize_up.label,
            bindings.resize_down.label,
            bindings.quit.label,
            bindings.focus_left.label,
            bindings.focus_right.label,
            bindings.focus_up.label,
            bindings.focus_down.label,
        )
}

pub(crate) fn cli_version_string() -> String {
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

pub(crate) fn tabs_error(error: impl ToString) -> AppError {
    AppError::Tabs(error.to_string())
}

pub(crate) fn state_error(error: impl ToString) -> AppError {
    AppError::State(error.to_string())
}

pub(crate) fn keymap_error(error: impl ToString) -> AppError {
    AppError::Config(error.to_string())
}

pub(crate) fn terminal_io_error(error: impl ToString) -> AppError {
    AppError::TerminalIo(error.to_string())
}

pub(crate) fn notice_for_clipboard_error(error: &ClipboardError) -> &'static str {
    match error {
        ClipboardError::Unavailable => "Clipboard is unavailable",
        ClipboardError::Read(_) => "Failed to read from clipboard",
        ClipboardError::Write(_) => "Failed to write to clipboard",
    }
}

pub(crate) fn build_clipboard(disable_clipboard: bool) -> Box<dyn ClipboardBackend> {
    if disable_clipboard {
        return Box::new(UnavailableClipboard);
    }

    match SystemClipboard::new() {
        Ok(clipboard) => Box::new(clipboard),
        Err(_) => Box::new(UnavailableClipboard),
    }
}

pub(crate) fn terminal_content_area<B: Backend>(
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
