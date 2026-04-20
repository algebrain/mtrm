use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use nix::sys::signal::{self, Signal};
use nix::sys::termios::{SetArg, Termios, tcsetattr};
use nix::unistd::Pid;
use portable_pty::MasterPty;

use crate::ProcessError;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ProcSummary {
    pid: u32,
    comm: String,
    state: String,
    parent_pid: u32,
    process_group_id: i32,
    session_id: i32,
}

pub(crate) fn resolve_current_dir_via_procfs(process_id: u32) -> Result<PathBuf, ProcessError> {
    let proc_path = PathBuf::from("/proc")
        .join(process_id.to_string())
        .join("cwd");
    fs::read_link(proc_path).map_err(|error| ProcessError::CurrentDir(error.to_string()))
}

pub(crate) fn resolve_current_dir(process_id: u32) -> Result<PathBuf, ProcessError> {
    resolve_current_dir_via_procfs(process_id)
}

pub(crate) fn descendant_pids(root_pid: i32) -> Vec<i32> {
    let mut result = Vec::new();
    collect_descendant_pids(root_pid, &mut result);
    result
}

pub(crate) fn lingering_tty_processes_for_interrupted_group(
    process_id: u32,
    shell_process_group_id: i32,
    interrupted_process_group_id: i32,
    descendants: &[i32],
) -> Vec<()> {
    tty_attached_processes(process_id)
        .into_iter()
        .filter(|summary| summary.pid != process_id)
        .filter(|summary| summary.process_group_id != shell_process_group_id)
        .filter(|summary| summary.process_group_id == interrupted_process_group_id)
        .filter(|summary| !descendants.contains(&(summary.pid as i32)))
        .map(|_| ())
        .collect()
}

pub(crate) fn has_lingering_tty_processes_for_interrupted_group(
    process_id: u32,
    shell_process_group_id: i32,
    interrupted_process_group_id: i32,
    descendants: &[i32],
) -> bool {
    !lingering_tty_processes_for_interrupted_group(
        process_id,
        shell_process_group_id,
        interrupted_process_group_id,
        descendants,
    )
    .is_empty()
}

pub(crate) fn apply_termios_via_shell_tty(
    process_id: u32,
    termios: &Termios,
) -> Result<(), std::io::Error> {
    let tty_path = shell_tty_path(process_id);
    let tty = OpenOptions::new().read(true).write(true).open(tty_path)?;
    tcsetattr(&tty, SetArg::TCSANOW, termios).map_err(std::io::Error::other)
}

pub(crate) fn cleanup_lingering_tty_processes_after_interrupt(
    master: &(dyn MasterPty + Send),
    process_id: u32,
    shell_process_group_id: i32,
    interrupted_process_group_id: i32,
    attempts: usize,
    recheck_delay: Duration,
) -> Result<(), ProcessError> {
    for _ in 0..attempts {
        thread::sleep(recheck_delay);

        let foreground_process_group_id = master.process_group_leader().map(|pid| pid as i32);
        let shell_is_foreground = foreground_process_group_id == Some(shell_process_group_id);
        let descendants = descendant_pids(process_id as i32);

        let lingering = has_lingering_tty_processes_for_interrupted_group(
            process_id,
            shell_process_group_id,
            interrupted_process_group_id,
            &descendants,
        );

        if !shell_is_foreground || !lingering {
            continue;
        }

        signal::kill(Pid::from_raw(-interrupted_process_group_id), Signal::SIGHUP)
            .map_err(|error| ProcessError::Interrupt(error.to_string()))?;
        let _ = signal::kill(Pid::from_raw(-interrupted_process_group_id), Signal::SIGCONT);
        thread::sleep(Duration::from_millis(50));

        let descendants = descendant_pids(process_id as i32);
        let still_lingering = has_lingering_tty_processes_for_interrupted_group(
            process_id,
            shell_process_group_id,
            interrupted_process_group_id,
            &descendants,
        );

        if !still_lingering {
            return Ok(());
        }

        signal::kill(Pid::from_raw(-interrupted_process_group_id), Signal::SIGTERM)
            .map_err(|error| ProcessError::Interrupt(error.to_string()))?;
        return Ok(());
    }

    Ok(())
}

pub(crate) fn terminate_process_tree(process_id: u32, process_group_id: i32) -> Result<(), ProcessError> {
    let descendants = descendant_pids(process_id as i32);

    for pid in &descendants {
        let _ = signal::kill(Pid::from_raw(*pid), Signal::SIGHUP);
    }
    signal::kill(Pid::from_raw(-process_group_id), Signal::SIGHUP)
        .map_err(|error| ProcessError::Interrupt(error.to_string()))?;
    thread::sleep(Duration::from_millis(100));
    for pid in descendants.into_iter().rev() {
        let _ = signal::kill(Pid::from_raw(pid), Signal::SIGKILL);
    }
    let _ = signal::kill(Pid::from_raw(-process_group_id), Signal::SIGKILL);
    Ok(())
}

fn tty_attached_processes(process_id: u32) -> Vec<ProcSummary> {
    let tty_path = shell_tty_path(process_id);
    let Ok(shell_tty_target) = fs::read_link(&tty_path) else {
        return Vec::new();
    };

    let mut attached = Vec::new();
    let Ok(proc_entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };

    for entry in proc_entries.flatten() {
        let Ok(file_name) = entry.file_name().into_string() else {
            continue;
        };
        let Ok(pid) = file_name.parse::<u32>() else {
            continue;
        };
        let fd0_path = entry.path().join("fd").join("0");
        let Ok(target) = fs::read_link(fd0_path) else {
            continue;
        };
        if target != shell_tty_target {
            continue;
        }
        if let Some(summary) = describe_process(pid) {
            attached.push(summary);
        }
    }

    attached.sort();
    attached
}

fn describe_process(pid: u32) -> Option<ProcSummary> {
    let proc_dir = PathBuf::from("/proc").join(pid.to_string());
    let comm = fs::read_to_string(proc_dir.join("comm"))
        .map(|text| text.trim().to_owned())
        .ok()?;
    let stat = fs::read_to_string(proc_dir.join("stat")).ok()?;

    let (state, parent_pid, process_group_id, session_id) = parse_proc_stat_summary(&stat)?;
    Some(ProcSummary {
        pid,
        comm,
        state,
        parent_pid,
        process_group_id,
        session_id,
    })
}

fn parse_proc_stat_summary(stat: &str) -> Option<(String, u32, i32, i32)> {
    let close_paren_index = stat.rfind(") ")?;
    let remainder = stat.get(close_paren_index + 2..)?;
    let mut fields = remainder.split_whitespace();
    let state = fields.next()?.to_owned();
    let parent_pid = fields.next()?.parse().ok()?;
    let process_group_id = fields.next()?.parse().ok()?;
    let session_id = fields.next()?.parse().ok()?;
    Some((state, parent_pid, process_group_id, session_id))
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

fn shell_tty_path(process_id: u32) -> PathBuf {
    PathBuf::from("/proc")
        .join(process_id.to_string())
        .join("fd")
        .join("0")
}
