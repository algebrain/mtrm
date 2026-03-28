# PORTABILITY_NOTES

Этот документ фиксирует места в проекте, где текущая реализация опирается на особенности конкретной платформы и позже должна быть переработана для более полной переносимости.

## 2026-03-26

### Жизненный цикл процессов

Текущее исправление явного завершения процессов в `crates/process` не является переносимым.

Сейчас оно опирается на Linux-специфичные и Unix-специфичные механизмы:

- группы процессов и сигналы (`SIGHUP`, `SIGKILL`);
- чтение дерева потомков через `/proc/<pid>/task/<pid>/children`.

Это приемлемо как практическое решение для текущего окружения разработки, но не является завершенной переносимой архитектурой.

Позже это место нужно будет переработать в более общий платформенный слой управления процессами.

### Определение текущего каталога

Текущее определение `cwd` в `crates/process` тоже реализовано только для Linux.

Сейчас поддерживаемый путь один:

- чтение `/proc/<pid>/cwd`.

Для остальных платформ код уже возвращает аккуратную ошибку о неподдерживаемом способе определения `cwd`, но полноценной альтернативной реализации пока нет.

Архитектурно это место уже локализовано во внутреннем слое выбора стратегии, поэтому позже сюда можно добавить другой платформенный способ без изменения публичного метода `ShellProcess::current_dir()`.

## 2026-03-27

### Windows build failure in `crates/process`

Текущий провал сборки под Windows подтверждает, что `crates/process` всё ещё собран как Unix-first слой, а не как платформенно-разделённая реализация.

Первичные ошибки:

- `use nix::sys::signal::{self, Signal};`
- `use nix::unistd::{Pid, getpgid};`

На Windows эти модули `nix` недоступны, поэтому код не компилируется уже на этапе импорта.

Это не случайная несовместимость отдельной функции, а прямое проявление того, что жизненный цикл дочерних процессов сейчас выражен через Unix-концепции:

- PID/PGID;
- сигналы (`SIGINT`, `SIGHUP`, `SIGKILL`);
- получение process group через `getpgid(...)`;
- отправка сигнала всей группе через отрицательный PID.

Конкретные места:

- импорты Unix-only API в [../../crates/process/src/lib.rs](../../crates/process/src/lib.rs)
- вычисление `process_group_id` через `getpgid(...)` в [../../crates/process/src/lib.rs](../../crates/process/src/lib.rs)
- `send_interrupt()` через `signal::kill(..., SIGINT)` в [../../crates/process/src/lib.rs](../../crates/process/src/lib.rs)
- завершение дерева процессов через `SIGHUP`/`SIGKILL` в [../../crates/process/src/lib.rs](../../crates/process/src/lib.rs)
- отправка сигналов в process group в [../../crates/process/src/lib.rs](../../crates/process/src/lib.rs)

### Secondary compiler errors

Ошибки `E0282` с `type annotations needed` в `map_err(...)` выглядят вторичными и, вероятно, являются следствием того, что из-за платформенно-недоступных частей компилятор теряет нормальный вывод типов в соседних выражениях.

То есть эти `E0282` не стоит считать самостоятельной корневой проблемой. С высокой вероятностью они исчезнут или изменятся после нормального разделения Unix/Windows-веток.

### Practical conclusion

Текущее состояние `crates/process` переносимо только частично:

- базовая работа через `portable-pty` потенциально кроссплатформенна;
- управление сигналами, process groups и разбор `/proc` остаются Unix/Linux-специфичными.

Значит, для Windows здесь нужен не точечный фикс одного импорта, а явный платформенный слой хотя бы в трёх областях:

- отправка interrupt активному процессу;
- завершение процесса и его потомков;
- определение текущего каталога дочернего shell.

До такой декомпозиции Windows-сборка `crates/process` будет оставаться хрупкой или вовсе некомпилируемой.

### macOS test failures caused by Linux-only cwd snapshotting

Провалы тестов на macOS показывают другую грань той же проблемы: код уже компилируется, но многие операции в `app` и `tabs` неявно требуют, чтобы `ShellProcess::current_dir()` умел работать на текущей платформе.

Сейчас это не так:

- на Linux `current_dir()` использует `/proc/<pid>/cwd`
- на остальных платформах `platform_cwd_resolution_strategy()` возвращает `Unsupported`
- это превращается в `ProcessError::CurrentDir("cwd resolution unsupported on this platform")`

Конкретные места:

- выбор стратегии в [../../crates/process/src/lib.rs](../../crates/process/src/lib.rs)
- возврат ошибки для неподдерживаемой платформы в [../../crates/process/src/lib.rs](../../crates/process/src/lib.rs)
- Linux-only реализация через `/proc/<pid>/cwd` в [../../crates/process/src/lib.rs](../../crates/process/src/lib.rs)

На macOS эта неподдерживаемая ветка начинает ломать не только прямые тесты на `cwd`, но и широкий набор seemingly unrelated тестов. Причина в том, что snapshot tab/pane состояния всегда пытается прочитать живой `cwd` каждого процесса:

- [../../crates/tabs/src/lib.rs](../../crates/tabs/src/lib.rs)
- [../../crates/tabs/src/lib.rs](../../crates/tabs/src/lib.rs)

А `snapshot()` вызывается во многих обычных пользовательских путях:

- `App::save()` в [../../app/src/main.rs](../../app/src/main.rs)
- обработка paste, layout-команд, tab-команд, quit и explicit save в [../../app/src/main.rs](../../app/src/main.rs)

Из-за этого на macOS падают тесты, которые внешне проверяют совсем не `cwd`:

- split pane
- создание вкладки
- paste
- redraw
- mouse focus
- quit/save
- snapshot/restore сценарии

То есть корневая причина не в конкретных тестах, а в архитектурной связке:

- операции интерфейса вызывают `save()`
- `save()` вызывает `tabs.snapshot()`
- `tabs.snapshot()` требует live `cwd` от каждого shell
- `current_dir()` за пределами Linux сейчас не реализован

### Practical conclusion for macOS

С точки зрения переносимости это означает:

- текущая persistence-модель слишком жёстко завязана на live cwd introspection процесса
- Linux-only способ определения cwd уже влияет на большую часть app-level поведения
- до появления альтернативной стратегии для macOS или более мягкого fallback поведение на macOS будет ломаться не локально, а каскадно

Иными словами, проблема на macOS сейчас не в UI и не в тестах как таковых, а в том, что snapshot persistence зависит от платформенно-непереносимой функции `current_dir()`.

## 2026-03-28

### Alt+X recovery path remains Unix/Linux-first

Финальный стабильный фикс `Alt+X` подтвердил, что путь прерывания foreground job и post-interrupt recovery в `crates/process` сейчас по сути остается Unix-first, а отдельные части уже прямо Linux-specific.

Сейчас этот путь опирается на:

- process groups и Unix signals (`SIGINT`, `SIGHUP`, `SIGCONT`, `SIGTERM`);
- определение foreground process group через PTY/process-group semantics;
- восстановление baseline `termios` после interrupt;
- Linux-only чтение `/proc` для поиска lingering same-TTY процессов;
- Linux-only доступ к shell tty через `/proc/<pid>/fd/0`.

Практически это означает, что текущий рабочий `Alt+X`-фикс состоит не только из "послать SIGINT", а из нескольких платформенно-зависимых шагов:

- доставить interrupt в foreground job;
- дождаться возврата shell в foreground;
- при необходимости восстановить baseline `termios`;
- дополнительно применить baseline через shell tty;
- очистить lingering same-TTY процессы из interrupted group.

Конкретные места:

- foreground-aware interrupt delivery в [../../crates/process/src/lib.rs](../../crates/process/src/lib.rs)
- shell-tty restore path в [../../crates/process/src/lib.rs](../../crates/process/src/lib.rs)
- lingering same-TTY cleanup в [../../crates/process/src/lib.rs](../../crates/process/src/lib.rs)
- Linux-only `/proc` introspection helper-ы в [../../crates/process/src/lib.rs](../../crates/process/src/lib.rs)

### Practical conclusion after the Alt+X fix

С точки зрения переносимости это усиливает предыдущий вывод:

- `portable-pty` дает базовый кроссплатформенный PTY слой;
- но надежный interrupt/recovery path в текущем виде уже выражен через Unix job control и Linux `/proc`.

Значит, для реальной переносимости сюда позже нужен не один `cfg` вокруг отдельного сигнала, а платформенный слой как минимум для:

- доставки interrupt активной foreground job;
- post-interrupt terminal recovery;
- поиска и cleanup lingering same-TTY процессов;
- доступа к shell-owned tty вне Linux `/proc`.
