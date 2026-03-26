# `mtrm-clipboard`

## Что это

Библиотека для чтения и записи текста в системный буфер обмена.

Разработчик этой библиотеки не должен знать ничего про проект целиком. Его задача: сделать маленький изолированный интерфейс над системным буфером обмена и дать тестируемую абстракцию.

## Публичный интерфейс, который нужно реализовать

```rust
#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("failed to read from system clipboard: {0}")]
    Read(String),
    #[error("failed to write to system clipboard: {0}")]
    Write(String),
}

pub trait ClipboardBackend: Send {
    fn get_text(&mut self) -> Result<String, ClipboardError>;
    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError>;
}

pub struct SystemClipboard {
    // Внутреннее устройство выбирается исполнителем.
}

impl SystemClipboard {
    pub fn new() -> Result<Self, ClipboardError>;
}

impl ClipboardBackend for SystemClipboard {
    fn get_text(&mut self) -> Result<String, ClipboardError>;
    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError>;
}
```

Дополнительно нужно сделать тестовую заглушку:

```rust
#[derive(Debug, Default)]
pub struct MemoryClipboard {
    // Хранит текст в памяти процесса.
}

impl MemoryClipboard {
    pub fn new() -> Self;
}

impl ClipboardBackend for MemoryClipboard {
    fn get_text(&mut self) -> Result<String, ClipboardError>;
    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError>;
}
```

`MemoryClipboard` может быть публичным или доступным только в тестах, но он обязан существовать, потому что на нем будут строиться тесты более высокого уровня.

## Допустимые зависимости

- `arboard`;
- `thiserror`;
- стандартная библиотека Rust.

## Что покрыть тестами

- `MemoryClipboard::new` возвращает пустое состояние или согласованное начальное состояние;
- `MemoryClipboard::set_text` сохраняет строку;
- `MemoryClipboard::get_text` возвращает последнюю записанную строку;
- многострочный текст сохраняется без искажений;
- пустая строка сохраняется без ошибок;
- ошибки `SystemClipboard` корректно преобразуются в `ClipboardError`.

Обычные тесты должны работать без настоящего системного буфера обмена.

## Какая документация нужна после тестов

После прохождения тестов создать `README.md` с:

- точным списком публичных типов и методов;
- объяснением, зачем нужен `ClipboardBackend`;
- примером использования `MemoryClipboard` в тестах.

