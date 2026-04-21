# `mtrm` User Guide

This is a short user guide: how to start `mtrm`, which keys it handles, and how state persistence works.

## Launching

From the repository root:

```bash
cargo run -p mtrm
```

The binary also supports a few direct flags:

```bash
mtrm --help
mtrm --version
mtrm --no-clipboard
mtrm --debug-log /tmp/mtrm-pty.log
```

- `--help` prints a short help message and exits;
- `--version` prints the version string and exits;
- `--no-clipboard` disables built-in system clipboard integration even if it is available;
- `--debug-log PATH` writes raw PTY chunks into the given file, which is useful when diagnosing terminal emulation issues and fullscreen TUI behavior.

## What the Program Does

`mtrm` runs local shells in pseudoterminals and provides:

- tabs;
- pane splits;
- keyboard-based focus movement between panes;
- system clipboard integration;
- automatic persistence of layout and working directories.

On a normal start, the shell inside a pane runs in interactive mode, so the initial shell output and the command line should be visible immediately.

Current limitation: the cursor is still shown in a simplified way, by visually highlighting the current cell.

After restart, the program restores:

- the set of tabs;
- the layout inside each tab;
- the active tab;
- the active pane;
- the working directory of each pane.

It does not restore old live processes. On startup it creates fresh shells.

If the system clipboard is unavailable, `mtrm` still starts and keeps the rest of the interface working.

## Keybindings

- `Ctrl+C` copies the selected text from the active pane into the system clipboard.
- `Ctrl+V` pastes text from the system clipboard into the active pane.
- `Alt+X` sends `SIGINT` to the active process.
- `Alt+-` splits the active pane into left and right.
- `Alt+=` splits the active pane into top and bottom.
- `Alt+Q` closes the active pane if it is not the last pane in the tab.
- `Alt+T` creates a new tab.
- `Alt+Shift+R` renames the current tab.
- `Alt+Shift+E` renames the current pane.
- `Shift+F1` opens the help overlay.
- `Alt+Shift+Left` / `Alt+Shift+Right` / `Alt+Shift+Up` / `Alt+Shift+Down` resize the active pane by one cell.
- `Alt+,` switches to the previous tab.
- `Alt+.` switches to the next tab.
- `Alt+W` closes the current tab if it is not the last one.
- `Alt+Shift+Q` saves state and quits `mtrm`.
- `Left` / `Right` / `Up` / `Down` send arrows into the active shell.
- `Alt+Left` moves focus left.
- `Alt+Right` moves focus right.
- `Alt+Up` moves focus up.
- `Alt+Down` moves focus down.
- `Shift+Up` scrolls the active pane history up by one line.
- `Shift+Down` scrolls the active pane history down by one line.
- `Shift+PageUp` scrolls the active pane history up by one screen.
- `Shift+PageDown` scrolls the active pane history down by one screen.
- `Home` sends Home into the active shell.
- `End` returns to the live bottom of the active pane.

The help overlay shows the same text as `mtrm --help`. It closes with `Esc` and supports scrolling with arrow keys and `PageUp` / `PageDown` when the terminal is too small to show the full text at once.

By default, letter-based shortcuts like `Alt+T`, `Alt+Q`, `Alt+W`, `Alt+X`, `Alt+Shift+R`, `Alt+Shift+E`, and `Alt+Shift+Q` work for Latin letters, which already covers English, Spanish, and Portuguese layouts, and additionally includes Russian, French AZERTY, and Greek layouts.
The exact set of symbols for letter-based shortcuts is stored in `~/.mtrm/keymap.toml`. If you need another layout, you can add its symbols there.

## Scrollback

By default, the active pane shows the newest output.

If you scroll history upward, the pane enters view mode:

- new output continues to accumulate;
- the screen does not jump down automatically;
- the cursor is hidden in that mode.

You can return to the live bottom by:

- pressing `End`;
- or simply starting to type into the active pane.

## `Ctrl+C` Behavior

In `mtrm`, `Ctrl+C` does not interrupt the process.

It is used to copy the current selection. If nothing is selected, nothing is copied into the clipboard. To interrupt a process, use:

- `Alt+X`

## Clipboard Availability

Built-in copy and paste depend on a working local system clipboard backend.

If that backend is unavailable:

- `mtrm` still starts normally;
- the rest of the interface continues to work;
- `Ctrl+C` and `Ctrl+V` do not terminate the program;
- `mtrm` shows a short notice instead.

If clipboard read or write fails after startup, `mtrm` uses the same short-notice path instead of exiting.

The same in-UI notice path is also used for other recoverable runtime failures, such as state-save errors.

This is especially relevant in remote or headless sessions.

## State Persistence

State is saved automatically.

On the first save, the program creates:

```text
~/.mtrm
```

The state file is stored here:

```text
~/.mtrm/state.yaml
```

If `~/.mtrm/state.yaml` is missing, `mtrm` can still read a legacy `~/.mtrm/state.toml`, but it always saves state back as YAML.
The current YAML state format version is `0.1.0`.

The letter-based keybinding file is stored here:

```text
~/.mtrm/keymap.toml
```

You do not need to configure the path manually.

On a normal exit through `Alt+Shift+Q`, the state is also saved before the program terminates.

Scroll position is not persisted.

## Window Focus Loss

If the outer terminal window loses focus, the active tab and the active pane border are highlighted in red.

## Version String

`mtrm --version` prints:

- the latest local git tag;
- after a space, the modification time of the current executable in Unix seconds.

This is useful when you need to quickly understand which installed binary is actually being run.

## If You Want to Start from a Clean State

It is enough to delete the state file:

```bash
rm ~/.mtrm/state.yaml
```

On the next start, `mtrm` will create a new empty workspace.

## Where to Read Engineering Documents

If you need internal documentation rather than user documentation:

- [Architecture Overview](engineering/ARCHITECTURE.md)
- [Implementation Order](engineering/IMPLEMENTATION_ORDER.md)
