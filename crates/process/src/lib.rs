//! Псевдотерминалы, дочерние процессы и определение рабочего каталога.

use std::collections::VecDeque;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[cfg(test)]
use std::time::Instant;

use nix::sys::signal::{self, Signal};
use nix::unistd::{Pid, getpgid};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use thiserror::Error;

const MAX_READ_BUFFER_BYTES: usize = 65_536;

#[cfg_attr(target_os = "linux", allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CwdResolutionStrategy {
    Procfs,
    Unsupported,
}

#[derive(Debug, Clone)]
pub struct ShellProcessConfig {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub initial_cwd: PathBuf,
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

        let read_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let read_error = Arc::new(Mutex::new(None));
        spawn_reader_thread(reader, Arc::clone(&read_buffer), Arc::clone(&read_error));

        Ok(Self {
            master: pair.master,
            child,
            writer,
            read_buffer,
            read_error,
            process_id,
            process_group_id,
        })
    }

    pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), ProcessError> {
        self.writer
            .write_all(bytes)
            .and_then(|_| self.writer.flush())
            .map_err(|error| ProcessError::Write(error.to_string()))
    }

    pub fn try_read(&mut self) -> Result<Vec<u8>, ProcessError> {
        if let Some(error) = self.read_error.lock().expect("read_error poisoned").take() {
            return Err(ProcessError::Read(error));
        }

        let mut buffer = self.read_buffer.lock().expect("read_buffer poisoned");
        let bytes: Vec<u8> = buffer.drain(..).collect();
        Ok(bytes)
    }

    pub fn send_interrupt(&mut self) -> Result<(), ProcessError> {
        signal::kill(Pid::from_raw(self.process_id as i32), Signal::SIGINT)
            .map_err(|error| ProcessError::Interrupt(error.to_string()))
    }

    pub fn current_dir(&self) -> Result<PathBuf, ProcessError> {
        resolve_current_dir_with_strategy(self.process_id, platform_cwd_resolution_strategy())
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
        let descendants = descendant_pids(self.process_id as i32);
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
}

impl Drop for ShellProcess {
    fn drop(&mut self) {
        let _ = self.terminate_process_group();
    }
}

fn platform_cwd_resolution_strategy() -> CwdResolutionStrategy {
    #[cfg(target_os = "linux")]
    {
        CwdResolutionStrategy::Procfs
    }

    #[cfg(not(target_os = "linux"))]
    {
        CwdResolutionStrategy::Unsupported
    }
}

fn resolve_current_dir_with_strategy(
    process_id: u32,
    strategy: CwdResolutionStrategy,
) -> Result<PathBuf, ProcessError> {
    match strategy {
        CwdResolutionStrategy::Procfs => resolve_current_dir_via_procfs(process_id),
        CwdResolutionStrategy::Unsupported => Err(ProcessError::CurrentDir(
            "cwd resolution unsupported on this platform".to_owned(),
        )),
    }
}

fn resolve_current_dir_via_procfs(process_id: u32) -> Result<PathBuf, ProcessError> {
    let proc_path = PathBuf::from("/proc")
        .join(process_id.to_string())
        .join("cwd");
    fs::read_link(proc_path).map_err(|error| ProcessError::CurrentDir(error.to_string()))
}

fn descendant_pids(root_pid: i32) -> Vec<i32> {
    let mut result = Vec::new();
    collect_descendant_pids(root_pid, &mut result);
    result
}

fn collect_descendant_pids(root_pid: i32, out: &mut Vec<i32>) {
    let path = PathBuf::from("/proc")
        .join(root_pid.to_string())
        .join("task")
        .join(root_pid.to_string())
        .join("children");

    let Ok(children) = fs::read_to_string(path) else {
        return;
    };

    for child_pid in children.split_whitespace() {
        let Ok(child_pid) = child_pid.parse::<i32>() else {
            continue;
        };
        if out.contains(&child_pid) {
            continue;
        }
        out.push(child_pid);
        collect_descendant_pids(child_pid, out);
    }
}

fn spawn_reader_thread(
    mut reader: Box<dyn Read + Send>,
    read_buffer: Arc<Mutex<VecDeque<u8>>>,
    read_error: Arc<Mutex<Option<String>>>,
) {
    thread::spawn(move || {
        let mut buf = [0_u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(count) => {
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

fn truncate_read_buffer(buffer: &mut VecDeque<u8>, max_bytes: usize) {
    while buffer.len() > max_bytes {
        let _ = buffer.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn shell_config(initial_cwd: PathBuf) -> ShellProcessConfig {
        ShellProcessConfig {
            program: PathBuf::from("/bin/sh"),
            args: vec![],
            initial_cwd,
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
        let error = resolve_current_dir_with_strategy(123, CwdResolutionStrategy::Unsupported)
            .expect_err("unsupported strategy must return an error");

        match error {
            ProcessError::CurrentDir(detail) => {
                assert!(detail.contains("unsupported"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn interactive_bash_emits_prompt_into_pty() {
        let temp = tempdir().unwrap();
        let config = ShellProcessConfig {
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
            initial_cwd: temp.path().to_path_buf(),
        };
        let mut process = ShellProcess::spawn(config).unwrap();

        let output =
            read_until_contains(&mut process, "__MTRM_PROMPT__", Duration::from_secs(3)).unwrap();

        assert!(output.contains("__MTRM_PROMPT__"));
    }
}
