use super::helpers::*;
use super::*;
use tempfile::tempdir;

#[test]
fn current_dir_returns_canonical_temp_path_on_macos() {
    let temp = tempdir().unwrap();
    let canonical_dir = fs::canonicalize(temp.path()).unwrap();
    let canonical_text = canonical_dir.to_string_lossy();
    let alias_text = canonical_text.replacen("/private/var/", "/var/", 1);

    assert_ne!(
        alias_text, canonical_text,
        "test requires a canonical /private/var/... path on macOS"
    );

    let alias_dir = PathBuf::from(alias_text);
    assert!(alias_dir.exists(), "alias temp path must exist: {:?}", alias_dir);

    let mut process = ShellProcess::spawn(shell_config(alias_dir.clone())).unwrap();

    assert_eq!(process.current_dir().unwrap(), canonical_dir);
    assert_ne!(alias_dir, canonical_dir);
}

#[test]
fn current_dir_tracks_canonical_working_directory_after_cd_on_macos() {
    let temp = tempdir().unwrap();
    let canonical_root = fs::canonicalize(temp.path()).unwrap();
    let next_dir = canonical_root.join("next");
    fs::create_dir(&next_dir).unwrap();

    let canonical_text = canonical_root.to_string_lossy();
    let alias_text = canonical_text.replacen("/private/var/", "/var/", 1);
    assert_ne!(
        alias_text, canonical_text,
        "test requires a canonical /private/var/... path on macOS"
    );

    let alias_root = PathBuf::from(alias_text);
    let alias_next = alias_root.join("next");
    assert!(alias_next.exists(), "alias path must exist: {:?}", alias_next);

    let mut process = ShellProcess::spawn(shell_config(alias_root)).unwrap();
    process
        .write_all(format!("cd '{}'\n", alias_next.display()).as_bytes())
        .unwrap();

    let changed = wait_until(Duration::from_secs(2), || {
        process
            .current_dir()
            .map(|cwd| cwd == next_dir)
            .unwrap_or(false)
    });
    assert!(changed, "shell cwd did not change to canonical {:?}", next_dir);
}

#[test]
fn terminate_stops_background_work_started_by_shell_on_macos() {
    let temp = tempdir().unwrap();
    let marker = temp.path().join("terminated-marker.txt");
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

    process.terminate().unwrap();
    thread::sleep(Duration::from_secs(2));

    assert!(
        !marker.exists(),
        "background child survived terminate() and kept running"
    );
}

#[test]
fn send_interrupt_recovers_shell_after_orphan_like_same_tty_job_on_macos() {
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

    let _ = process.try_read().unwrap();
    process.write_all(b"echo ac\x1b[D\x1b[Db\n").unwrap();
    let output =
        read_until_contains(&mut process, "__MTRM_PROMPT__", Duration::from_secs(3)).unwrap();

    assert!(
        output.contains("bac"),
        "expected shell editing to remain healthy after interrupted same-tty job; got {output:?}"
    );
    assert!(
        !output.contains("^[[D"),
        "shell echoed raw left-arrow after interrupted same-tty job; got {output:?}"
    );
}

#[test]
fn send_interrupt_preserves_backspace_behavior_after_late_tty_corruption_on_macos() {
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
        "expected shell backspace editing to remain healthy after late tty corruption; got {output:?}"
    );
    assert!(
        !output.contains("^H"),
        "shell echoed raw backspace after late tty corruption; got {output:?}"
    );
}

#[test]
fn interactive_bash_accepts_interrupt_and_returns_prompt_after_spawn_on_macos() {
    let temp = tempdir().unwrap();
    let config = interactive_bash_config(temp.path().to_path_buf());
    let mut process = ShellProcess::spawn(config).unwrap();

    let _ = wait_for_prompt(&mut process).unwrap();
    process.write_all(b"sleep 5\n").unwrap();
    thread::sleep(Duration::from_millis(200));
    process.send_interrupt().unwrap();
    let _ = wait_for_prompt(&mut process).unwrap();

    process
        .write_all(b"printf '__MACOS_INTERRUPT_READY__\\n'\n")
        .unwrap();
    let output = read_until_contains(
        &mut process,
        "__MACOS_INTERRUPT_READY__",
        Duration::from_secs(3),
    )
    .unwrap();

    assert!(output.contains("__MACOS_INTERRUPT_READY__"));
}

#[test]
fn terminate_stops_interactive_bash_background_work_on_macos() {
    let temp = tempdir().unwrap();
    let marker = temp.path().join("interactive-terminated-marker.txt");
    let config = interactive_bash_config(temp.path().to_path_buf());
    let mut process = ShellProcess::spawn(config).unwrap();

    let _ = wait_for_prompt(&mut process).unwrap();
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

    process.terminate().unwrap();
    thread::sleep(Duration::from_secs(2));

    assert!(
        !marker.exists(),
        "interactive bash background child survived terminate() and kept running"
    );
}
