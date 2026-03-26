//! Экранное состояние одной терминальной панели.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenCell {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenLine {
    pub cells: Vec<ScreenCell>,
}

pub struct TerminalScreen {
    parser: vt100::Parser,
}

impl TerminalScreen {
    pub fn new(rows: u16, cols: u16, scrollback_len: usize) -> Self {
        Self {
            parser: vt100::Parser::new(rows.max(1), cols.max(1), scrollback_len),
        }
    }

    pub fn process_bytes(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.screen_mut().set_size(rows.max(1), cols.max(1));
    }

    pub fn size(&self) -> (u16, u16) {
        self.parser.screen().size()
    }

    pub fn cursor_position(&self) -> (u16, u16) {
        self.parser.screen().cursor_position()
    }

    pub fn text_contents(&self) -> String {
        self.parser.screen().contents()
    }

    pub fn visible_rows(&self) -> Vec<String> {
        let (_, cols) = self.size();
        self.parser.screen().rows(0, cols).collect()
    }

    pub fn visible_lines(&self) -> Vec<ScreenLine> {
        let (rows, cols) = self.size();
        let screen = self.parser.screen();
        let mut lines = Vec::with_capacity(rows as usize);

        for row in 0..rows {
            let mut cells = Vec::with_capacity(cols as usize);
            for col in 0..cols {
                let cell = screen.cell(row, col).expect("cell in visible bounds");
                cells.push(ScreenCell {
                    text: cell.contents().to_owned(),
                    bold: cell.bold(),
                    italic: cell.italic(),
                    underline: cell.underline(),
                    inverse: cell.inverse(),
                });
            }
            lines.push(ScreenLine { cells });
        }

        lines
    }

    pub fn scrollback(&self) -> usize {
        self.parser.screen().scrollback()
    }

    pub fn set_scrollback(&mut self, rows: usize) {
        self.parser.screen_mut().set_scrollback(rows);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_row_text(screen: &TerminalScreen) -> String {
        screen.visible_rows().into_iter().next().unwrap_or_default()
    }

    #[test]
    fn prompt_bytes_appear_in_visible_rows() {
        let mut screen = TerminalScreen::new(24, 80, 0);

        screen.process_bytes(b"user@mint:~$ ");

        assert!(first_row_text(&screen).contains("user@mint:~$ "));
    }

    #[test]
    fn carriage_return_and_clear_line_replace_previous_text() {
        let mut screen = TerminalScreen::new(24, 80, 0);

        screen.process_bytes(b"abcdef");
        screen.process_bytes(b"\r\x1b[2Kxy");

        let first_row = first_row_text(&screen);
        assert!(first_row.starts_with("xy"));
        assert!(!first_row.contains("abcdef"));
    }

    #[test]
    fn resize_keeps_screen_operational() {
        let mut screen = TerminalScreen::new(10, 20, 0);
        screen.process_bytes(b"prompt$ ");

        screen.resize(30, 90);
        screen.process_bytes(b"echo test");

        assert_eq!(screen.size(), (30, 90));
        assert!(screen.text_contents().contains("prompt$ "));
        assert!(screen.text_contents().contains("echo test"));
    }

    #[test]
    fn visible_lines_expose_basic_cell_attributes() {
        let mut screen = TerminalScreen::new(5, 20, 0);

        screen.process_bytes(b"\x1b[1mA\x1b[3mB\x1b[4mC\x1b[7mD\x1b[m");

        let first_line = screen.visible_lines().remove(0);
        assert_eq!(first_line.cells[0].text, "A");
        assert!(first_line.cells[0].bold);
        assert_eq!(first_line.cells[1].text, "B");
        assert!(first_line.cells[1].italic);
        assert_eq!(first_line.cells[2].text, "C");
        assert!(first_line.cells[2].underline);
        assert_eq!(first_line.cells[3].text, "D");
        assert!(first_line.cells[3].inverse);
    }

    #[test]
    fn scrollback_changes_visible_rows() {
        let mut screen = TerminalScreen::new(3, 20, 20);

        screen.process_bytes(b"line1\nline2\nline3\nline4\nline5\n");
        let bottom = screen.visible_rows();
        screen.set_scrollback(2);
        let scrolled = screen.visible_rows();

        assert_ne!(bottom, scrolled);
        assert_eq!(screen.scrollback(), 2);
    }
}
