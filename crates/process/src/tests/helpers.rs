use super::*;
pub(super) fn interactive_bash_config(initial_cwd: PathBuf) -> ShellProcessConfig {
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

pub(super) fn shell_config(initial_cwd: PathBuf) -> ShellProcessConfig {
    ShellProcessConfig {
        program: PathBuf::from("/bin/sh"),
        args: vec![],
        initial_cwd,
        debug_log_path: None,
    }
}

pub(super) fn wait_until<F>(timeout: Duration, mut predicate: F) -> bool
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

pub(super) fn read_until_contains(
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

pub(super) fn wait_for_prompt(process: &mut ShellProcess) -> Result<String, ProcessError> {
    read_until_contains(process, "__MTRM_PROMPT__", Duration::from_secs(3))
}

#[cfg(target_os = "linux")]
pub(super) fn proc_signal_masks(pid: u32) -> (u64, u64) {
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

pub(super) fn signal_bit(signo: i32) -> u64 {
    1_u64 << ((signo - 1) as u64)
}

pub(super) fn control_char_to_stty_notation(byte: u8) -> String {
    match byte {
        0 => "undef".to_owned(),
        127 => "^?".to_owned(),
        value @ 1..=31 => format!("^{}", (value + 64) as char),
        value => (value as char).to_string(),
    }
}
