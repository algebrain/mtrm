# `mtrm-terminal-screen`

`mtrm-terminal-screen` хранит экранное состояние одной терминальной панели и обновляет его по потоку байтов из PTY.

## Публичный интерфейс

```rust
pub struct ScreenCell {
    pub text: String,
    pub has_contents: bool,
    pub is_wide: bool,
    pub is_wide_continuation: bool,
    pub fg: ScreenColor,
    pub bg: ScreenColor,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

pub struct ScreenLine {
    pub cells: Vec<ScreenCell>,
}

pub struct TerminalScreen;

impl TerminalScreen {
    pub fn new(rows: u16, cols: u16, scrollback_len: usize) -> Self;
    pub fn process_bytes(&mut self, bytes: &[u8]);
    pub fn resize(&mut self, rows: u16, cols: u16);
    pub fn size(&self) -> (u16, u16);
    pub fn scrollback(&self) -> usize;
    pub fn set_scrollback(&mut self, rows: usize);
    pub fn cursor_position(&self) -> (u16, u16);
    pub fn text_contents(&self) -> String;
    pub fn visible_rows(&self) -> Vec<String>;
    pub fn visible_lines(&self) -> Vec<ScreenLine>;
}
```

## Ответственность

Эта библиотека:

- принимает байты terminal output;
- применяет их к экранному состоянию;
- хранит видимые строки, курсор, атрибуты ячеек и scrollback;
- хранит foreground/background colors terminal cells;
- различает обычные ячейки, wide-char ячейки и continuation cells;
- дает безопасное представление экрана для следующих слоев.

Эта библиотека не должна:

- запускать процессы;
- читать PTY напрямую;
- знать про вкладки;
- рисовать через `ratatui`.

## Текущее основание реализации

Внутри используется `vt100`.

Это значит, что `mtrm-terminal-screen` является адаптером между общим устройством `mtrm` и терминальным движком, а не местом, где вся терминальная логика пишется вручную.

## Что проверено тестами

- prompt появляется в видимой строке после обработки байтов;
- возврат каретки и очистка строки заменяют старое содержимое;
- resize не ломает экран и не теряет работоспособность;
- изменение scrollback меняет видимую часть экрана;
- базовые атрибуты ячеек доступны через `visible_lines()`.
- gap cells видны как отдельные terminal cells даже при пустом `text`;
- continuation-ячейки wide-character помечаются явно.
- foreground и background colors пробрасываются в `ScreenCell`.
