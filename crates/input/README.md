# `mtrm-input`

`mtrm-input` преобразует одно клавиатурное событие `crossterm::event::KeyEvent` в один из трех результатов:

- `InputAction::Command(AppCommand)` — команда программы;
- `InputAction::PtyBytes(Vec<u8>)` — байты для передачи в псевдотерминал;
- `InputAction::Ignore` — событие, которое библиотека не обрабатывает.

Библиотека не читает файловую систему сама.

Для буквенных горячих клавиш она работает по уже загруженному `Keymap` из `mtrm-keymap`.

## Таблица соответствий

| Клавиша | Результат |
| --- | --- |
| `Ctrl+C` | `Command(AppCommand::Clipboard(ClipboardCommand::CopySelection))` |
| `Ctrl+V` | `Command(AppCommand::Clipboard(ClipboardCommand::PasteFromSystem))` |
| `Alt+X` | `Command(AppCommand::SendInterrupt)` |
| `Alt+-` | `Command(AppCommand::Layout(LayoutCommand::SplitFocused(SplitDirection::Vertical)))` |
| `Alt+=` | `Command(AppCommand::Layout(LayoutCommand::SplitFocused(SplitDirection::Horizontal)))` |
| `Alt+Q` | `Command(AppCommand::Layout(LayoutCommand::CloseFocusedPane))` |
| `Alt+T` | `Command(AppCommand::Tabs(TabCommand::NewTab))` |
| `Alt+,` | `Command(AppCommand::Tabs(TabCommand::PreviousTab))` |
| `Alt+.` | `Command(AppCommand::Tabs(TabCommand::NextTab))` |
| `Alt+W` | `Command(AppCommand::Tabs(TabCommand::CloseCurrentTab))` |
| `Alt+Shift+Q` | `Command(AppCommand::Quit)` |
| `Shift+Up` | `Command(AppCommand::Layout(LayoutCommand::ScrollUpLines(1)))` |
| `Shift+Down` | `Command(AppCommand::Layout(LayoutCommand::ScrollDownLines(1)))` |
| `Shift+PageUp` | `Command(AppCommand::Layout(LayoutCommand::ScrollUpPages(1)))` |
| `Shift+PageDown` | `Command(AppCommand::Layout(LayoutCommand::ScrollDownPages(1)))` |
| `End` | `Command(AppCommand::Layout(LayoutCommand::ScrollToBottom))` |
| `Alt+Left` | `Command(AppCommand::Layout(LayoutCommand::MoveFocus(FocusMoveDirection::Left)))` |
| `Alt+Right` | `Command(AppCommand::Layout(LayoutCommand::MoveFocus(FocusMoveDirection::Right)))` |
| `Alt+Up` | `Command(AppCommand::Layout(LayoutCommand::MoveFocus(FocusMoveDirection::Up)))` |
| `Alt+Down` | `Command(AppCommand::Layout(LayoutCommand::MoveFocus(FocusMoveDirection::Down)))` |
| обычный `Char` без модификаторов | `PtyBytes` в UTF-8 |
| `Enter` | `PtyBytes(vec![b'\n'])` |
| `Backspace` | `PtyBytes(vec![0x08])` |
| `Tab` | `PtyBytes(vec![b'\t'])` |
| неподдержанные сочетания | `Ignore` |

Символы для буквенных горячих клавиш берутся не из захардкоженного списка, а из `Keymap`.

Это значит, что набор поддерживаемых раскладок определяется содержимым `~/.mtrm/keymap.toml`.

## Примеры кодирования в `PtyBytes`

- `KeyCode::Char('a')` -> `vec![0x61]`
- `KeyCode::Char('й')` -> UTF-8 байты строки `"й"`
- `KeyCode::Enter` -> `vec![0x0A]`
- `KeyCode::Tab` -> `vec![0x09]`

## Когда возвращается `Ignore`

`Ignore` возвращается в двух случаях:

- клавиша не входит в поддерживаемую таблицу соответствий;
- у события есть модификаторы, для которых в библиотеке не задано явное правило.
