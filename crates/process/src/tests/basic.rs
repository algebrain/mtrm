use super::helpers::*;
use super::*;
use tempfile::tempdir;

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
    let output = read_until_contains(&mut process, "__MTRM_OK__", Duration::from_secs(2)).unwrap();

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
    let initial_dir = fs::canonicalize(initial_dir).unwrap();
    let next_dir = fs::canonicalize(next_dir).unwrap();
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
    match ProcessError::CurrentDir("cwd resolution unsupported on this platform".to_owned()) {
        ProcessError::CurrentDir(detail) => {
            assert!(detail.contains("unsupported"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}
