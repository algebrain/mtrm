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

- импорты Unix-only API в [crates/process/src/lib.rs](/home/algebrain/src/my/mterm/crates/process/src/lib.rs:14)
- вычисление `process_group_id` через `getpgid(...)` в [crates/process/src/lib.rs](/home/algebrain/src/my/mterm/crates/process/src/lib.rs:88)
- `send_interrupt()` через `signal::kill(..., SIGINT)` в [crates/process/src/lib.rs](/home/algebrain/src/my/mterm/crates/process/src/lib.rs:133)
- завершение дерева процессов через `SIGHUP`/`SIGKILL` в [crates/process/src/lib.rs](/home/algebrain/src/my/mterm/crates/process/src/lib.rs:164)
- отправка сигналов в process group в [crates/process/src/lib.rs](/home/algebrain/src/my/mterm/crates/process/src/lib.rs:179)

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

- выбор стратегии в [crates/process/src/lib.rs](/home/algebrain/src/my/mterm/crates/process/src/lib.rs:191)
- возврат ошибки для неподдерживаемой платформы в [crates/process/src/lib.rs](/home/algebrain/src/my/mterm/crates/process/src/lib.rs:209)
- Linux-only реализация через `/proc/<pid>/cwd` в [crates/process/src/lib.rs](/home/algebrain/src/my/mterm/crates/process/src/lib.rs:215)

На macOS эта неподдерживаемая ветка начинает ломать не только прямые тесты на `cwd`, но и широкий набор seemingly unrelated тестов. Причина в том, что snapshot tab/pane состояния всегда пытается прочитать живой `cwd` каждого процесса:

- [crates/tabs/src/lib.rs](/home/algebrain/src/my/mterm/crates/tabs/src/lib.rs:343)
- [crates/tabs/src/lib.rs](/home/algebrain/src/my/mterm/crates/tabs/src/lib.rs:355)

А `snapshot()` вызывается во многих обычных пользовательских путях:

- `App::save()` в [app/src/main.rs](/home/algebrain/src/my/mterm/app/src/main.rs:183)
- обработка paste, layout-команд, tab-команд, quit и explicit save в [app/src/main.rs](/home/algebrain/src/my/mterm/app/src/main.rs:226)

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
