use std::ffi::CStr;
use std::mem::size_of;
use std::path::PathBuf;

use nix::libc;
use nix::sys::termios::Termios;

use crate::ProcessError;

pub(crate) fn resolve_current_dir_via_libproc(process_id: u32) -> Result<PathBuf, ProcessError> {
    let mut info = std::mem::MaybeUninit::<libc::proc_vnodepathinfo>::zeroed();
    let buffer_size = size_of::<libc::proc_vnodepathinfo>() as libc::c_int;

    // SAFETY: `info` points to valid writable memory sized exactly as required by
    // `proc_pidinfo` for the `PROC_PIDVNODEPATHINFO` flavor.
    let result = unsafe {
        libc::proc_pidinfo(
            process_id as libc::c_int,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            info.as_mut_ptr().cast(),
            buffer_size,
        )
    };

    if result <= 0 {
        return Err(ProcessError::CurrentDir(format!(
            "proc_pidinfo failed for pid {process_id}"
        )));
    }

    if result != buffer_size {
        return Err(ProcessError::CurrentDir(format!(
            "proc_pidinfo returned unexpected size {result} for pid {process_id}"
        )));
    }

    // SAFETY: `proc_pidinfo` reported success and wrote a full `proc_vnodepathinfo`.
    let info = unsafe { info.assume_init() };
    let path = vip_path_to_path_buf(&info.pvi_cdir.vip_path)?;

    if path.as_os_str().is_empty() {
        return Err(ProcessError::CurrentDir(
            "proc_pidinfo returned an empty cwd path".to_owned(),
        ));
    }

    Ok(path)
}

pub(crate) fn resolve_current_dir(process_id: u32) -> Result<PathBuf, ProcessError> {
    resolve_current_dir_via_libproc(process_id)
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
) -> Result<(), std::io::Error> {
    Ok(())
}

fn vip_path_to_path_buf(vip_path: &[[libc::c_char; 32]; 32]) -> Result<PathBuf, ProcessError> {
    let ptr = vip_path.as_ptr().cast::<libc::c_char>();
    // SAFETY: `vip_path` is a MAXPATHLEN-sized C buffer returned by `proc_pidinfo`
    // and is expected to contain a NUL-terminated path on success.
    let path = unsafe { CStr::from_ptr(ptr) };
    let text = path
        .to_str()
        .map_err(|error| ProcessError::CurrentDir(error.to_string()))?;
    Ok(PathBuf::from(text))
}
