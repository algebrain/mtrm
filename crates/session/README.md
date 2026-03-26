# `mtrm-session`

`mtrm-session` описывает снимок состояния `mtrm` как чистые сериализуемые данные.

## Формат `SessionSnapshot`

```rust
pub struct SessionSnapshot {
    pub tabs: Vec<TabSnapshot>,
    pub active_tab: TabId,
}

pub struct TabSnapshot {
    pub id: TabId,
    pub title: String,
    pub layout: LayoutSnapshot,
    pub panes: Vec<PaneSnapshot>,
    pub active_pane: PaneId,
}

pub struct PaneSnapshot {
    pub id: PaneId,
    pub cwd: PathBuf,
}
```

Смысл полей:

- `tabs` — все вкладки в порядке хранения;
- `active_tab` — активная вкладка;
- `TabSnapshot::id` — идентификатор вкладки;
- `TabSnapshot::title` — заголовок вкладки;
- `TabSnapshot::layout` — сериализованная раскладка окон внутри вкладки;
- `TabSnapshot::panes` — список окон вкладки и их рабочих каталогов;
- `TabSnapshot::active_pane` — активное окно во вкладке;
- `PaneSnapshot::id` — идентификатор окна;
- `PaneSnapshot::cwd` — рабочий каталог окна.

## Инварианты `validate()`

`SessionSnapshot::validate()` проверяет:

- в снимке есть хотя бы одна вкладка;
- `active_tab` существует в `tabs`;
- идентификаторы вкладок не повторяются;
- идентификаторы окон не повторяются во всем снимке;
- `active_pane` каждой вкладки существует в `panes`;
- каждое окно, которое присутствует в `layout`, имеет запись в `panes`;
- `active_pane` также присутствует в раскладке вкладки.

## Минимальный валидный пример

```rust
use std::path::PathBuf;
use mtrm_core::{PaneId, TabId};
use mtrm_layout::LayoutTree;
use mtrm_session::{PaneSnapshot, SessionSnapshot, TabSnapshot};

let layout = LayoutTree::new(PaneId::new(10)).to_snapshot();

let snapshot = SessionSnapshot {
    tabs: vec![TabSnapshot {
        id: TabId::new(1),
        title: "main".to_owned(),
        layout,
        panes: vec![PaneSnapshot {
            id: PaneId::new(10),
            cwd: PathBuf::from("/tmp"),
        }],
        active_pane: PaneId::new(10),
    }],
    active_tab: TabId::new(1),
};

snapshot.validate()?;
# Ok::<(), mtrm_session::SessionValidationError>(())
```
