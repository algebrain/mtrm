# `mtrm-tabs`

`mtrm-tabs` управляет живым набором вкладок, их раскладками, процессами оболочек внутри окон и экранным состоянием каждой живой панели.

## Публичные методы `TabManager`

- `new(initial_shell: &ShellProcessConfig) -> Result<TabManager, TabsError>`
- `from_snapshot(snapshot: SessionSnapshot, shell: &ShellProcessConfig) -> Result<TabManager, TabsError>`
- `active_tab_id(&self) -> TabId`
- `active_pane_id(&self) -> PaneId`
- `tab_ids(&self) -> Vec<TabId>`
- `new_tab(&mut self, shell: &ShellProcessConfig) -> Result<TabId, TabsError>`
- `close_active_tab(&mut self) -> Result<TabId, TabsError>`
- `activate_tab(&mut self, tab_id: TabId) -> Result<(), TabsError>`
- `split_active_pane(&mut self, direction: SplitDirection, shell: &ShellProcessConfig) -> Result<PaneId, TabsError>`
- `close_active_pane(&mut self) -> Result<PaneId, TabsError>`
- `move_focus(&mut self, direction: FocusMoveDirection) -> Result<PaneId, TabsError>`
- `write_to_active_pane(&mut self, bytes: &[u8]) -> Result<(), TabsError>`
- `read_from_active_pane(&mut self) -> Result<Vec<u8>, TabsError>`
- `refresh_all_panes(&mut self) -> Result<bool, TabsError>`
- `send_interrupt_to_active_pane(&mut self) -> Result<(), TabsError>`
- `active_pane_cwd(&self) -> Result<PathBuf, TabsError>`
- `pane_lines(&self, pane_id: PaneId) -> Result<Vec<ScreenLine>, TabsError>`
- `pane_cursor(&self, pane_id: PaneId) -> Result<Option<(u16, u16)>, TabsError>`
- `pane_text(&self, pane_id: PaneId) -> Result<String, TabsError>`
- `active_pane_text(&self) -> Result<String, TabsError>`
- `resize_active_tab(&mut self, area: Rect) -> Result<(), TabsError>`
- `scroll_active_pane_up_lines(&mut self, lines: u16) -> Result<(), TabsError>`
- `scroll_active_pane_down_lines(&mut self, lines: u16) -> Result<(), TabsError>`
- `scroll_active_pane_up_pages(&mut self, pages: u16) -> Result<(), TabsError>`
- `scroll_active_pane_down_pages(&mut self, pages: u16) -> Result<(), TabsError>`
- `scroll_active_pane_to_bottom(&mut self) -> Result<(), TabsError>`
- `placements(&self, area: Rect) -> Result<Vec<(PaneId, Rect, bool)>, TabsError>`
- `snapshot(&self) -> Result<SessionSnapshot, TabsError>`

## Экранное состояние панели

У каждой живой панели теперь есть не только `ShellProcess`, но и собственный `TerminalScreen`.

Это означает:

- байты читаются из PTY внутри `mtrm-tabs`;
- экран панели обновляется там же;
- `app` больше не хранит отдельную карту текста панелей;
- `ui` получает экранные линии и курсор панели через `pane_lines()` и `pane_cursor()`;
- временное текстовое представление панели получается через `pane_text()` и `active_pane_text()`.

У каждой панели есть собственное положение просмотра истории.

Если панель прокручена вверх:

- `pane_lines()` и `pane_text()` отдают строки с учетом scrollback;
- `pane_cursor()` возвращает `None`;
- обычный ввод в активную панель автоматически возвращает ее к живому хвосту.

## Правило закрытия последнего окна

Закрытие последнего окна во вкладке запрещено.

`close_active_pane()` опирается на `mtrm-layout` и возвращает ошибку `TabsError::Layout(LayoutError::CannotCloseLastPane)`, если в текущей вкладке осталось только одно окно.

Удаление вкладки делается отдельной операцией `close_active_tab()`. Последнюю вкладку удалить тоже нельзя: в этом случае возвращается `TabsError::CannotCloseLastTab`.

## Пример последовательности

```rust
use std::path::PathBuf;
use mtrm_process::ShellProcessConfig;
use mtrm_core::SplitDirection;
use mtrm_tabs::TabManager;

let shell = ShellProcessConfig {
    program: PathBuf::from("/bin/sh"),
    args: vec![],
    initial_cwd: PathBuf::from("/tmp"),
};

let mut manager = TabManager::new(&shell)?;
manager.split_active_pane(SplitDirection::Vertical, &shell)?;
let snapshot = manager.snapshot()?;

assert_eq!(snapshot.tabs.len(), 1);
assert_eq!(snapshot.tabs[0].panes.len(), 2);
# Ok::<(), mtrm_tabs::TabsError>(())
```
