# `mtrm-clipboard`

`mtrm-clipboard` дает маленький изолированный интерфейс для работы с текстовым буфером обмена.

## Публичные типы и методы

- `ClipboardError`
  - `Read(String)` — ошибка чтения из системного буфера;
  - `Write(String)` — ошибка записи в системный буфер.
- `ClipboardBackend`
  - `get_text(&mut self) -> Result<String, ClipboardError>`;
  - `set_text(&mut self, text: &str) -> Result<(), ClipboardError>`.
- `SystemClipboard`
  - `SystemClipboard::new() -> Result<SystemClipboard, ClipboardError>`;
  - реализует `ClipboardBackend` через `arboard`.
- `MemoryClipboard`
  - `MemoryClipboard::new() -> MemoryClipboard`;
  - реализует `ClipboardBackend`, храня текст в памяти процесса.

## Зачем нужен `ClipboardBackend`

`ClipboardBackend` отделяет код приложения от конкретной реализации буфера обмена.

Это дает две вещи:

- в рабочем приложении можно использовать `SystemClipboard`;
- в тестах можно использовать `MemoryClipboard`, не обращаясь к настоящему системному буферу.

## Пример использования `MemoryClipboard` в тестах

```rust
use mtrm_clipboard::{ClipboardBackend, MemoryClipboard};

let mut clipboard = MemoryClipboard::new();

clipboard.set_text("line 1\nline 2")?;
assert_eq!(clipboard.get_text()?, "line 1\nline 2");

# Ok::<(), mtrm_clipboard::ClipboardError>(())
```
