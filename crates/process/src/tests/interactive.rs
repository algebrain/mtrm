use super::helpers::*;
use super::*;
use tempfile::tempdir;

#[test]
fn interactive_bash_emits_prompt_into_pty() {
    let temp = tempdir().unwrap();
    let config = interactive_bash_config(temp.path().to_path_buf());
    let mut process = ShellProcess::spawn(config).unwrap();

    let output = wait_for_prompt(&mut process).unwrap();

    assert!(output.contains("__MTRM_PROMPT__"));
}

#[test]
fn send_interrupt_reaches_foreground_job_in_interactive_bash() {
    let temp = tempdir().unwrap();
    let config = interactive_bash_config(temp.path().to_path_buf());
    let mut process = ShellProcess::spawn(config).unwrap();

    let _ = read_until_contains(&mut process, "__MTRM_PROMPT__", Duration::from_secs(5)).unwrap();
    process.write_all(b"sleep 5\n").unwrap();
    thread::sleep(Duration::from_millis(200));
    process.send_interrupt().unwrap();
    process
        .write_all(b"printf '__INTERRUPTED_BASH__\\n'\n")
        .unwrap();

    let output =
        read_until_contains(&mut process, "__INTERRUPTED_BASH__", Duration::from_secs(3)).unwrap();
    assert!(output.contains("__INTERRUPTED_BASH__"));
}

#[test]
#[ignore = "flaky on CI; foreground raw tty recovery timing is unstable"]
fn send_interrupt_restores_shell_after_foreground_job_leaves_tty_raw() {
    let temp = tempdir().unwrap();
    let config = interactive_bash_config(temp.path().to_path_buf());
    let mut process = ShellProcess::spawn(config).unwrap();
    let _ = read_until_contains(&mut process, "__MTRM_PROMPT__", Duration::from_secs(5)).unwrap();
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
