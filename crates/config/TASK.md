# `mtrm-config`

## Что это

Библиотека для вычисления и создания служебных путей `mtrm` в домашнем каталоге пользователя.

Разработчик этой библиотеки не должен знать, как устроены вкладки, раскладка или интерфейс терминала. Он должен сделать только одно: стабильный доступ к каталогу `~/.mtrm` и файлам внутри него.

## Публичный интерфейс, который нужно реализовать

```rust
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtrmPaths {
    pub home_dir: PathBuf,
    pub data_dir: PathBuf,
    pub state_file: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("home directory is unavailable")]
    HomeDirUnavailable,
    #[error("failed to create directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub fn resolve_paths() -> Result<MtrmPaths, ConfigError>;

pub fn ensure_data_dir() -> Result<MtrmPaths, ConfigError>;

impl MtrmPaths {
    pub fn data_dir(&self) -> &Path;
    pub fn state_file(&self) -> &Path;
}
```

Точное правило:

- `resolve_paths()` только вычисляет пути;
- `ensure_data_dir()` вычисляет пути и создает `~/.mtrm`, если каталога нет;
- `state_file` должен указывать на один конкретный файл состояния внутри `~/.mtrm`.

Имя файла состояния выбрать и зафиксировать в коде. Рекомендуемое имя: `state.toml`.

## Допустимые зависимости

- `directories`;
- `thiserror`;
- стандартная библиотека Rust;
- `tempfile` только в тестах.

## Что покрыть тестами

- `resolve_paths()` возвращает путь `~/.mtrm`;
- `resolve_paths()` возвращает путь `~/.mtrm/state.toml`;
- `ensure_data_dir()` создает каталог, если его нет;
- повторный вызов `ensure_data_dir()` не падает и не меняет уже вычисленные пути;
- ошибки корректно пробрасываются при невозможности создать каталог.

Для тестов сделать внутреннюю подменяемую функцию или скрытый конструктор, который позволяет задать искусственный домашний каталог.

## Какая документация нужна после тестов

После прохождения тестов создать `README.md` с:

- точным списком путей, которые возвращает библиотека;
- правилом автоматического создания `~/.mtrm`;
- примером использования `ensure_data_dir()`.
