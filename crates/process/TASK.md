# `mtrm-process`

## Что это

Библиотека управления псевдотерминалом и запущенной в нем оболочкой.

Разработчик этой библиотеки не должен знать ничего про вкладки, отрисовку и сохранение состояния. Он должен дать интерфейс для жизни одного окна с оболочкой.

## Публичный интерфейс, который нужно реализовать

```rust
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ShellProcessConfig {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub initial_cwd: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("failed to spawn shell: {0}")]
    Spawn(String),
    #[error("failed to write to pty: {0}")]
    Write(String),
    #[error("failed to read from pty: {0}")]
    Read(String),
    #[error("failed to send interrupt: {0}")]
    Interrupt(String),
    #[error("failed to resolve cwd: {0}")]
    CurrentDir(String),
}

pub struct ShellProcess {
    // Внутреннее устройство выбирается исполнителем.
}

impl ShellProcess {
    pub fn spawn(config: ShellProcessConfig) -> Result<Self, ProcessError>;
    pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), ProcessError>;
    pub fn try_read(&mut self) -> Result<Vec<u8>, ProcessError>;
    pub fn send_interrupt(&mut self) -> Result<(), ProcessError>;
    pub fn current_dir(&self) -> Result<PathBuf, ProcessError>;
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), ProcessError>;
    pub fn is_alive(&mut self) -> Result<bool, ProcessError>;
    pub fn terminate(&mut self) -> Result<(), ProcessError>;
}
```

Дополнительные требования:

- `spawn` запускает процесс оболочки в псевдотерминале;
- `try_read` не должен бесконечно блокировать главный цикл;
- `current_dir` для Linux сначала реализовать через `/proc/<pid>/cwd`;
- код должен быть построен так, чтобы позже можно было заменить способ определения каталога.

## Допустимые зависимости

- `portable-pty`;
- `thiserror`;
- `nix`, если нужен для сигналов;
- стандартная библиотека Rust;
- `tempfile` и другие тестовые зависимости только в тестах.

## Что покрыть тестами

- `spawn` создает живой процесс;
- `write_all` позволяет отправить команду оболочке;
- `try_read` возвращает байты после выполнения команды;
- `send_interrupt` прерывает долгую команду;
- `current_dir` возвращает каталог, указанный при запуске, а затем меняется после `cd`;
- `resize` не падает на валидных размерах;
- `terminate` завершает процесс;
- `is_alive` меняется после завершения.

Интеграционные тесты могут использовать реальную оболочку, но должны быть написаны так, чтобы не зависеть от нестабильного вывода приглашения.

## Какая документация нужна после тестов

После прохождения тестов создать `README.md` с:

- точными сигнатурами публичного интерфейса;
- описанием семантики `try_read`;
- описанием способа определения текущего каталога.

