# `mtrm-session`

## Что это

Библиотека чистых данных для снимка состояния `mtrm`.

Разработчик этой библиотеки не должен знать про живые процессы и отрисовку. Он должен описать структуры данных, которые можно сохранить на диск и потом восстановить.

## Публичный интерфейс, который нужно реализовать

```rust
use std::path::PathBuf;
use mtrm_core::{PaneId, TabId};
use mtrm_layout::LayoutSnapshot;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionSnapshot {
    pub tabs: Vec<TabSnapshot>,
    pub active_tab: TabId,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TabSnapshot {
    pub id: TabId,
    pub title: String,
    pub layout: LayoutSnapshot,
    pub panes: Vec<PaneSnapshot>,
    pub active_pane: PaneId,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PaneSnapshot {
    pub id: PaneId,
    pub cwd: PathBuf,
}

impl SessionSnapshot {
    pub fn validate(&self) -> Result<(), SessionValidationError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionValidationError {
    NoTabs,
    MissingActiveTab(TabId),
    DuplicateTabId(TabId),
    DuplicatePaneId(PaneId),
    MissingActivePane { tab_id: TabId, pane_id: PaneId },
    MissingPaneInLayout { tab_id: TabId, pane_id: PaneId },
}
```

Точное правило:

- в снимке обязана быть хотя бы одна вкладка;
- `active_tab` обязан существовать в `tabs`;
- `active_pane` каждой вкладки обязан существовать в `panes`;
- каждое окно, присутствующее в `layout`, обязано иметь запись в `panes`;
- запрещены дублирующиеся `TabId` и `PaneId`.

## Допустимые зависимости

- `mtrm-core`;
- `mtrm-layout`;
- `serde`;
- стандартная библиотека Rust.

## Что покрыть тестами

- корректный снимок с одной вкладкой;
- корректный снимок с несколькими вкладками;
- `validate()` для отсутствующей активной вкладки;
- `validate()` для дублирующегося `TabId`;
- `validate()` для дублирующегося `PaneId`;
- `validate()` для отсутствующего активного окна;
- сериализацию и десериализацию полного снимка.

## Какая документация нужна после тестов

После прохождения тестов создать `README.md` с:

- точным форматом `SessionSnapshot`;
- списком инвариантов `validate()`;
- примером минимального валидного снимка.

