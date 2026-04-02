# `mtrm-keymap`

`mtrm-keymap` хранит и загружает настраиваемую таблицу буквенных горячих клавиш `mtrm`.

## Что делает библиотека

- содержит встроенный в бинарник `keymap.toml` по умолчанию;
- создает `~/.mtrm/keymap.toml`, если файла еще нет;
- читает `keymap.toml` с диска;
- валидирует обязательные команды;
- отдает готовую структуру `Keymap` для слоя ввода.

## Публичный интерфейс

- `Keymap`
  - структура с наборами символов для буквенных команд;
- `Keymap::from_toml_str(&str) -> Result<Keymap, KeymapError>`
- `default_keymap_toml() -> &'static str`
- `keymap_file_path() -> Result<PathBuf, KeymapError>`
- `ensure_keymap_file() -> Result<PathBuf, KeymapError>`
- `load_keymap() -> Result<Keymap, KeymapError>`
- `load_keymap_from_path(path: &Path) -> Result<Keymap, KeymapError>`

## Формат файла

```toml
[commands]
copy = ["c", "с"]
paste = ["v", "м"]
interrupt = ["x", "ч"]
close_pane = ["q", "й"]
new_tab = ["t", "е"]
close_tab = ["w", "ц"]
rename_tab = ["R", "К"]
rename_pane = ["E", "У"]
quit = ["Q", "Й"]
previous_tab = [",", "б"]
next_tab = [".", "ю"]
```

Это не привязка к физическим клавишам.

Это настраиваемый символьный слой: пользователь перечисляет символы, которые должны считаться одной и той же командой в его раскладках.

Во встроенном keymap по умолчанию уже есть:

- латинские символы;
- испанские и португальские раскладки через те же латинские символы;
- русская раскладка;
- французский AZERTY для буквенных команд `close_pane`, `close_tab`, `rename_pane` и `quit`;
- греческие символы для `copy`, `paste`, `interrupt`, `new_tab`, `close_tab`, `close_pane` и `quit`.

## Архитектурная граница

- `mtrm-keymap` работает с файлами и форматом `TOML`;
- `mtrm-input` только сопоставляет `KeyEvent` с командами по уже загруженному `Keymap`;
- `app` отвечает за загрузку keymap при старте приложения.
