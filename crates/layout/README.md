# `mtrm-layout`

`mtrm-layout` хранит раскладку окон внутри одной вкладки как нормализованное n-арное дерево разбиений.

## Модель данных

`LayoutTree` состоит из:

- корневого узла дерева;
- `focused_pane`, который указывает на активное окно.

Узел дерева бывает двух видов:

- лист с `PaneId`;
- внутренний узел `Split` с направлением `Horizontal` или `Vertical` и набором дочерних узлов.

Узлы `Split` нормализованы:

- если у родителя и ребенка одно и то же направление, они схлопываются в один контейнер;
- соседние уровни одного направления не живут как отдельные узлы;
- в runtime и snapshot-модели каждый child split-контейнера имеет целочисленный `weight`.

Сериализуемое представление — `LayoutSnapshot`. Оно хранит ту же структуру дерева и идентификатор активного окна. При чтении `LayoutSnapshot` также умеет принять старую бинарную форму split-узлов и привести ее к новой нормализованной модели.

## Правила поведения

- `LayoutTree::new(root_pane)` создает раскладку из одного окна.
- `split_focused(direction, new_pane)` разбивает текущее активное окно, при необходимости локально схлопывает split того же направления и переводит фокус на новое окно.
- `close_focused()` закрывает активное окно и переводит фокус на первое окно в соседнем поддереве.
- `resize_focused(direction, area)` сдвигает ближайшую границу на одну ячейку, если после этого pane не станет меньше минимального размера.
- если в раскладке осталось только одно окно, `close_focused()` возвращает `LayoutError::CannotCloseLastPane`.
- `move_focus(direction)` ищет соседнее окно по геометрии отрисовки:
  - сначала выбирается ближайшее окно в нужном направлении;
  - при равенстве выбирается окно с меньшим смещением по поперечной оси;
  - если равенство сохраняется, выбирается верхний кандидат для лево-право или левый кандидат для верх-низ.

## Примеры

```rust
use mtrm_core::{FocusMoveDirection, PaneId, ResizeDirection, SplitDirection};
use mtrm_layout::{LayoutTree, Rect};

let mut layout = LayoutTree::new(PaneId::new(1));

layout.split_focused(SplitDirection::Vertical, PaneId::new(2));
layout.split_focused(SplitDirection::Horizontal, PaneId::new(3));
layout.resize_focused(
    ResizeDirection::Right,
    Rect { x: 0, y: 0, width: 120, height: 40 },
)?;

let focused = layout.move_focus(FocusMoveDirection::Left)?;
assert_eq!(focused, PaneId::new(1));

let snapshot = layout.to_snapshot();
let restored = LayoutTree::from_snapshot(snapshot)?;

let placements = restored.placements(Rect {
    x: 0,
    y: 0,
    width: 120,
    height: 40,
});
assert_eq!(placements.len(), 3);
# Ok::<(), mtrm_layout::LayoutError>(())
```
