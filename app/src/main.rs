use std::io;

use crossterm::ExecutableCommand;
use crossterm::event::{
    DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

mod app;
mod cli;
mod commands;
mod events;
mod rename;
mod render;
mod selection;

use app::{App, AppError};
use cli::{build_clipboard, cli_version_string, default_shell_config, parse_cli_args, print_help};

fn main() -> Result<(), AppError> {
    let cli = parse_cli_args(std::env::args())?;

    match cli.action {
        cli::CliAction::Run => {}
        cli::CliAction::PrintHelp => {
            print_help();
            return Ok(());
        }
        cli::CliAction::PrintVersion => {
            println!("{}", cli_version_string());
            return Ok(());
        }
    }

    let shell = default_shell_config(cli.debug_log_path)
        .map_err(|error| AppError::Config(error.to_string()))?;

    enable_raw_mode().map_err(cli::terminal_io_error)?;
    let mut stdout = io::stdout();
    stdout
        .execute(EnterAlternateScreen)
        .map_err(cli::terminal_io_error)?;
    stdout
        .execute(EnableFocusChange)
        .map_err(cli::terminal_io_error)?;
    stdout
        .execute(EnableMouseCapture)
        .map_err(cli::terminal_io_error)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(cli::terminal_io_error)?;
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

#[cfg(test)]
mod tests;
