use super::helpers::*;
use super::*;
use tempfile::tempdir;

#[test]
fn termios_restore_is_needed_when_canonical_echo_and_signal_flags_are_missing() {
    let temp = tempdir().unwrap();
    let process = ShellProcess::spawn(shell_config(temp.path().to_path_buf())).unwrap();
    let baseline = process
        .baseline_termios
        .clone()
        .expect("pty baseline termios must be available");
    let mut raw_like = baseline.clone();
    raw_like
        .local_flags
        .remove(LocalFlags::ICANON | LocalFlags::ECHO | LocalFlags::ISIG);

    assert!(termios_needs_restore(&raw_like, &baseline));
    assert!(!termios_needs_restore(&baseline, &baseline));
}

#[test]
fn termios_restore_is_needed_when_sane_input_output_and_echo_flags_drift() {
    let temp = tempdir().unwrap();
    let process = ShellProcess::spawn(shell_config(temp.path().to_path_buf())).unwrap();
    let baseline = process
        .baseline_termios
        .clone()
        .expect("pty baseline termios must be available");
    let mut drifted = baseline.clone();
    drifted
        .input_flags
        .remove(InputFlags::ICRNL | InputFlags::IXON);
    drifted
        .output_flags
        .remove(OutputFlags::OPOST | OutputFlags::ONLCR);
    drifted
        .local_flags
        .remove(LocalFlags::IEXTEN | LocalFlags::ECHOE | LocalFlags::ECHOK);

    assert!(termios_needs_restore(&drifted, &baseline));
}

#[test]
fn termios_restore_is_needed_when_control_characters_drift() {
    let temp = tempdir().unwrap();
    let process = ShellProcess::spawn(shell_config(temp.path().to_path_buf())).unwrap();
    let baseline = process
        .baseline_termios
        .clone()
        .expect("pty baseline termios must be available");
    let mut drifted = baseline.clone();
    drifted.control_chars[SpecialCharacterIndices::VINTR as usize] = 0;
    drifted.control_chars[SpecialCharacterIndices::VERASE as usize] = 8;

    assert!(termios_needs_restore(&drifted, &baseline));
}

#[test]
fn restore_baseline_termios_recovers_shell_from_raw_like_mode() {
    let temp = tempdir().unwrap();
    let mut process = ShellProcess::spawn(shell_config(temp.path().to_path_buf())).unwrap();
    let baseline = process
        .baseline_termios
        .clone()
        .expect("pty baseline termios must be available");

    process.write_all(b"stty raw -echo\n").unwrap();
    let raw_mode_set = wait_until(Duration::from_secs(2), || {
        process
            .master
            .get_termios()
            .map(|termios| termios_needs_restore(&termios, &baseline))
            .unwrap_or(false)
    });
    assert!(raw_mode_set, "shell did not switch pty into raw-like mode");

    process.restore_baseline_termios_if_needed().unwrap();
    let restored = process.master.get_termios().expect("current termios");

    assert!(restored.local_flags.contains(LocalFlags::ICANON));
    assert!(restored.local_flags.contains(LocalFlags::ECHO));
    assert!(restored.local_flags.contains(LocalFlags::ISIG));
}

#[test]
fn restore_baseline_termios_preserves_interactive_backspace_behavior() {
    let temp = tempdir().unwrap();
    let config = interactive_bash_config(temp.path().to_path_buf());
    let mut process = ShellProcess::spawn(config).unwrap();

    let _ = wait_for_prompt(&mut process).unwrap();
    process.write_all(b"stty raw -echo\n").unwrap();
    let _ = read_until_contains(&mut process, "raw", Duration::from_secs(2)).ok();
    process.restore_baseline_termios_if_needed().unwrap();

    let _ = process.try_read().unwrap();
    process.write_all(b"echo abc\x7fd\n").unwrap();

    let output =
        read_until_contains(&mut process, "__MTRM_PROMPT__", Duration::from_secs(3)).unwrap();
    assert!(
        output.contains("abd"),
        "expected interactive backspace to erase the previous character; got {output:?}"
    );
    assert!(
        !output.contains("^H"),
        "shell echoed raw backspace instead of line editing; got {output:?}"
    );
}

#[test]
fn interactive_bash_baseline_tracks_shell_prompt_state() {
    let temp = tempdir().unwrap();
    let config = interactive_bash_config(temp.path().to_path_buf());
    let mut process = ShellProcess::spawn(config).unwrap();

    let _ = wait_for_prompt(&mut process).unwrap();
    let baseline_after_prompt = process
        .baseline_termios
        .clone()
        .expect("shell prompt baseline termios must be available");
    let current = process.master.get_termios().expect("current termios");
    assert_eq!(baseline_after_prompt, current);
}

#[test]
fn interactive_bash_prompt_time_control_chars_match_shell_termios() {
    let temp = tempdir().unwrap();
    let config = interactive_bash_config(temp.path().to_path_buf());
    let mut process = ShellProcess::spawn(config).unwrap();

    let _ = wait_for_prompt(&mut process).unwrap();
    let baseline = process
        .baseline_termios
        .clone()
        .expect("shell prompt baseline termios must be available");
    let expected_intr =
        control_char_to_stty_notation(baseline.control_chars[SpecialCharacterIndices::VINTR as usize]);
    let expected_erase = control_char_to_stty_notation(
        baseline.control_chars[SpecialCharacterIndices::VERASE as usize],
    );

    process
        .write_all(b"stty -a; printf '__TTY_CHARS_DONE__\\n'\n")
        .unwrap();
    let output =
        read_until_contains(&mut process, "__TTY_CHARS_DONE__", Duration::from_secs(3)).unwrap();

    assert!(
        output.contains(&format!("intr = {expected_intr};")),
        "shell reported unexpected intr char; expected {expected_intr:?}, got {output:?}"
    );
    assert!(
        output.contains(&format!("erase = {expected_erase};")),
        "shell reported unexpected erase char; expected {expected_erase:?}, got {output:?}"
    );
}
