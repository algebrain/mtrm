# `mtrm-config`

`mtrm-config` отвечает за вычисление и создание служебных путей `mtrm` в домашнем каталоге пользователя.

## Возвращаемые пути

Библиотека работает со структурой `MtrmPaths`:

- `home_dir` — домашний каталог пользователя;
- `data_dir` — каталог `~/.mtrm`;
- `state_file` — файл состояния `~/.mtrm/state.toml`.

Методы:

- `MtrmPaths::data_dir()` возвращает путь к `~/.mtrm`;
- `MtrmPaths::state_file()` возвращает путь к `~/.mtrm/state.toml`.

## Публичные функции

- `resolve_paths()` вычисляет пути, но не создает каталог на диске;
- `ensure_data_dir()` вычисляет пути и гарантирует, что каталог `~/.mtrm` существует.

## Правило автоматического создания

`mtrm` не требует ручной настройки каталога хранения.

При вызове `ensure_data_dir()` библиотека автоматически создает `~/.mtrm`, если его еще нет. Повторный вызов безопасен и не должен менять уже вычисленные пути.

## Пример использования

```rust
use mtrm_config::ensure_data_dir;

let paths = ensure_data_dir()?;
assert!(paths.data_dir().ends_with(".mtrm"));
assert!(paths.state_file().ends_with(".mtrm/state.toml"));
# Ok::<(), mtrm_config::ConfigError>(())
```
