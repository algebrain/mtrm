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
        let foreground_process_group_id = self.master.process_group_leader().map(|pid| pid as i32);
        let interrupted_process_group_id =
            foreground_process_group_id.filter(|pgid| *pgid != self.process_group_id);
        let interrupted_foreground_job = interrupted_process_group_id.is_some();
        let result = if foreground_process_group_id == Some(self.process_group_id)
            || foreground_process_group_id.is_none()
        {
            self.writer
                .write_all(&[0x03])
                .and_then(|_| self.writer.flush())
                .map_err(|error| ProcessError::Interrupt(error.to_string()))
        } else {
            let process_group_id = foreground_process_group_id.unwrap_or(self.process_group_id);
            signal::kill(Pid::from_raw(-process_group_id), Signal::SIGINT)
                .map_err(|error| ProcessError::Interrupt(error.to_string()))
        };
        thread::sleep(Duration::from_millis(20));
        if result.is_ok() {
            let _ = self.restore_baseline_termios_after_interrupt();
            if interrupted_foreground_job {
                let _ = self.restore_baseline_termios_via_shell_tty_after_interrupt();
                let _ = self.cleanup_lingering_tty_processes_after_interrupt(
                    interrupted_process_group_id.unwrap_or(self.process_group_id),
                );
            }
            self.refresh_baseline_termios_if_shell_foreground();
        }
        result
    }

    pub fn current_dir(&self) -> Result<PathBuf, ProcessError> {
        #[cfg(target_os = "linux")]
        {
            platform_linux::resolve_current_dir_via_procfs(self.process_id)
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(unsupported_current_dir_error())
        }
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
        #[cfg(target_os = "linux")]
        let descendants = platform_linux::descendant_pids(self.process_id as i32);

        #[cfg(not(target_os = "linux"))]
        let descendants: Vec<i32> = Vec::new();

        for pid in &descendants {
            let _ = signal::kill(Pid::from_raw(*pid), Signal::SIGHUP);
        }
        self.signal_process_group(Signal::SIGHUP)?;
        thread::sleep(Duration::from_millis(100));
        for pid in descendants.into_iter().rev() {
            let _ = signal::kill(Pid::from_raw(pid), Signal::SIGKILL);
        }
        let _ = self.signal_process_group(Signal::SIGKILL);
        let _ = self.child.kill();
        Ok(())
    }

    fn signal_process_group(&self, signal_kind: Signal) -> Result<(), ProcessError> {
        signal::kill(Pid::from_raw(-self.process_group_id), signal_kind)
            .map_err(|error| ProcessError::Interrupt(error.to_string()))
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

    fn restore_baseline_termios_via_shell_tty_after_interrupt(&self) -> Result<(), ProcessError> {
        for _ in 0..INTERRUPT_TERMIO_RECHECK_ATTEMPTS {
            thread::sleep(INTERRUPT_TERMIO_RECHECK_DELAY);

            let foreground_process_group_id =
                self.master.process_group_leader().map(|pid| pid as i32);
            if foreground_process_group_id != Some(self.process_group_id) {
                continue;
            }

            self.restore_baseline_termios_via_shell_tty()?;
            break;
        }

        Ok(())
    }

    fn restore_baseline_termios_via_shell_tty(&self) -> Result<(), ProcessError> {
        let Some(baseline_termios) = &self.baseline_termios else {
            return Ok(());
        };

        #[cfg(target_os = "linux")]
        {
            platform_linux::apply_termios_via_shell_tty(self.process_id, baseline_termios)
                .map_err(|error| ProcessError::Interrupt(error.to_string()))
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = baseline_termios;
            Ok(())
        }
    }

    fn cleanup_lingering_tty_processes_after_interrupt(
        &self,
        interrupted_process_group_id: i32,
    ) -> Result<(), ProcessError> {
        for _ in 0..INTERRUPT_TERMIO_RECHECK_ATTEMPTS {
            thread::sleep(INTERRUPT_TERMIO_RECHECK_DELAY);

            let foreground_process_group_id =
                self.master.process_group_leader().map(|pid| pid as i32);
            let shell_is_foreground = foreground_process_group_id == Some(self.process_group_id);
            #[cfg(target_os = "linux")]
            let descendants = platform_linux::descendant_pids(self.process_id as i32);

            #[cfg(not(target_os = "linux"))]
            let descendants: Vec<i32> = Vec::new();

            #[cfg(target_os = "linux")]
            let lingering = platform_linux::lingering_tty_processes_for_interrupted_group(
                self.process_id,
                self.process_group_id,
                interrupted_process_group_id,
                &descendants,
            );

            #[cfg(not(target_os = "linux"))]
            let lingering: Vec<()> = Vec::new();

            if !shell_is_foreground || lingering.is_empty() {
                continue;
            }

            signal::kill(Pid::from_raw(-interrupted_process_group_id), Signal::SIGHUP)
                .map_err(|error| ProcessError::Interrupt(error.to_string()))?;
            let _ = signal::kill(
                Pid::from_raw(-interrupted_process_group_id),
                Signal::SIGCONT,
            );
            thread::sleep(Duration::from_millis(50));

            #[cfg(target_os = "linux")]
            let descendants = platform_linux::descendant_pids(self.process_id as i32);

            #[cfg(not(target_os = "linux"))]
            let descendants: Vec<i32> = Vec::new();

            #[cfg(target_os = "linux")]
            let still_lingering = platform_linux::lingering_tty_processes_for_interrupted_group(
                self.process_id,
                self.process_group_id,
                interrupted_process_group_id,
                &descendants,
            );

            #[cfg(not(target_os = "linux"))]
            let still_lingering: Vec<()> = Vec::new();

            if still_lingering.is_empty() {
                return Ok(());
            }

            signal::kill(
                Pid::from_raw(-interrupted_process_group_id),
                Signal::SIGTERM,
            )
            .map_err(|error| ProcessError::Interrupt(error.to_string()))?;
            return Ok(());
        }

        Ok(())
    }
}

impl Drop for ShellProcess {
    fn drop(&mut self) {
        let _ = self.terminate_process_group();
    }
}

#[cfg_attr(target_os = "linux", allow(dead_code))]
fn unsupported_current_dir_error() -> ProcessError {
    ProcessError::CurrentDir("cwd resolution unsupported on this platform".to_owned())
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
mod tests {
    use super::*;
    use nix::libc::SIGINT;
    use tempfile::tempdir;
    #[cfg(target_os = "linux")]
    use crate::platform_linux::{descendant_pids, lingering_tty_processes_for_interrupted_group};

    fn interactive_bash_config(initial_cwd: PathBuf) -> ShellProcessConfig {
        ShellProcessConfig {
            program: PathBuf::from("/usr/bin/env"),
            args: vec![
                "-i".to_owned(),
                "TERM=xterm-256color".to_owned(),
                "PS1=__MTRM_PROMPT__ ".to_owned(),
                "bash".to_owned(),
                "--noprofile".to_owned(),
                "--norc".to_owned(),
                "-i".to_owned(),
            ],
            initial_cwd,
            debug_log_path: None,
        }
    }

    fn shell_config(initial_cwd: PathBuf) -> ShellProcessConfig {
        ShellProcessConfig {
            program: PathBuf::from("/bin/sh"),
            args: vec![],
            initial_cwd,
            debug_log_path: None,
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

    fn read_until_contains(
        process: &mut ShellProcess,
        needle: &str,
        timeout: Duration,
    ) -> Result<String, ProcessError> {
        let deadline = Instant::now() + timeout;
        let mut combined = String::new();

        while Instant::now() < deadline {
            let chunk = process.try_read()?;
            if !chunk.is_empty() {
                combined.push_str(&String::from_utf8_lossy(&chunk));
                if combined.contains(needle) {
                    return Ok(combined);
                }
            }
            thread::sleep(Duration::from_millis(20));
        }

        Err(ProcessError::Read(format!(
            "timed out waiting for output containing {needle:?}; got {combined:?}"
        )))
    }

    fn wait_for_prompt(process: &mut ShellProcess) -> Result<String, ProcessError> {
        read_until_contains(process, "__MTRM_PROMPT__", Duration::from_secs(3))
    }

    fn proc_signal_masks(pid: u32) -> (u64, u64) {
        let status = fs::read_to_string(format!("/proc/{pid}/status")).expect("read /proc status");
        let mut ignored = None;
        let mut caught = None;
        for line in status.lines() {
            if let Some(value) = line.strip_prefix("SigIgn:\t") {
                ignored = Some(u64::from_str_radix(value.trim(), 16).expect("parse SigIgn"));
            } else if let Some(value) = line.strip_prefix("SigCgt:\t") {
                caught = Some(u64::from_str_radix(value.trim(), 16).expect("parse SigCgt"));
            }
        }
        (
            ignored.expect("SigIgn present in /proc status"),
            caught.expect("SigCgt present in /proc status"),
        )
    }

    fn signal_bit(signo: i32) -> u64 {
        1_u64 << ((signo - 1) as u64)
    }

    fn control_char_to_stty_notation(byte: u8) -> String {
        match byte {
            0 => "undef".to_owned(),
            127 => "^?".to_owned(),
            value @ 1..=31 => format!("^{}", (value + 64) as char),
            value => (value as char).to_string(),
        }
    }

    #[test]
    fn spawn_creates_live_process() {
        let temp = tempdir().unwrap();
        let mut process = ShellProcess::spawn(shell_config(temp.path().to_path_buf())).unwrap();

        assert!(process.is_alive().unwrap());
    }

    #[test]
    fn write_all_and_try_read_exchange_data_with_shell() {
        let temp = tempdir().unwrap();
        let mut process = ShellProcess::spawn(shell_config(temp.path().to_path_buf())).unwrap();

        process.write_all(b"printf '__MTRM_OK__\\n'\n").unwrap();
        let output =
            read_until_contains(&mut process, "__MTRM_OK__", Duration::from_secs(2)).unwrap();

        assert!(output.contains("__MTRM_OK__"));
    }

    #[test]
    fn send_interrupt_stops_running_command() {
        let temp = tempdir().unwrap();
        let mut process = ShellProcess::spawn(shell_config(temp.path().to_path_buf())).unwrap();

        process.write_all(b"sleep 5\n").unwrap();
        thread::sleep(Duration::from_millis(150));
        process.send_interrupt().unwrap();
        process.write_all(b"printf '__INTERRUPTED__\\n'\n").unwrap();

        let output =
            read_until_contains(&mut process, "__INTERRUPTED__", Duration::from_secs(3)).unwrap();
        assert!(output.contains("__INTERRUPTED__"));
    }

    #[test]
    fn current_dir_tracks_shell_working_directory() {
        let temp = tempdir().unwrap();
        let initial_dir = temp.path().join("initial");
        let next_dir = temp.path().join("next");
        fs::create_dir(&initial_dir).unwrap();
        fs::create_dir(&next_dir).unwrap();
        let mut process = ShellProcess::spawn(shell_config(initial_dir.clone())).unwrap();

        assert_eq!(process.current_dir().unwrap(), initial_dir);

        process
            .write_all(format!("cd '{}'\n", next_dir.display()).as_bytes())
            .unwrap();

        let changed = wait_until(Duration::from_secs(2), || {
            process
                .current_dir()
                .map(|cwd| cwd == next_dir)
                .unwrap_or(false)
        });
        assert!(changed, "shell cwd did not change to {:?}", next_dir);
    }

    #[test]
    fn resize_accepts_valid_dimensions() {
        let temp = tempdir().unwrap();
        let mut process = ShellProcess::spawn(shell_config(temp.path().to_path_buf())).unwrap();

        process.resize(120, 40).unwrap();
        assert!(process.is_alive().unwrap());
    }

    #[test]
    fn terminate_stops_process_and_changes_alive_state() {
        let temp = tempdir().unwrap();
        let mut process = ShellProcess::spawn(shell_config(temp.path().to_path_buf())).unwrap();

        process.terminate().unwrap();

        let stopped = wait_until(Duration::from_secs(2), || !process.is_alive().unwrap());
        assert!(stopped, "process still alive after terminate");
    }

    #[test]
    fn dropping_shell_process_stops_background_work() {
        let temp = tempdir().unwrap();
        let marker = temp.path().join("marker.txt");
        let mut process = ShellProcess::spawn(shell_config(temp.path().to_path_buf())).unwrap();

        process
            .write_all(
                format!(
                    "sh -c 'sleep 1; touch \"{}\"' >/dev/null 2>&1 &\n",
                    marker.display()
                )
                .as_bytes(),
            )
            .unwrap();
        thread::sleep(Duration::from_millis(150));

        drop(process);
        thread::sleep(Duration::from_secs(2));

        assert!(
            !marker.exists(),
            "background child survived ShellProcess drop and kept running"
        );
    }

    #[test]
    fn read_buffer_is_bounded() {
        let temp = tempdir().unwrap();
        let mut process = ShellProcess::spawn(shell_config(temp.path().to_path_buf())).unwrap();

        process.write_all(b"yes X\n").unwrap();
        thread::sleep(Duration::from_millis(300));

        let buffered_len = process
            .read_buffer
            .lock()
            .expect("read_buffer poisoned")
            .len();
        process.send_interrupt().unwrap();

        assert_eq!(
            buffered_len, MAX_READ_BUFFER_BYTES,
            "read buffer must be capped at {} bytes, got {}",
            MAX_READ_BUFFER_BYTES, buffered_len
        );
    }

    #[test]
    fn process_error_display_is_sanitized_but_debug_keeps_detail() {
        let error = ProcessError::CurrentDir("/proc/123/cwd: permission denied".to_owned());

        let display = error.to_string();
        let debug = format!("{error:?}");

        assert!(!display.contains("/proc/123/cwd"));
        assert!(!display.contains("permission denied"));
        assert!(debug.contains("/proc/123/cwd"));
    }

    #[test]
    fn unsupported_cwd_strategy_returns_current_dir_error() {
        match unsupported_current_dir_error() {
            ProcessError::CurrentDir(detail) => {
                assert!(detail.contains("unsupported"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn interactive_bash_emits_prompt_into_pty() {
        let temp = tempdir().unwrap();
        let config = interactive_bash_config(temp.path().to_path_buf());
        let mut process = ShellProcess::spawn(config).unwrap();

        let output = wait_for_prompt(&mut process).unwrap();

        assert!(output.contains("__MTRM_PROMPT__"));
    }

    #[test]
    fn termios_restore_is_needed_when_canonical_echo_and_signal_flags_are_missing() {
        let temp = tempdir().unwrap();
        let process = ShellProcess::spawn(shell_config(temp.path().to_path_buf())).unwrap();
        let baseline = process
            .baseline_termios
            .clone()
            .expect("pty baseline termios must be available");
        let mut raw_like = baseline.clone();
        raw_like
            .local_flags
            .remove(LocalFlags::ICANON | LocalFlags::ECHO | LocalFlags::ISIG);

        assert!(termios_needs_restore(&raw_like, &baseline));
        assert!(!termios_needs_restore(&baseline, &baseline));
    }

    #[test]
    fn termios_restore_is_needed_when_sane_input_output_and_echo_flags_drift() {
        let temp = tempdir().unwrap();
        let process = ShellProcess::spawn(shell_config(temp.path().to_path_buf())).unwrap();
        let baseline = process
            .baseline_termios
            .clone()
            .expect("pty baseline termios must be available");
        let mut drifted = baseline.clone();
        drifted
            .input_flags
            .remove(InputFlags::ICRNL | InputFlags::IXON);
        drifted
            .output_flags
            .remove(OutputFlags::OPOST | OutputFlags::ONLCR);
        drifted
            .local_flags
            .remove(LocalFlags::IEXTEN | LocalFlags::ECHOE | LocalFlags::ECHOK);

        assert!(termios_needs_restore(&drifted, &baseline));
    }

    #[test]
    fn termios_restore_is_needed_when_control_characters_drift() {
        let temp = tempdir().unwrap();
        let process = ShellProcess::spawn(shell_config(temp.path().to_path_buf())).unwrap();
        let baseline = process
            .baseline_termios
            .clone()
            .expect("pty baseline termios must be available");
        let mut drifted = baseline.clone();
        drifted.control_chars[SpecialCharacterIndices::VINTR as usize] = 0;
        drifted.control_chars[SpecialCharacterIndices::VERASE as usize] = 8;

        assert!(termios_needs_restore(&drifted, &baseline));
    }

    #[test]
    fn restore_baseline_termios_recovers_shell_from_raw_like_mode() {
        let temp = tempdir().unwrap();
        let mut process = ShellProcess::spawn(shell_config(temp.path().to_path_buf())).unwrap();
        let baseline = process
            .baseline_termios
            .clone()
            .expect("pty baseline termios must be available");

        process.write_all(b"stty raw -echo\n").unwrap();
        let raw_mode_set = wait_until(Duration::from_secs(2), || {
            process
                .master
                .get_termios()
                .map(|termios| termios_needs_restore(&termios, &baseline))
                .unwrap_or(false)
        });
        assert!(raw_mode_set, "shell did not switch pty into raw-like mode");

        process.restore_baseline_termios_if_needed().unwrap();
        let restored = process.master.get_termios().expect("current termios");

        assert!(restored.local_flags.contains(LocalFlags::ICANON));
        assert!(restored.local_flags.contains(LocalFlags::ECHO));
        assert!(restored.local_flags.contains(LocalFlags::ISIG));
    }

    #[test]
    fn send_interrupt_reaches_foreground_job_in_interactive_bash() {
        let temp = tempdir().unwrap();
        let config = interactive_bash_config(temp.path().to_path_buf());
        let mut process = ShellProcess::spawn(config).unwrap();

        let _ = read_until_contains(&mut process, "__MTRM_PROMPT__", Duration::from_secs(5))
            .unwrap();
        process.write_all(b"sleep 5\n").unwrap();
        thread::sleep(Duration::from_millis(200));
        process.send_interrupt().unwrap();
        process
            .write_all(b"printf '__INTERRUPTED_BASH__\\n'\n")
            .unwrap();

        let output =
            read_until_contains(&mut process, "__INTERRUPTED_BASH__", Duration::from_secs(3))
                .unwrap();
        assert!(output.contains("__INTERRUPTED_BASH__"));
    }

    #[test]
    #[ignore = "flaky on CI; foreground raw tty recovery timing is unstable"]
    fn send_interrupt_restores_shell_after_foreground_job_leaves_tty_raw() {
        let temp = tempdir().unwrap();
        let config = interactive_bash_config(temp.path().to_path_buf());
        let mut process = ShellProcess::spawn(config).unwrap();
        let _ = read_until_contains(&mut process, "__MTRM_PROMPT__", Duration::from_secs(5))
            .unwrap();
        process
            .write_all(b"sh -c 'stty raw -echo; sleep 5'\n")
            .unwrap();
        process.send_interrupt().unwrap();

        let _ = wait_for_prompt(&mut process).unwrap();

        process
            .write_all(b"printf '__RAW_INTERRUPT_RECOVERED__\\n'\n")
            .unwrap();
        let output = read_until_contains(
            &mut process,
            "__RAW_INTERRUPT_RECOVERED__",
            Duration::from_secs(3),
        )
        .unwrap();
        assert!(output.contains("__RAW_INTERRUPT_RECOVERED__"));
    }

    #[test]
    fn send_interrupt_restores_shell_when_tty_turns_raw_after_delayed_interrupt_trap() {
        let temp = tempdir().unwrap();
        let config = interactive_bash_config(temp.path().to_path_buf());
        let mut process = ShellProcess::spawn(config).unwrap();
        let _ = wait_for_prompt(&mut process).unwrap();
        process
            .write_all(
                b"sh -c 'trap \"sleep 0.05; stty raw -echo; exit 130\" INT; while :; do sleep 1; done'\n",
            )
            .unwrap();
        thread::sleep(Duration::from_millis(200));

        process.send_interrupt().unwrap();
        let _ = wait_for_prompt(&mut process).unwrap();

        process
            .write_all(b"printf '__DELAYED_RAW_INTERRUPT_RECOVERED__\\n'\n")
            .unwrap();
        let output = read_until_contains(
            &mut process,
            "__DELAYED_RAW_INTERRUPT_RECOVERED__",
            Duration::from_secs(3),
        )
        .unwrap();
        assert!(output.contains("__DELAYED_RAW_INTERRUPT_RECOVERED__"));
    }

    #[test]
    fn send_interrupt_restores_shell_when_tty_turns_raw_after_shell_regains_foreground() {
        let temp = tempdir().unwrap();
        let config = interactive_bash_config(temp.path().to_path_buf());
        let mut process = ShellProcess::spawn(config).unwrap();
        let _ = wait_for_prompt(&mut process).unwrap();
        process
            .write_all(
                b"sh -c 'trap \"(sleep 0.25; stty raw -echo </dev/tty >/dev/tty) & exit 130\" INT; while :; do sleep 1; done'\n",
            )
            .unwrap();
        thread::sleep(Duration::from_millis(200));

        process.send_interrupt().unwrap();
        let _ = wait_for_prompt(&mut process).unwrap();

        let _ = process.try_read().unwrap();
        process.write_all(b"echo abc\x7fd\n").unwrap();
        let output =
            read_until_contains(&mut process, "__MTRM_PROMPT__", Duration::from_secs(3)).unwrap();
        assert!(
            output.contains("abd"),
            "expected shell line editing to remain healthy after late tty corruption; got {output:?}"
        );
        assert!(
            !output.contains("^H"),
            "shell echoed raw backspace after late tty corruption; got {output:?}"
        );

        let _ = process.try_read().unwrap();
        process.write_all(b"echo ac\x1b[D\x1b[Db\n").unwrap();
        let output =
            read_until_contains(&mut process, "__MTRM_PROMPT__", Duration::from_secs(3)).unwrap();
        assert!(
            output.contains("bac"),
            "expected left-arrow line editing to remain healthy after late tty corruption; got {output:?}"
        );
        assert!(
            !output.contains("^[[D"),
            "shell echoed raw left-arrow escape after late tty corruption; got {output:?}"
        );
    }

    #[test]
    fn send_interrupt_cleans_up_orphaned_same_tty_processes_from_interrupted_group() {
        let temp = tempdir().unwrap();
        let config = interactive_bash_config(temp.path().to_path_buf());
        let mut process = ShellProcess::spawn(config).unwrap();
        let _ = wait_for_prompt(&mut process).unwrap();
        process
            .write_all(
                b"sh -c 'trap \"exit 130\" INT; (sh -c \"trap \\\"\\\" HUP INT TERM; while :; do sleep 1; done\") & while :; do sleep 1; done'\n",
            )
            .unwrap();
        thread::sleep(Duration::from_millis(200));

        process.send_interrupt().unwrap();
        let _ = wait_for_prompt(&mut process).unwrap();

        let cleaned_up = wait_until(Duration::from_secs(2), || {
            let descendants = descendant_pids(process.process_id as i32);
            lingering_tty_processes_for_interrupted_group(
                process.process_id,
                process.process_group_id,
                process.process_group_id + 1,
                &descendants,
            )
            .is_empty()
        });

        process.write_all(b"echo ac\x1b[D\x1b[Db\n").unwrap();
        let output =
            read_until_contains(&mut process, "__MTRM_PROMPT__", Duration::from_secs(3)).unwrap();

        assert!(
            cleaned_up,
            "expected lingering same-tty processes from interrupted job to disappear"
        );
        assert!(
            output.contains("bac"),
            "expected shell editing to remain healthy; got {output:?}"
        );
        assert!(
            !output.contains("^[[D"),
            "shell echoed raw left-arrow after orphan cleanup scenario; got {output:?}"
        );
    }

    #[test]
    fn restore_baseline_termios_preserves_interactive_backspace_behavior() {
        let temp = tempdir().unwrap();
        let config = interactive_bash_config(temp.path().to_path_buf());
        let mut process = ShellProcess::spawn(config).unwrap();

        let _ = wait_for_prompt(&mut process).unwrap();
        process.write_all(b"stty raw -echo\n").unwrap();
        let _ = read_until_contains(&mut process, "raw", Duration::from_secs(2)).ok();
        process.restore_baseline_termios_if_needed().unwrap();

        let _ = process.try_read().unwrap();
        process.write_all(b"echo abc\x7fd\n").unwrap();

        let output =
            read_until_contains(&mut process, "__MTRM_PROMPT__", Duration::from_secs(3)).unwrap();
        assert!(
            output.contains("abd"),
            "expected interactive backspace to erase the previous character; got {output:?}"
        );
        assert!(
            !output.contains("^H"),
            "shell echoed raw backspace instead of line editing; got {output:?}"
        );
    }

    #[test]
    fn interactive_bash_baseline_tracks_shell_prompt_state() {
        let temp = tempdir().unwrap();
        let config = interactive_bash_config(temp.path().to_path_buf());
        let mut process = ShellProcess::spawn(config).unwrap();

        let _ = wait_for_prompt(&mut process).unwrap();
        let baseline_after_prompt = process
            .baseline_termios
            .clone()
            .expect("shell prompt baseline termios must be available");
        let current = process.master.get_termios().expect("current termios");
        assert_eq!(baseline_after_prompt, current);
    }

    #[test]
    fn interactive_bash_does_not_ignore_interrupt_signal_after_spawn() {
        let temp = tempdir().unwrap();
        let config = interactive_bash_config(temp.path().to_path_buf());
        let mut process = ShellProcess::spawn(config).unwrap();

        let _ = wait_for_prompt(&mut process).unwrap();
        let (ignored_mask, _caught_mask) = proc_signal_masks(process.process_id);

        assert_eq!(
            ignored_mask & signal_bit(SIGINT),
            0,
            "SIGINT is unexpectedly ignored by the shell after spawn"
        );
    }

    #[test]
    fn interactive_bash_prompt_time_control_chars_match_shell_termios() {
        let temp = tempdir().unwrap();
        let config = interactive_bash_config(temp.path().to_path_buf());
        let mut process = ShellProcess::spawn(config).unwrap();

        let _ = wait_for_prompt(&mut process).unwrap();
        let baseline = process
            .baseline_termios
            .clone()
            .expect("shell prompt baseline termios must be available");
        let expected_intr = control_char_to_stty_notation(
            baseline.control_chars[SpecialCharacterIndices::VINTR as usize],
        );
        let expected_erase = control_char_to_stty_notation(
            baseline.control_chars[SpecialCharacterIndices::VERASE as usize],
        );

        process
            .write_all(b"stty -a; printf '__TTY_CHARS_DONE__\\n'\n")
            .unwrap();
        let output =
            read_until_contains(&mut process, "__TTY_CHARS_DONE__", Duration::from_secs(3)).unwrap();

        assert!(
            output.contains(&format!("intr = {expected_intr};")),
            "shell reported unexpected intr char; expected {expected_intr:?}, got {output:?}"
        );
        assert!(
            output.contains(&format!("erase = {expected_erase};")),
            "shell reported unexpected erase char; expected {expected_erase:?}, got {output:?}"
        );
    }
}
