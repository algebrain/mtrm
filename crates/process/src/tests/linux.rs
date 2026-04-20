use super::helpers::*;
use super::*;
use crate::platform_linux::{descendant_pids, lingering_tty_processes_for_interrupted_group};
use nix::libc::SIGINT;
use tempfile::tempdir;

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
