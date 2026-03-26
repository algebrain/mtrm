# `mtrm-core`

## Что это

Библиотека общих типов для проекта `mtrm`.

Разработчик этой библиотеки не должен знать устройство других библиотек. Его задача: определить базовые идентификаторы, перечисления и команды, на которые будут опираться остальные части рабочего пространства.

## Что запрещено

В этой библиотеке запрещено:

- работать с файловой системой;
- работать с терминалом;
- работать с буфером обмена;
- работать с псевдотерминалами и процессами;
- подключать `crossterm`, `ratatui`, `portable-pty`, `arboard`.

## Публичный интерфейс, который нужно реализовать

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TabId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PaneId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SplitId(u64);

impl TabId {
    pub fn new(raw: u64) -> Self;
    pub fn get(self) -> u64;
}

impl PaneId {
    pub fn new(raw: u64) -> Self;
    pub fn get(self) -> u64;
}

impl SplitId {
    pub fn new(raw: u64) -> Self;
    pub fn get(self) -> u64;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMoveDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardCommand {
    CopySelection,
    PasteFromSystem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutCommand {
    SplitFocused(SplitDirection),
    CloseFocusedPane,
    MoveFocus(FocusMoveDirection),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabCommand {
    NewTab,
    CloseCurrentTab,
    NextTab,
    PreviousTab,
    Activate(TabId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppCommand {
    Clipboard(ClipboardCommand),
    Layout(LayoutCommand),
    Tabs(TabCommand),
    SendInterrupt,
    RequestSave,
    Quit,
}
```

Если при реализации понадобится генерация новых идентификаторов, добавить отдельный тип:

```rust
#[derive(Debug, Default)]
pub struct IdAllocator;

impl IdAllocator {
    pub fn new() -> Self;
    pub fn next_tab_id(&mut self) -> TabId;
    pub fn next_pane_id(&mut self) -> PaneId;
    pub fn next_split_id(&mut self) -> SplitId;
}
```

Если `serde` подключается, все публичные типы выше должны поддерживать сериализацию и десериализацию.

## Допустимые зависимости

- стандартная библиотека Rust;
- `serde` при необходимости;
- `thiserror` только если действительно вводится общий тип ошибки.

## Что покрыть тестами

- `TabId::new` и `TabId::get`;
- `PaneId::new` и `PaneId::get`;
- `SplitId::new` и `SplitId::get`;
- монотонность выдачи идентификаторов в `IdAllocator`, если он реализован;
- равенство и неравенство идентификаторов;
- сериализацию и десериализацию всех публичных перечислений и идентификаторов, если используется `serde`.

## Какая документация нужна после тестов

После прохождения тестов создать `README.md` с:

- полным списком публичных типов;
- кратким назначением каждого типа;
- правилом, что `core` не зависит от прикладочных библиотек.

