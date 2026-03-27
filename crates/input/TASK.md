# `mtrm-input`

## Что это

Библиотека преобразования клавиатурных событий в команды программы или обычный ввод для псевдотерминала.

Разработчик этой библиотеки не должен знать про вкладки, раскладку и интерфейс отрисовки. Он должен преобразовать входное событие в строго определенный результат.

## Публичный интерфейс, который нужно реализовать

```rust
use crossterm::event::{KeyEvent, KeyModifiers};
use mtrm_core::AppCommand;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    Command(AppCommand),
    PtyBytes(Vec<u8>),
    Ignore,
}

pub fn map_key_event(event: KeyEvent) -> InputAction;
```

Точное обязательное поведение:

- `Ctrl+C` -> `InputAction::Command(AppCommand::Clipboard(ClipboardCommand::CopySelection))`
- `Ctrl+V` -> `InputAction::Command(AppCommand::Clipboard(ClipboardCommand::PasteFromSystem))`
- `Alt+X` -> `InputAction::Command(AppCommand::SendInterrupt)`
- `Alt+Left` -> команда перемещения фокуса влево
- `Alt+Right` -> команда перемещения фокуса вправо
- `Alt+Up` -> команда перемещения фокуса вверх
- `Alt+Down` -> команда перемещения фокуса вниз
- обычные печатные символы -> `PtyBytes`
- служебные клавиши, которые не поддерживаются, -> `Ignore`

Для кодирования байтов использовать обычное UTF-8 представление печатных символов и стандартные управляющие байты там, где это уместно.

## Допустимые зависимости

- `mtrm-core`;
- `crossterm`;
- стандартная библиотека Rust.

## Что покрыть тестами

- `Ctrl+C`;
- `Ctrl+V`;
- `Alt+X`;
- `Alt+Left`, `Alt+Right`, `Alt+Up`, `Alt+Down`;
- обычный символ, например `a`;
- символ не ASCII, например `й` или `ж`;
- `Enter`, `Backspace`, `Tab`, если они кодируются как байты;
- неизвестные или неподдержанные сочетания.

Для каждого теста проверять точный `InputAction`, а не только его вид.

## Какая документация нужна после тестов

После прохождения тестов создать `README.md` с:

- полной таблицей соответствия клавиш и `InputAction`;
- примерами кодирования обычных символов в `PtyBytes`;
- правилом, какие события возвращают `Ignore`.
