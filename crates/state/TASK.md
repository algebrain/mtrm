# `mtrm-state`

## Что это

Библиотека чтения и записи снимка состояния в файл на диске.

Разработчик этой библиотеки не должен знать ничего про терминальный интерфейс. Он получает снимок состояния, сохраняет его в файл и читает обратно.

## Публичный интерфейс, который нужно реализовать

```rust
use std::path::Path;
use mtrm_session::SessionSnapshot;

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("failed to resolve mtrm paths: {0}")]
    Config(String),
    #[error("failed to serialize snapshot: {0}")]
    Serialize(String),
    #[error("failed to deserialize snapshot: {0}")]
    Deserialize(String),
    #[error("failed to read state file {path}: {source}")]
    Read {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write state file {path}: {source}")]
    Write {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub fn load_state() -> Result<Option<SessionSnapshot>, StateError>;

pub fn save_state(snapshot: &SessionSnapshot) -> Result<(), StateError>;

pub fn load_state_from_path(path: &Path) -> Result<Option<SessionSnapshot>, StateError>;

pub fn save_state_to_path(path: &Path, snapshot: &SessionSnapshot) -> Result<(), StateError>;
```

Точное правило:

- `load_state()` использует стандартный путь из `mtrm-config`;
- если файла состояния нет, вернуть `Ok(None)`;
- `save_state()` обязан сначала обеспечить существование `~/.mtrm`;
- запись делать атомарно: временный файл рядом, затем замена основного;
- формат файла состояния выбрать и зафиксировать. Рекомендуемый основной формат: YAML, с legacy fallback чтения старого TOML.

## Допустимые зависимости

- `mtrm-config`;
- `mtrm-session`;
- `toml`;
- `thiserror`;
- `tempfile` только в тестах;
- стандартная библиотека Rust.

## Что покрыть тестами

- `load_state_from_path()` возвращает `Ok(None)` при отсутствии файла;
- `save_state_to_path()` и `load_state_from_path()` сохраняют и читают один и тот же снимок без потерь;
- поврежденный файл дает ошибку десериализации;
- атомарная запись оставляет итоговый файл в корректном состоянии;
- `save_state()` создает служебный каталог, если его нет.

## Какая документация нужна после тестов

После прохождения тестов создать `README.md` с:

- точным списком функций;
- выбранным форматом файла состояния;
- описанием атомарной записи.
