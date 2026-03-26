# mtrm

```text
███╗   ███╗████████╗██████╗ ███╗   ███╗
████╗ ████║╚══██╔══╝██╔══██╗████╗ ████║
██╔████╔██║   ██║   ██████╔╝██╔████╔██║
██║╚██╔╝██║   ██║   ██╔══██╗██║╚██╔╝██║
██║ ╚═╝ ██║   ██║   ██║  ██║██║ ╚═╝ ██║
╚═╝     ╚═╝   ╚═╝   ╚═╝  ╚═╝╚═╝     ╚═╝

  local terminal workspace manager
  split panes  •  selection  •  copy  •  esc  •  home/end

┌──────────────────────────────────────────────────────────────┐
│                                                              │
│   current focus: local, fast, minimal, owned by the user     │
│   no plugin stack, no remote-session assumptions             │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

`mtrm` is a personal terminal workspace manager for local shell work.

It gives you tabs, pane splits, keyboard-driven focus movement, clipboard integration, and automatic persistence of layout and working directories.

This repository is primarily a working codebase for the tool itself, but it also contains the internal engineering notes used to evolve it.

## Platform Status

`mtrm` is currently tested only on Linux.

More specifically, the implementation in this repository has been tested on:

- Linux Mint 22.3

## Installation

### Build and run from source

From the repository root:

```bash
cargo run -p mtrm
```

### Install into `~/.local/bin`

```bash
cargo install --path app --root ~/.local
```

If `~/.local/bin` is in your `PATH`, you can then run:

```bash
mtrm
```

## What It Does

- Runs local shells in PTYs
- Supports multiple tabs
- Splits the active tab into multiple panes
- Moves focus between panes with the keyboard
- Copies and pastes through the system clipboard
- Saves and restores layout, active tab, active pane, and pane working directories

`mtrm` does not restore old live processes after restart. It recreates fresh shells in the saved working directories.

## Default Keybindings

- `Ctrl+C`: copy pane text
- `Ctrl+V`: paste from the system clipboard
- `Ctrl+Shift+C` or `Alt+X`: send `SIGINT` to the active process
- `Alt+-`: split the active pane left/right
- `Alt+=`: split the active pane top/bottom
- `Alt+Q`: close the active pane if it is not the last one
- `Alt+T`: open a new tab
- `Alt+,`: previous tab
- `Alt+.`: next tab
- `Alt+W`: close the current tab if it is not the last one
- `Alt+Shift+Q`: save state and quit
- `Left` / `Right` / `Up` / `Down`: send arrows to the active shell
- `Alt+Left` / `Alt+Right` / `Alt+Up` / `Alt+Down`: move focus between panes
- `Shift+Up` / `Shift+Down`: scroll pane history by one line
- `Shift+PageUp` / `Shift+PageDown`: scroll pane history by one screen
- `End`: return to the live bottom of the active pane

Letter-based shortcuts are configured through `~/.mtrm/keymap.toml`.

The bundled default keymap already covers:

- Latin layouts, including English, Spanish, and Portuguese
- Russian
- French AZERTY
- Greek

## Persistent Files

`mtrm` creates `~/.mtrm` automatically.

Important files:

```text
~/.mtrm/state.toml
~/.mtrm/keymap.toml
```

Scroll position is not persisted. After restart, panes reopen at the live bottom.

## Documentation

User-facing documents:

- [User Guide](docs/USER_GUIDE.md)
- [Application README](app/README.md)

Engineering documents:

- [Architecture Overview](docs/engineering/ARCHITECTURE.md)
- [Implementation Order](docs/engineering/IMPLEMENTATION_ORDER.md)
- [Portability Notes](docs/engineering/PORTABILITY_NOTES.md)
- [Terminal Emulation Notes](docs/engineering/TERMINAL_EMULATION.md)
- [Engineering Idea](docs/engineering/idea.engineering.md)
- [Original Idea Draft](docs/engineering/idea.preliminary.md)
