# `mtrm-core`

`mtrm-core` содержит базовые типы, идентификаторы и команды, которые используются остальными библиотеками рабочего пространства `mtrm`.

## Публичные типы

- `TabId` — типизированный идентификатор вкладки.
- `PaneId` — типизированный идентификатор окна внутри вкладки.
- `SplitId` — типизированный идентификатор узла разбиения.
- `SplitDirection` — направление разбиения: `Horizontal` или `Vertical`.
- `FocusMoveDirection` — направление переноса фокуса: `Left`, `Right`, `Up`, `Down`.
- `ClipboardCommand` — команды буфера обмена: копирование выделения и вставка из системного буфера.
- `LayoutCommand` — команды раскладки: разбиение активного окна, закрытие активного окна и перенос фокуса.
- `TabCommand` — команды вкладок: создание, закрытие, переключение и явная активация.
- `AppCommand` — верхнеуровневая команда приложения, объединяющая команды буфера обмена, раскладки, вкладок, а также `SendInterrupt`, `RequestSave` и `Quit`.
- `IdAllocator` — простой распределитель идентификаторов вкладок, окон и узлов разбиения.

## Примеры

```rust
use mtrm_core::{
    AppCommand, ClipboardCommand, FocusMoveDirection, IdAllocator, LayoutCommand, SplitDirection,
    TabId, TabCommand,
};

let tab_id = TabId::new(10);
assert_eq!(tab_id.get(), 10);

let command = AppCommand::Layout(LayoutCommand::SplitFocused(SplitDirection::Vertical));

let move_left = AppCommand::Layout(LayoutCommand::MoveFocus(FocusMoveDirection::Left));

let activate = AppCommand::Tabs(TabCommand::Activate(tab_id));

let copy = AppCommand::Clipboard(ClipboardCommand::CopySelection);

let mut allocator = IdAllocator::new();
let first_tab = allocator.next_tab_id();
let first_pane = allocator.next_pane_id();
assert_eq!(first_tab.get(), 0);
assert_eq!(first_pane.get(), 0);
```

## Архитектурное правило

`mtrm-core` не зависит от прикладочных библиотек рабочего пространства.

В этой библиотеке не должно быть:

- логики файловой системы;
- логики терминального интерфейса;
- логики псевдотерминалов и процессов;
- логики буфера обмена;
- зависимостей на `crossterm`, `ratatui`, `portable-pty`, `arboard`.
