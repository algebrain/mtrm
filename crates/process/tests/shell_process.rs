use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use mtrm_process::{ProcessError, ShellProcess, ShellProcessConfig};
use tempfile::tempdir;

fn shell_config(initial_cwd: PathBuf) -> ShellProcessConfig {
    ShellProcessConfig {
        program: PathBuf::from("/bin/sh"),
        args: Vec::new(),
        initial_cwd,
        debug_log_path: None,
    }
}

fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
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
    let mut output = String::new();
    while Instant::now() < deadline {
        let chunk = process.try_read()?;
        if !chunk.is_empty() {
            output.push_str(&String::from_utf8_lossy(&chunk));
            if output.contains(needle) {
                return Ok(output);
            }
        } else {
            thread::sleep(Duration::from_millis(20));
        }
    }

    Err(ProcessError::Read(format!(
        "timed out waiting for output containing {needle:?}; got {output:?}"
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
    std::fs::create_dir(&initial_dir).unwrap();
    std::fs::create_dir(&next_dir).unwrap();
    let mut process = ShellProcess::spawn(shell_config(initial_dir.clone())).unwrap();

    let initial_ok = wait_until(Duration::from_secs(2), || {
        process
            .current_dir()
            .map(|cwd| cwd == initial_dir)
            .unwrap_or(false)
    });
    assert!(initial_ok, "initial cwd did not stabilize");

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
