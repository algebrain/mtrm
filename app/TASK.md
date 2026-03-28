# `mtrm` main

## Что это

Главный исполняемый пакет рабочего пространства.

Разработчик этого пакета не пишет низкоуровневую логику библиотек заново. Его задача: собрать их в одну работающую программу с главным циклом.

## Публичный интерфейс, который нужно реализовать

Пакет должен содержать следующие функции:

```rust
use mtrm_clipboard::ClipboardBackend;
use mtrm_process::ShellProcessConfig;
use mtrm_tabs::TabManager;

pub struct App {
    // Внутреннее устройство выбирается исполнителем.
}

impl App {
    pub fn new(shell: ShellProcessConfig) -> Result<Self, AppError>;
    pub fn restore_or_new(shell: ShellProcessConfig) -> Result<Self, AppError>;
    pub fn handle_key_event(
        &mut self,
        event: crossterm::event::KeyEvent,
        clipboard: &mut dyn ClipboardBackend,
    ) -> Result<(), AppError>;
    pub fn redraw<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut ratatui::Terminal<B>,
    ) -> Result<(), AppError>;
    pub fn save(&mut self) -> Result<(), AppError>;
    pub fn run<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut ratatui::Terminal<B>,
        clipboard: &mut dyn ClipboardBackend,
    ) -> Result<(), AppError>;
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("state error: {0}")]
    State(String),
    #[error("tabs error: {0}")]
    Tabs(String),
    #[error("clipboard error: {0}")]
    Clipboard(String),
    #[error("terminal io error: {0}")]
    TerminalIo(String),
}

fn main() -> Result<(), AppError>;
```

Точное правило работы:

- `restore_or_new` пытается загрузить состояние с диска;
- если файла состояния нет, создается новое приложение с одной вкладкой и одним окном;
- `handle_key_event` использует keymap-aware преобразование ввода из `mtrm-input`;
- команды копирования и вставки идут через `ClipboardBackend`;
- `Ctrl+C` копирует только текущее выделение, а не весь текст панели;
- обычный ввод передается в активный псевдотерминал;
- `Esc`-prefixed последовательности для `Alt+<буква>` при необходимости синтезируются обратно в один `KeyEvent`;
- после изменения вкладок, раскладки или рабочих каталогов вызывается `save`;
- `redraw` строит данные для `mtrm-ui::render_frame`;
- `run` содержит главный цикл чтения ввода, обновления данных и перерисовки;
- при потере фокуса внешнего окна приложение обновляет UI, чтобы активные элементы подсвечивались красным.

## Допустимые зависимости

- все локальные библиотеки рабочего пространства;
- `thiserror`;
- `crossterm`;
- `ratatui`;
- стандартная библиотека Rust.

## Что покрыть тестами

- `restore_or_new` создает новое состояние при отсутствии файла;
- `restore_or_new` восстанавливает состояние при наличии файла;
- `handle_key_event` на `Ctrl+V` читает текст из буфера и отправляет его в активное окно;
- `handle_key_event` на обычный символ отправляет байты в активное окно;
- `handle_key_event` на `Alt+X` отправляет прерывание;
- `save` реально сохраняет состояние;
- `redraw` не падает на минимальном состоянии;
- сценарный тест: создать приложение, разбить окно, сохранить, восстановить, убедиться что раскладка и каталоги вернулись.

Для тестов разрешено ввести внутренние точки расширения, чтобы подставлять тестовый буфер обмена и тестовую реализацию управления вкладками.

## Какая документация нужна после тестов

После прохождения тестов создать корневой `README.md` с:

- точным описанием жизненного цикла приложения;
- списком горячих клавиш;
- описанием автосоздания `~/.mtrm`;
- сценарием восстановления состояния после перезапуска.
