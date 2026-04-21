use super::*;
use std::fs;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;

fn shell_config(initial_cwd: PathBuf) -> ShellProcessConfig {
    ShellProcessConfig {
        program: PathBuf::from("/bin/sh"),
        args: vec![],
        initial_cwd,
        debug_log_path: None,
    }
}

fn wait_until<F>(timeout: Duration, mut predicate: F) -> bool
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

fn read_until_contains(
    manager: &mut TabManager,
    needle: &str,
    timeout: Duration,
) -> Result<String, TabsError> {
    let deadline = Instant::now() + timeout;
    let mut combined = String::new();

    while Instant::now() < deadline {
        let chunk = manager.read_from_active_pane()?;
        if !chunk.is_empty() {
            combined.push_str(&String::from_utf8_lossy(&chunk));
            if combined.contains(needle) {
                return Ok(combined);
            }
        }
        thread::sleep(Duration::from_millis(20));
    }

    Err(TabsError::Process(format!(
        "timed out waiting for output containing {needle:?}; got {combined:?}"
    )))
}

fn with_env_var_removed<T>(name: &str, f: impl FnOnce() -> T) -> T {
    let previous = std::env::var_os(name);
    unsafe {
        std::env::remove_var(name);
    }
    let result = f();
    if let Some(previous) = previous {
        unsafe {
            std::env::set_var(name, previous);
        }
    }
    result
}

fn find_visible_text_position(
    manager: &TabManager,
    pane_id: PaneId,
    needle: &str,
) -> (u16, u16) {
    let text = manager.pane_text(pane_id).unwrap();
    for (row, line) in text.split('\n').enumerate() {
        if let Some(col) = line.find(needle) {
            return (row as u16, col as u16);
        }
    }
    panic!("could not find {needle:?} in pane text: {text:?}");
}

include!("lifecycle_snapshot_and_io.rs");
include!("alternate_screen_env_and_selection.rs");
