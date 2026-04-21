//! Псевдотерминалы, дочерние процессы и определение рабочего каталога.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::BorrowedFd;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(test)]
use std::time::Instant;

use nix::sys::signal::{self, Signal};
use nix::sys::termios::{
    ControlFlags, InputFlags, LocalFlags, OutputFlags, SetArg, SpecialCharacterIndices, Termios,
    tcsetattr,
};
use nix::unistd::{Pid, getpgid};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use thiserror::Error;

#[cfg(target_os = "linux")]
mod platform_linux;
#[cfg(target_os = "macos")]
mod platform_macos;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod platform_other;

#[cfg(target_os = "linux")]
use self::platform_linux as platform;
#[cfg(target_os = "macos")]
use self::platform_macos as platform;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
use self::platform_other as platform;

const MAX_READ_BUFFER_BYTES: usize = 65_536;
const TERM_PROGRAM_NAME: &str = "mtrm";
const COLOR_TERM_HINT: &str = "truecolor";
const INTERRUPT_TERMIO_RECHECK_DELAY: Duration = Duration::from_millis(25);
const INTERRUPT_TERMIO_RECHECK_ATTEMPTS: usize = 6;

#[derive(Debug, Clone)]
pub struct ShellProcessConfig {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub initial_cwd: PathBuf,
    pub debug_log_path: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("failed to spawn shell")]
    Spawn(String),
    #[error("failed to write to pty")]
    Write(String),
    #[error("failed to read from pty")]
    Read(String),
    #[error("failed to send interrupt")]
    Interrupt(String),
    #[error("failed to resolve cwd")]
    CurrentDir(String),
}

pub struct ShellProcess {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    read_buffer: Arc<Mutex<VecDeque<u8>>>,
    read_error: Arc<Mutex<Option<String>>>,
    process_id: u32,
    process_group_id: i32,
    baseline_termios: Option<Termios>,
}

#[derive(Debug, Clone, Copy)]
struct InterruptContext {
    foreground_process_group_id: Option<i32>,
    interrupted_process_group_id: Option<i32>,
}

impl ShellProcess {
    pub fn spawn(config: ShellProcessConfig) -> Result<Self, ProcessError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| ProcessError::Spawn(error.to_string()))?;

        let mut command = CommandBuilder::new(&config.program);
        for arg in &config.args {
            command.arg(arg);
        }
        command.cwd(&config.initial_cwd);
        command.env("TERM_PROGRAM", TERM_PROGRAM_NAME);
        command.env("COLORTERM", COLOR_TERM_HINT);

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| ProcessError::Spawn(error.to_string()))?;
        let process_id = child
            .process_id()
            .ok_or_else(|| ProcessError::Spawn("spawned process has no pid".to_owned()))?;
        let process_group_id = getpgid(Some(Pid::from_raw(process_id as i32)))
            .map_err(|error| ProcessError::Spawn(error.to_string()))?
            .as_raw();

        let writer = pair
            .master
            .take_writer()
            .map_err(|error| ProcessError::Spawn(error.to_string()))?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| ProcessError::Spawn(error.to_string()))?;
        let baseline_termios = pair.master.get_termios();

        let read_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let read_error = Arc::new(Mutex::new(None));
        let debug_log = config
            .debug_log_path
            .as_ref()
            .map(open_debug_log_file)
            .transpose()
            .map_err(|error| ProcessError::Spawn(error.to_string()))?;
        spawn_reader_thread(
            reader,
            Arc::clone(&read_buffer),
            Arc::clone(&read_error),
            debug_log,
            process_id,
        );

        Ok(Self {
            master: pair.master,
            child,
            writer,
            read_buffer,
            read_error,
            process_id,
            process_group_id,
            baseline_termios,
        })
    }

    pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), ProcessError> {
        let result = self
            .writer
            .write_all(bytes)
            .and_then(|_| self.writer.flush())
            .map_err(|error| ProcessError::Write(error.to_string()));
        if result.is_ok() {
            self.refresh_baseline_termios_if_shell_foreground();
        }
        result
    }

    pub fn try_read(&mut self) -> Result<Vec<u8>, ProcessError> {
        if let Some(error) = self.read_error.lock().expect("read_error poisoned").take() {
            return Err(ProcessError::Read(error));
        }

        let mut buffer = self.read_buffer.lock().expect("read_buffer poisoned");
        let bytes: Vec<u8> = buffer.drain(..).collect();
        drop(buffer);
        if !bytes.is_empty() {
            self.refresh_baseline_termios_if_shell_foreground();
        }
        Ok(bytes)
    }

    pub fn send_interrupt(&mut self) -> Result<(), ProcessError> {
        let context = self.interrupt_context();
        let result = self.deliver_interrupt(context);
        thread::sleep(Duration::from_millis(20));
        if result.is_ok() {
            self.finish_interrupt_recovery(context);
        }
        result
    }

    pub fn current_dir(&self) -> Result<PathBuf, ProcessError> {
        platform::resolve_current_dir(self.process_id)
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), ProcessError> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| ProcessError::Write(error.to_string()))
    }

    pub fn is_alive(&mut self) -> Result<bool, ProcessError> {
        self.child
            .try_wait()
            .map(|status| status.is_none())
            .map_err(|error| ProcessError::Read(error.to_string()))
    }

    pub fn terminate(&mut self) -> Result<(), ProcessError> {
        self.terminate_process_group()
    }

    fn terminate_process_group(&mut self) -> Result<(), ProcessError> {
        platform::terminate_process_tree(self.process_id, self.process_group_id)?;
        let _ = self.child.kill();
        Ok(())
    }

    fn restore_baseline_termios_if_needed(&self) -> Result<(), ProcessError> {
        let Some(baseline_termios) = &self.baseline_termios else {
            return Ok(());
        };
        let Some(current_termios) = self.master.get_termios() else {
            return Ok(());
        };

        if !termios_needs_restore(&current_termios, baseline_termios) {
            return Ok(());
        }

        apply_termios_to_master(&*self.master, baseline_termios)
            .map_err(|error| ProcessError::Interrupt(error.to_string()))?;
        Ok(())
    }

    fn interrupt_context(&self) -> InterruptContext {
        let foreground_process_group_id = self.master.process_group_leader().map(|pid| pid as i32);
        let interrupted_process_group_id =
            foreground_process_group_id.filter(|pgid| *pgid != self.process_group_id);
        InterruptContext {
            foreground_process_group_id,
            interrupted_process_group_id,
        }
    }

    fn deliver_interrupt(&mut self, context: InterruptContext) -> Result<(), ProcessError> {
        if context.foreground_process_group_id == Some(self.process_group_id)
            || context.foreground_process_group_id.is_none()
        {
            self.writer
                .write_all(&[0x03])
                .and_then(|_| self.writer.flush())
                .map_err(|error| ProcessError::Interrupt(error.to_string()))
        } else {
            let process_group_id = context
                .foreground_process_group_id
                .unwrap_or(self.process_group_id);
            signal::kill(Pid::from_raw(-process_group_id), Signal::SIGINT)
                .map_err(|error| ProcessError::Interrupt(error.to_string()))
        }
    }

    fn finish_interrupt_recovery(&mut self, context: InterruptContext) {
        let _ = self.restore_baseline_termios_after_interrupt();
        if let Some(interrupted_process_group_id) = context.interrupted_process_group_id {
            let _ = self.platform_post_interrupt_recovery(interrupted_process_group_id);
        }
        self.refresh_baseline_termios_if_shell_foreground();
    }

    fn restore_baseline_termios_after_interrupt(&self) -> Result<(), ProcessError> {
        self.restore_baseline_termios_if_needed()?;

        for _ in 0..INTERRUPT_TERMIO_RECHECK_ATTEMPTS {
            thread::sleep(INTERRUPT_TERMIO_RECHECK_DELAY);

            let foreground_process_group_id =
                self.master.process_group_leader().map(|pid| pid as i32);
            let shell_is_foreground = foreground_process_group_id == Some(self.process_group_id);
            let termios_needs_attention = self
                .master
                .get_termios()
                .zip(self.baseline_termios.as_ref())
                .map(|(current, baseline)| termios_needs_restore(&current, baseline))
                .unwrap_or(false);

            if !shell_is_foreground && !termios_needs_attention {
                continue;
            }

            self.restore_baseline_termios_if_needed()?;

            if shell_is_foreground {
                break;
            }
        }

        Ok(())
    }

    fn refresh_baseline_termios_if_shell_foreground(&mut self) {
        let foreground_process_group_id = self.master.process_group_leader().map(|pid| pid as i32);
        if foreground_process_group_id != Some(self.process_group_id) {
            return;
        }

        let Some(current_termios) = self.master.get_termios() else {
            return;
        };

        self.baseline_termios = Some(current_termios);
    }

    fn platform_post_interrupt_recovery(
        &self,
        interrupted_process_group_id: i32,
    ) -> Result<(), ProcessError> {
        platform::post_interrupt_recovery(
            &*self.master,
            self.process_id,
            self.process_group_id,
            interrupted_process_group_id,
            self.baseline_termios.as_ref(),
            INTERRUPT_TERMIO_RECHECK_ATTEMPTS,
            INTERRUPT_TERMIO_RECHECK_DELAY,
        )
    }
}

impl Drop for ShellProcess {
    fn drop(&mut self) {
        let _ = self.terminate_process_group();
    }
}

fn spawn_reader_thread(
    mut reader: Box<dyn Read + Send>,
    read_buffer: Arc<Mutex<VecDeque<u8>>>,
    read_error: Arc<Mutex<Option<String>>>,
    debug_log: Option<Arc<Mutex<File>>>,
    process_id: u32,
) {
    thread::spawn(move || {
        let mut buf = [0_u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(count) => {
                    if let Some(debug_log) = &debug_log {
                        let _ = write_debug_log(debug_log, process_id, &buf[..count]);
                    }
                    let mut target = read_buffer.lock().expect("read_buffer poisoned");
                    target.extend(&buf[..count]);
                    truncate_read_buffer(&mut target, MAX_READ_BUFFER_BYTES);
                }
                Err(error) => {
                    *read_error.lock().expect("read_error poisoned") = Some(error.to_string());
                    break;
                }
            }
        }
    });
}

fn open_debug_log_file(path: &PathBuf) -> Result<Arc<Mutex<File>>, std::io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = OpenOptions::new().create(true).append(true).open(path)?;
    Ok(Arc::new(Mutex::new(file)))
}

fn write_debug_log(
    debug_log: &Arc<Mutex<File>>,
    process_id: u32,
    bytes: &[u8],
) -> Result<(), std::io::Error> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);

    let escaped = bytes
        .iter()
        .flat_map(|byte| std::ascii::escape_default(*byte))
        .map(char::from)
        .collect::<String>();

    let mut file = debug_log.lock().expect("debug_log poisoned");
    writeln!(
        file,
        "[{timestamp}] pid={process_id} bytes={} escaped={escaped}",
        bytes.len()
    )?;
    file.flush()
}

fn truncate_read_buffer(buffer: &mut VecDeque<u8>, max_bytes: usize) {
    while buffer.len() > max_bytes {
        let _ = buffer.pop_front();
    }
}

fn termios_needs_restore(current: &Termios, baseline: &Termios) -> bool {
    let important_local_flags = LocalFlags::ISIG
        | LocalFlags::ICANON
        | LocalFlags::IEXTEN
        | LocalFlags::ECHO
        | LocalFlags::ECHOE
        | LocalFlags::ECHOK
        | LocalFlags::ECHOCTL
        | LocalFlags::ECHOKE;
    let important_input_flags =
        InputFlags::BRKINT | InputFlags::ICRNL | InputFlags::IXON | InputFlags::IMAXBEL;
    let important_output_flags = OutputFlags::OPOST | OutputFlags::ONLCR;
    let important_control_flags = ControlFlags::CREAD;

    let local_mismatch =
        baseline.local_flags & important_local_flags != current.local_flags & important_local_flags;
    let input_mismatch =
        baseline.input_flags & important_input_flags != current.input_flags & important_input_flags;
    let output_mismatch = baseline.output_flags & important_output_flags
        != current.output_flags & important_output_flags;
    let control_mismatch = baseline.control_flags & important_control_flags
        != current.control_flags & important_control_flags;

    let special_chars_to_match = [
        SpecialCharacterIndices::VINTR,
        SpecialCharacterIndices::VQUIT,
        SpecialCharacterIndices::VERASE,
        SpecialCharacterIndices::VKILL,
        SpecialCharacterIndices::VEOF,
        SpecialCharacterIndices::VSTART,
        SpecialCharacterIndices::VSTOP,
        SpecialCharacterIndices::VSUSP,
        SpecialCharacterIndices::VREPRINT,
        SpecialCharacterIndices::VWERASE,
        SpecialCharacterIndices::VLNEXT,
        SpecialCharacterIndices::VDISCARD,
        SpecialCharacterIndices::VMIN,
        SpecialCharacterIndices::VTIME,
    ];
    let control_char_mismatch = special_chars_to_match.iter().any(|index| {
        baseline.control_chars[*index as usize] != current.control_chars[*index as usize]
    });

    local_mismatch || input_mismatch || output_mismatch || control_mismatch || control_char_mismatch
}

fn apply_termios_to_master(master: &dyn MasterPty, termios: &Termios) -> Result<(), nix::Error> {
    let Some(raw_fd) = master.as_raw_fd() else {
        return Ok(());
    };
    // SAFETY: raw_fd comes from the live PTY master owned by `master` and is used only
    // for the duration of this call without taking ownership.
    let borrowed_fd = unsafe { BorrowedFd::borrow_raw(raw_fd) };
    tcsetattr(borrowed_fd, SetArg::TCSANOW, termios)
}

#[cfg(test)]
mod tests;
