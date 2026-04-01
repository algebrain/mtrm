//! PTY process wrapper built on top of `prux`.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use prux::{ProcessSession, ProcessSessionConfig};

const TERM_PROGRAM_NAME: &str = "mtrm";
const COLOR_TERM_HINT: &str = "truecolor";

#[derive(Debug, Clone)]
pub struct ShellProcessConfig {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub initial_cwd: PathBuf,
    pub debug_log_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum ProcessError {
    Spawn(String),
    Write(String),
    Read(String),
    Interrupt(String),
    CurrentDir(String),
}

impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(_) => f.write_str("failed to spawn shell"),
            Self::Write(_) => f.write_str("failed to write to pty"),
            Self::Read(_) => f.write_str("failed to read from pty"),
            Self::Interrupt(_) => f.write_str("failed to send interrupt"),
            Self::CurrentDir(_) => f.write_str("failed to resolve cwd"),
        }
    }
}

impl std::error::Error for ProcessError {}

pub struct ShellProcess {
    session: ProcessSession,
}

impl ShellProcess {
    pub fn spawn(config: ShellProcessConfig) -> Result<Self, ProcessError> {
        let session = ProcessSession::spawn(ProcessSessionConfig {
            program: config.program,
            args: config.args,
            initial_cwd: config.initial_cwd,
            debug_log_path: config.debug_log_path,
            env: BTreeMap::from([
                ("TERM_PROGRAM".to_string(), TERM_PROGRAM_NAME.to_string()),
                ("COLORTERM".to_string(), COLOR_TERM_HINT.to_string()),
            ]),
        })
        .map_err(map_spawn_error)?;

        Ok(Self { session })
    }

    pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), ProcessError> {
        self.session.write_all(bytes).map_err(map_write_error)
    }

    pub fn try_read(&mut self) -> Result<Vec<u8>, ProcessError> {
        self.session.try_read().map_err(map_read_error)
    }

    pub fn send_interrupt(&mut self) -> Result<(), ProcessError> {
        self.session.send_interrupt().map_err(map_interrupt_error)
    }

    pub fn current_dir(&self) -> Result<PathBuf, ProcessError> {
        self.session.current_dir().map_err(map_current_dir_error)
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), ProcessError> {
        self.session.resize(cols, rows).map_err(map_write_error)
    }

    pub fn is_alive(&mut self) -> Result<bool, ProcessError> {
        self.session.is_alive().map_err(map_read_error)
    }

    pub fn terminate(&mut self) -> Result<(), ProcessError> {
        self.session.terminate().map_err(map_interrupt_error)
    }
}

fn map_spawn_error(error: prux::ProcessError) -> ProcessError {
    ProcessError::Spawn(error.to_string())
}

fn map_write_error(error: prux::ProcessError) -> ProcessError {
    ProcessError::Write(error.to_string())
}

fn map_read_error(error: prux::ProcessError) -> ProcessError {
    ProcessError::Read(error.to_string())
}

fn map_interrupt_error(error: prux::ProcessError) -> ProcessError {
    ProcessError::Interrupt(error.to_string())
}

fn map_current_dir_error(error: prux::ProcessError) -> ProcessError {
    ProcessError::CurrentDir(error.to_string())
}
