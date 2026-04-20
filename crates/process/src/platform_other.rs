use std::io;
use std::path::PathBuf;
use std::time::Duration;

use nix::sys::signal::{self, Signal};
use nix::sys::termios::Termios;
use nix::unistd::Pid;
use portable_pty::MasterPty;

use crate::ProcessError;

pub(crate) fn resolve_current_dir(_process_id: u32) -> Result<PathBuf, ProcessError> {
    Err(ProcessError::CurrentDir(
        "cwd resolution unsupported on this platform".to_owned(),
    ))
}

pub(crate) fn descendant_pids(_root_pid: i32) -> Vec<i32> {
    Vec::new()
}

pub(crate) fn has_lingering_tty_processes_for_interrupted_group(
    _process_id: u32,
    _shell_process_group_id: i32,
    _interrupted_process_group_id: i32,
    _descendants: &[i32],
) -> bool {
    false
}

pub(crate) fn apply_termios_via_shell_tty(
    _process_id: u32,
    _termios: &Termios,
) -> Result<(), io::Error> {
    Ok(())
}

pub(crate) fn cleanup_lingering_tty_processes_after_interrupt(
    _master: &(dyn MasterPty + Send),
    _process_id: u32,
    _shell_process_group_id: i32,
    _interrupted_process_group_id: i32,
    _attempts: usize,
    _recheck_delay: Duration,
) -> Result<(), ProcessError> {
    Ok(())
}

pub(crate) fn terminate_process_tree(
    _process_id: u32,
    process_group_id: i32,
) -> Result<(), ProcessError> {
    signal::kill(Pid::from_raw(-process_group_id), Signal::SIGHUP)
        .map_err(|error| ProcessError::Interrupt(error.to_string()))?;
    std::thread::sleep(Duration::from_millis(100));
    let _ = signal::kill(Pid::from_raw(-process_group_id), Signal::SIGKILL);
    Ok(())
}
