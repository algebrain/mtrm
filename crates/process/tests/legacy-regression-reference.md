# Legacy Regression Reference

До переписывания `crates/process` на `prux` старый крейт содержал более широкий набор PTY/termios-oriented regression tests.

Эти сценарии не удаляются концептуально, но больше не считаются прямым контрактом новой реализации автоматически. При необходимости их можно постепенно возвращать как:

- новые integration tests поверх `prux` или `mtrm-process`;
- специальные regression tests для реальных проблемных сценариев;
- отдельный compatibility suite, если будет решено сохранить старые гарантии.

Базовый источник прежних сценариев:

- история файла `crates/process/src/lib.rs` в ветке до переписывания

Особенно значимые старые сценарии:

- recovery после `SIGINT`, если foreground job оставляет TTY в raw mode;
- cleanup orphaned same-tty processes после interrupted process group;
- baseline/restore termios around shell prompt state;
- bounded read buffer;
- sanitized `ProcessError` display with detailed `Debug`.
