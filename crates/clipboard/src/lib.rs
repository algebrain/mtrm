//! Работа с системным буфером обмена.

use arboard::Clipboard;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClipboardError {
    #[error("failed to read from system clipboard: {0}")]
    Read(String),
    #[error("failed to write to system clipboard: {0}")]
    Write(String),
    #[error("system clipboard is unavailable")]
    Unavailable,
}

pub trait ClipboardBackend: Send {
    fn get_text(&mut self) -> Result<String, ClipboardError>;
    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError>;
}

pub struct SystemClipboard {
    clipboard: Clipboard,
}

impl SystemClipboard {
    pub fn new() -> Result<Self, ClipboardError> {
        let clipboard =
            Clipboard::new().map_err(|error| ClipboardError::Read(error.to_string()))?;
        Ok(Self { clipboard })
    }
}

impl ClipboardBackend for SystemClipboard {
    fn get_text(&mut self) -> Result<String, ClipboardError> {
        self.clipboard
            .get_text()
            .map_err(|error| ClipboardError::Read(error.to_string()))
    }

    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.clipboard
            .set_text(text.to_owned())
            .map_err(|error| ClipboardError::Write(error.to_string()))
    }
}

#[derive(Debug, Default)]
pub struct UnavailableClipboard;

impl ClipboardBackend for UnavailableClipboard {
    fn get_text(&mut self) -> Result<String, ClipboardError> {
        Err(ClipboardError::Unavailable)
    }

    fn set_text(&mut self, _text: &str) -> Result<(), ClipboardError> {
        Err(ClipboardError::Unavailable)
    }
}

#[derive(Debug, Default)]
pub struct MemoryClipboard {
    text: String,
}

impl MemoryClipboard {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ClipboardBackend for MemoryClipboard {
    fn get_text(&mut self) -> Result<String, ClipboardError> {
        Ok(self.text.clone())
    }

    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.text.clear();
        self.text.push_str(text);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_clipboard_starts_with_consistent_empty_state() {
        let mut clipboard = MemoryClipboard::new();
        assert_eq!(clipboard.get_text().unwrap(), "");
    }

    #[test]
    fn memory_clipboard_set_text_stores_string() {
        let mut clipboard = MemoryClipboard::new();

        clipboard.set_text("hello").unwrap();

        assert_eq!(clipboard.get_text().unwrap(), "hello");
    }

    #[test]
    fn memory_clipboard_returns_last_written_value() {
        let mut clipboard = MemoryClipboard::new();

        clipboard.set_text("first").unwrap();
        clipboard.set_text("second").unwrap();

        assert_eq!(clipboard.get_text().unwrap(), "second");
    }

    #[test]
    fn memory_clipboard_preserves_multiline_text() {
        let mut clipboard = MemoryClipboard::new();
        let text = "one\ntwo\nthree";

        clipboard.set_text(text).unwrap();

        assert_eq!(clipboard.get_text().unwrap(), text);
    }

    #[test]
    fn memory_clipboard_accepts_empty_string() {
        let mut clipboard = MemoryClipboard::new();

        clipboard.set_text("").unwrap();

        assert_eq!(clipboard.get_text().unwrap(), "");
    }

    #[test]
    fn unavailable_clipboard_reports_unavailable_for_read_and_write() {
        let mut clipboard = UnavailableClipboard;

        assert!(matches!(
            clipboard.get_text(),
            Err(ClipboardError::Unavailable)
        ));
        assert!(matches!(
            clipboard.set_text("hello"),
            Err(ClipboardError::Unavailable)
        ));
    }

    #[test]
    fn system_error_conversion_preserves_operation_kind() {
        let read_error = ClipboardError::Read("read failed".to_owned());
        let write_error = ClipboardError::Write("write failed".to_owned());

        assert_eq!(
            read_error.to_string(),
            "failed to read from system clipboard: read failed"
        );
        assert_eq!(
            write_error.to_string(),
            "failed to write to system clipboard: write failed"
        );
    }
}
