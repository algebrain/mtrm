use std::io;
use std::path::PathBuf;

use nix::sys::termios::Termios;

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
