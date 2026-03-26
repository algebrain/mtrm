# `mtrm-layout`

## Что это

Библиотека раскладки окон внутри одной вкладки.

Разработчик этой библиотеки не должен знать ничего про псевдотерминалы, буфер обмена или файловую систему. Он работает только со структурой разбиений, активным окном и прямоугольниками для отрисовки.

## Публичный интерфейс, который нужно реализовать

```rust
use mtrm_core::{FocusMoveDirection, PaneId, SplitDirection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    EmptyLayout,
    PaneNotFound(PaneId),
    CannotSplitMissingPane(PaneId),
    CannotCloseLastPane,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanePlacement {
    pub pane_id: PaneId,
    pub rect: Rect,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutSnapshot {
    // Структура выбирается исполнителем, но тип должен быть сериализуемым.
}

#[derive(Debug, Clone)]
pub struct LayoutTree {
    // Внутреннее устройство выбирается исполнителем.
}

impl LayoutTree {
    pub fn new(root_pane: PaneId) -> Self;
    pub fn focused_pane(&self) -> PaneId;
    pub fn contains(&self, pane_id: PaneId) -> bool;
    pub fn split_focused(&mut self, direction: SplitDirection, new_pane: PaneId) -> PaneId;
    pub fn close_focused(&mut self) -> Result<PaneId, LayoutError>;
    pub fn focus_pane(&mut self, pane_id: PaneId) -> Result<(), LayoutError>;
    pub fn move_focus(&mut self, direction: FocusMoveDirection) -> Result<PaneId, LayoutError>;
    pub fn pane_ids(&self) -> Vec<PaneId>;
    pub fn placements(&self, area: Rect) -> Vec<PanePlacement>;
    pub fn to_snapshot(&self) -> LayoutSnapshot;
    pub fn from_snapshot(snapshot: LayoutSnapshot) -> Result<Self, LayoutError>;
}
```

Дополнительное точное поведение:

- `split_focused` оставляет фокус на старом окне или переводит его на новое окно, но выбранное правило должно быть зафиксировано в документации и тестах;
- `close_focused` возвращает `PaneId` закрытого окна;
- закрытие последнего окна должно возвращать `LayoutError::CannotCloseLastPane`;
- `placements()` должен возвращать все окна без потерь и дубликатов.

`LayoutSnapshot` обязан поддерживать `serde::Serialize` и `serde::Deserialize`.

## Допустимые зависимости

- `mtrm-core`;
- `serde`;
- стандартная библиотека Rust;
- `proptest` только в тестах.

## Что покрыть тестами

- `LayoutTree::new`;
- `split_focused` для горизонтального и вертикального разбиения;
- `close_focused` для случая двух и более окон;
- запрет на закрытие последнего окна;
- `focus_pane` для существующего и несуществующего окна;
- `move_focus` на простой и составной раскладке;
- `placements()` возвращает корректное число прямоугольников;
- `to_snapshot()` и `from_snapshot()` сохраняют структуру и фокус;
- свойства через `proptest`: после случайной последовательности разбиений и закрытий дерево остается валидным.

## Какая документация нужна после тестов

После прохождения тестов создать `README.md` с:

- полным описанием `LayoutTree`;
- выбранным правилом смены фокуса после разбиения и закрытия;
- примером вызовов `split_focused`, `move_focus`, `to_snapshot`.

