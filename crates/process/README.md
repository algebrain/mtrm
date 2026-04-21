# `mtrm-process`

`mtrm-process` управляет одним псевдотерминалом и одной запущенной в нем оболочкой.

## Публичный интерфейс

```rust
pub struct ShellProcessConfig {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub initial_cwd: PathBuf,
}

pub enum ProcessError {
    Spawn(String),
    Write(String),
    Read(String),
    Interrupt(String),
    CurrentDir(String),
}

pub struct ShellProcess;

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

## Семантика `try_read()`

`try_read()` не ждет появления новых данных бесконечно.

Реализация библиотеки запускает фоновый поток чтения из PTY. Этот поток складывает уже полученные байты во внутренний буфер. Вызов `try_read()` просто забирает и возвращает то, что уже накоплено на момент вызова.

Из этого следуют два правила:

- если вывод уже есть, `try_read()` вернет его немедленно;
- если нового вывода пока нет, `try_read()` вернет пустой `Vec<u8>`.

## Завершение процесса

`terminate()` делает best-effort завершение не только корневой оболочки, но и ее дочерних процессов.

Текущая Linux-реализация делает две вещи:

- пытается послать сигнал группе процессов оболочки;
- дополнительно проходит по дереву потомков через `/proc` и завершает найденные дочерние процессы.

Текущая macOS-реализация проще:

- пытается завершить группу процессов оболочки через сигналы;
- не опирается на Linux-специфичное дерево потомков через `/proc`.

Такое же best-effort завершение выполняется и при `Drop` объекта `ShellProcess`, чтобы фоновые задачи не переживали закрытие панели только из-за неявочного освобождения ресурсов.

## Определение текущего каталога

Публичный метод `current_dir()` не меняется для вызывающего кода, но внутри библиотеки определение каталога вынесено в отдельную внутреннюю стратегию.

Сейчас поддерживаются такие варианты:

- Linux: чтение символической ссылки `/proc/<pid>/cwd`;
- macOS: чтение каталога процесса через системный интерфейс `proc_pidinfo(...)`;
- остальные платформы: явная ошибка `ProcessError::CurrentDir(...)` с сообщением, что определение `cwd` не поддерживается.

Linux-стратегия определяет текущий рабочий каталог по идентификатору процесса оболочки через путь:

```text
/proc/<pid>/cwd
```

Библиотека читает символическую ссылку по этому пути и возвращает полученный каталог как `PathBuf`.

На macOS библиотека получает тот же каталог через системный вызов `proc_pidinfo(...)` с вариантом `PROC_PIDVNODEPATHINFO`.

Это важно, потому что на macOS временные каталоги часто могут иметь псевдонимный путь вида `/var/...`, а системный ответ возвращает канонический путь вида `/private/var/...`.

Если позже понадобится поддержка другой платформы, нужно добавить новую внутреннюю стратегию определения `cwd`, не меняя сигнатуру `ShellProcess::current_dir()`.
