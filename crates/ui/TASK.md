# `mtrm-ui`

## Что это

Библиотека отрисовки интерфейса терминального менеджера.

Разработчик этой библиотеки не читает клавиатуру и не управляет процессами. Он получает уже готовое состояние и строит кадр отрисовки.

## Публичный интерфейс, который нужно реализовать

```rust
use mtrm_core::{PaneId, TabId};
use mtrm_layout::Rect;
use mtrm_terminal_screen::ScreenLine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabView {
    pub id: TabId,
    pub title: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneView {
    pub id: PaneId,
    pub title: String,
    pub area: Rect,
    pub active: bool,
    pub lines: Vec<ScreenLine>,
    pub cursor: Option<(u16, u16)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameView {
    pub tabs: Vec<TabView>,
    pub panes: Vec<PaneView>,
}

pub fn render_frame<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    frame_view: &FrameView,
) -> std::io::Result<()>;
```

Точное правило:

- вкладки рисуются отдельной полосой;
- активная вкладка должна визуально отличаться;
- активное окно должно визуально отличаться;
- содержимое окна берется из `PaneView.lines`;
- курсор активной панели рисуется по `PaneView.cursor`;
- функция ничего не знает о живом источнике данных.

Если для тестируемости понадобится разбить отрисовку на дополнительные функции, это разрешено, но `render_frame` обязан остаться основным публичным входом.

## Допустимые зависимости

- `mtrm-core`;
- `mtrm-layout`;
- `mtrm-terminal-screen`;
- `ratatui`;
- стандартная библиотека Rust;
- `insta` только в тестах, если используешь снимки.

## Что покрыть тестами

- отрисовку одной вкладки и одного окна;
- отрисовку нескольких вкладок;
- выделение активной вкладки;
- выделение активного окна;
- корректное размещение экранных строк окна в его прямоугольнике;
- визуальное отображение курсора активной панели.

Тесты могут проверять буфер `ratatui` или снимки через `insta`, но проверки должны быть детерминированными.

## Какая документация нужна после тестов

После прохождения тестов создать `README.md` с:

- описанием `FrameView`, `TabView`, `PaneView`;
- примером данных, которые нужно передать в `render_frame`;
- правилом визуального выделения активных элементов.
