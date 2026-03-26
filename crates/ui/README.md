# `mtrm-ui`

`mtrm-ui` рисует готовый кадр интерфейса по данным из `FrameView`.

## Структуры данных

- `TabView`
  - `id: TabId` — идентификатор вкладки;
  - `title: String` — отображаемое имя вкладки;
  - `active: bool` — активна ли вкладка.
- `PaneView`
  - `id: PaneId` — идентификатор окна;
  - `title: String` — заголовок окна;
  - `area: Rect` — прямоугольник окна в координатах области содержимого;
  - `active: bool` — активно ли окно;
  - `lines: Vec<ScreenLine>` — видимые строки экранного состояния панели;
  - `cursor: Option<(u16, u16)>` — координаты курсора внутри панели.
- `FrameView`
  - `tabs: Vec<TabView>` — все вкладки верхней полосы;
  - `panes: Vec<PaneView>` — все окна текущего кадра.

## Пример данных для `render_frame`

```rust
use mtrm_core::{PaneId, TabId};
use mtrm_layout::Rect;
use mtrm_terminal_screen::{ScreenCell, ScreenLine};
use mtrm_ui::{FrameView, PaneView, TabView};

let frame_view = FrameView {
    tabs: vec![
        TabView {
            id: TabId::new(1),
            title: "main".to_owned(),
            active: true,
        },
    ],
    panes: vec![
        PaneView {
            id: PaneId::new(10),
            title: "shell".to_owned(),
            area: Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 20,
            },
            active: true,
            lines: vec![ScreenLine {
                cells: vec![
                    ScreenCell {
                        text: "p".to_owned(),
                        has_contents: true,
                        is_wide: false,
                        is_wide_continuation: false,
                        fg: ScreenColor::Default,
                        bg: ScreenColor::Default,
                        bold: false,
                        italic: false,
                        underline: false,
                        inverse: false,
                    },
                ],
            }],
            cursor: Some((0, 0)),
        },
    ],
};
```

## Правило визуального выделения

- вкладки всегда рисуются отдельной верхней полосой;
- активная вкладка выделяется желтым фоном и жирным шрифтом;
- неактивные вкладки рисуются серым цветом;
- каждое окно рисуется рамкой;
- активное окно выделяется желтой рамкой и жирным шрифтом;
- неактивные окна рисуются темно-серой рамкой;
- содержимое окна рисуется не как обычный текстовый paragraph, а как terminal-cell grid по `PaneView.lines`;
- foreground/background colors terminal cells применяются при отрисовке pane content;
- перед записью pane content UI очищает content area панели, чтобы не оставлять артефакты старого кадра;
- курсор активной панели рисуется отдельным контрастным инвертированным блоком поверх уже отрисованной панели по `PaneView.cursor`;
- если курсор попадает на continuation-ячейку wide-character, UI нормализует его на ведущую ячейку символа;
- `mtrm-ui` не читает живой PTY и не знает о терминальной эмуляции сверх уже подготовленного представления.
