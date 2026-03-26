# `mtrm-tabs`

## Что это

Библиотека управления живым набором вкладок и окон.

Разработчик этой библиотеки работает с тремя вещами:

- раскладка;
- снимок состояния;
- живые процессы оболочки.

Он не должен заниматься отрисовкой и чтением клавиатурных событий.

## Публичный интерфейс, который нужно реализовать

```rust
use std::path::{Path, PathBuf};
use mtrm_core::{FocusMoveDirection, PaneId, SplitDirection, TabId};
use mtrm_layout::{LayoutError, LayoutTree, Rect};
use mtrm_process::{ProcessError, ShellProcess, ShellProcessConfig};
use mtrm_session::{PaneSnapshot, SessionSnapshot, TabSnapshot};

#[derive(Debug, thiserror::Error)]
pub enum TabsError {
    #[error("tab not found: {0:?}")]
    TabNotFound(TabId),
    #[error("pane not found: {0:?}")]
    PaneNotFound(PaneId),
    #[error("layout error: {0:?}")]
    Layout(LayoutError),
    #[error("process error: {0}")]
    Process(String),
    #[error("cannot close last tab")]
    CannotCloseLastTab,
}

pub struct RuntimePane {
    pub id: PaneId,
    pub cwd: PathBuf,
}

pub struct RuntimeTab {
    pub id: TabId,
    pub title: String,
    pub layout: LayoutTree,
}

pub struct TabManager {
    // Внутреннее устройство выбирается исполнителем.
}

impl TabManager {
    pub fn new(initial_shell: &ShellProcessConfig) -> Result<Self, TabsError>;
    pub fn from_snapshot(
        snapshot: SessionSnapshot,
        shell: &ShellProcessConfig,
    ) -> Result<Self, TabsError>;
    pub fn active_tab_id(&self) -> TabId;
    pub fn active_pane_id(&self) -> PaneId;
    pub fn tab_ids(&self) -> Vec<TabId>;
    pub fn new_tab(&mut self, shell: &ShellProcessConfig) -> Result<TabId, TabsError>;
    pub fn close_active_tab(&mut self) -> Result<TabId, TabsError>;
    pub fn activate_tab(&mut self, tab_id: TabId) -> Result<(), TabsError>;
    pub fn split_active_pane(
        &mut self,
        direction: SplitDirection,
        shell: &ShellProcessConfig,
    ) -> Result<PaneId, TabsError>;
    pub fn close_active_pane(&mut self) -> Result<PaneId, TabsError>;
    pub fn move_focus(&mut self, direction: FocusMoveDirection) -> Result<PaneId, TabsError>;
    pub fn write_to_active_pane(&mut self, bytes: &[u8]) -> Result<(), TabsError>;
    pub fn read_from_active_pane(&mut self) -> Result<Vec<u8>, TabsError>;
    pub fn send_interrupt_to_active_pane(&mut self) -> Result<(), TabsError>;
    pub fn active_pane_cwd(&self) -> Result<PathBuf, TabsError>;
    pub fn resize_active_tab(&mut self, cols: u16, rows: u16) -> Result<(), TabsError>;
    pub fn placements(&self, area: Rect) -> Result<Vec<(PaneId, Rect, bool)>, TabsError>;
    pub fn snapshot(&self) -> Result<SessionSnapshot, TabsError>;
}
```

Точное правило:

- `new` создает один таб с одним окном;
- `from_snapshot` восстанавливает вкладки и окна, но запускает новые процессы;
- `close_active_tab` запрещен, если вкладка последняя;
- `close_active_pane` внутри вкладки с одним окном должен либо запрещаться, либо закрывать вкладку по явно зафиксированному правилу. Выбрать одно правило и закрепить в тестах и документации;
- `active_pane_cwd` должен брать текущий каталог из живого процесса, а не из устаревшего снимка.

## Допустимые зависимости

- `mtrm-core`;
- `mtrm-layout`;
- `mtrm-process`;
- `mtrm-session`;
- `thiserror`;
- стандартная библиотека Rust.

## Что покрыть тестами

- `new` создает одну вкладку и одно окно;
- `new_tab` добавляет вкладку;
- `activate_tab` меняет активную вкладку;
- `split_active_pane` добавляет окно;
- `close_active_pane` корректно удаляет окно;
- `snapshot` отражает текущее состояние;
- `from_snapshot` восстанавливает вкладки и окна;
- `send_interrupt_to_active_pane` вызывается у активного процесса;
- `active_pane_cwd` возвращает актуальный каталог.

Если потребуется, использовать подменяемую фабрику процессов, чтобы тесты не зависели от реальных оболочек.

## Какая документация нужна после тестов

После прохождения тестов создать `README.md` с:

- полным списком публичных методов `TabManager`;
- точным правилом закрытия последнего окна во вкладке;
- примером последовательности: создать вкладку, разбить окно, получить снимок.

