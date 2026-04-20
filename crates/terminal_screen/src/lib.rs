//! Экранное состояние одной терминальной панели.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScreenMode {
    #[default]
    Normal,
    Alternate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScreenColor {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenCell {
    pub text: String,
    pub has_contents: bool,
    pub is_wide: bool,
    pub is_wide_continuation: bool,
    pub fg: ScreenColor,
    pub bg: ScreenColor,
    pub dim: bool,
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
    normal_history_limit: usize,
    normal_history: Vec<Vec<ScreenLine>>,
    normal_scrollback: usize,
    normal_dirty_since_snapshot: bool,
    normal_capture_phase: NormalCapturePhase,
    custom_scroll_region_active: bool,
    alt_history_limit: usize,
    alt_history: Vec<Vec<ScreenLine>>,
    alt_scrollback: usize,
    alt_dirty_since_snapshot: bool,
    screen_mode: ScreenMode,
    escape_state: EscapeSequenceState,
}

impl TerminalScreen {
    pub fn new(rows: u16, cols: u16, scrollback_len: usize) -> Self {
        Self {
            parser: vt100::Parser::new(rows.max(1), cols.max(1), scrollback_len),
            normal_history_limit: scrollback_len,
            normal_history: Vec::new(),
            normal_scrollback: 0,
            normal_dirty_since_snapshot: false,
            normal_capture_phase: NormalCapturePhase::Inactive,
            custom_scroll_region_active: false,
            alt_history_limit: scrollback_len,
            alt_history: Vec::new(),
            alt_scrollback: 0,
            alt_dirty_since_snapshot: false,
            screen_mode: ScreenMode::Normal,
            escape_state: EscapeSequenceState::Ground,
        }
    }

    pub fn process_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.screen_mode == ScreenMode::Normal
                && !self.custom_scroll_region_active
                && self.normal_capture_phase == NormalCapturePhase::PostRegion
                && byte == 0x1b
                && self.normal_dirty_since_snapshot
            {
                self.capture_normal_snapshot();
                self.normal_dirty_since_snapshot = false;
                self.normal_capture_phase = NormalCapturePhase::Inactive;
            }

            if self.screen_mode == ScreenMode::Alternate
                && byte == 0x1b
                && self.alt_dirty_since_snapshot
            {
                self.capture_alternate_snapshot();
                self.alt_dirty_since_snapshot = false;
            }

            let previous_mode = self.screen_mode;
            let rows = self.size().0;
            self.track_escape_state_byte(byte, rows);
            self.parser.process(&[byte]);

            if previous_mode != ScreenMode::Alternate && self.screen_mode == ScreenMode::Alternate {
                self.normal_history.clear();
                self.normal_scrollback = 0;
                self.normal_dirty_since_snapshot = false;
                self.normal_capture_phase = NormalCapturePhase::Inactive;
                self.custom_scroll_region_active = false;
                self.alt_history.clear();
                self.alt_scrollback = 0;
                self.alt_dirty_since_snapshot = false;
            }

            if self.screen_mode == ScreenMode::Alternate {
                if byte_affects_alternate_frame(byte) {
                    self.alt_dirty_since_snapshot = true;
                }
            } else {
                self.alt_scrollback = 0;
                self.alt_dirty_since_snapshot = false;

                if self.custom_scroll_region_active {
                    if byte_affects_normal_snapshot(byte) {
                        self.normal_dirty_since_snapshot = true;
                        self.normal_capture_phase = NormalCapturePhase::InRegion;
                    }
                } else if self.normal_capture_phase != NormalCapturePhase::Inactive {
                    if byte_affects_normal_snapshot(byte) {
                        self.normal_dirty_since_snapshot = true;
                        self.normal_capture_phase = NormalCapturePhase::PostRegion;
                    }
                }
            }
        }

        if self.screen_mode == ScreenMode::Normal
            && self.normal_capture_phase == NormalCapturePhase::PostRegion
            && self.normal_dirty_since_snapshot
        {
            self.capture_normal_snapshot();
            self.normal_dirty_since_snapshot = false;
            self.normal_capture_phase = NormalCapturePhase::Inactive;
        }

        if self.screen_mode == ScreenMode::Alternate && self.alt_dirty_since_snapshot {
            self.capture_alternate_snapshot();
            self.alt_dirty_since_snapshot = false;
        }
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.screen_mut().set_size(rows.max(1), cols.max(1));
        self.custom_scroll_region_active = false;
        if self.screen_mode == ScreenMode::Normal
            && self.normal_capture_phase != NormalCapturePhase::Inactive
        {
            self.capture_normal_snapshot();
            self.normal_capture_phase = NormalCapturePhase::Inactive;
        }
        if self.screen_mode == ScreenMode::Alternate {
            self.capture_alternate_snapshot();
        }
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
        if let Some(snapshot) = self.normal_history_snapshot() {
            return snapshot_rows(&snapshot);
        }

        if let Some(snapshot) = self.alternate_history_snapshot() {
            return snapshot_rows(&snapshot);
        }

        let (_, cols) = self.size();
        self.parser.screen().rows(0, cols).collect()
    }

    pub fn visible_lines(&self) -> Vec<ScreenLine> {
        if let Some(snapshot) = self.normal_history_snapshot() {
            return snapshot;
        }

        if let Some(snapshot) = self.alternate_history_snapshot() {
            return snapshot;
        }

        self.current_screen_lines()
    }

    pub fn shows_history_snapshot(&self) -> bool {
        self.normal_history_snapshot().is_some() || self.alternate_history_snapshot().is_some()
    }

    pub fn scrollback(&self) -> usize {
        match self.screen_mode {
            ScreenMode::Normal => {
                if !self.normal_history.is_empty() {
                    self.normal_scrollback
                } else {
                    self.parser.screen().scrollback()
                }
            }
            ScreenMode::Alternate => self.alt_scrollback,
        }
    }

    pub fn set_scrollback(&mut self, rows: usize) {
        match self.screen_mode {
            ScreenMode::Normal => {
                if !self.normal_history.is_empty() {
                    let max_offset = self.normal_history.len().saturating_sub(1);
                    self.normal_scrollback = rows.min(max_offset);
                    self.parser.screen_mut().set_scrollback(0);
                } else {
                    self.parser.screen_mut().set_scrollback(rows);
                }
            }
            ScreenMode::Alternate => {
                let max_offset = self.alt_history.len().saturating_sub(1);
                self.alt_scrollback = rows.min(max_offset);
            }
        }
    }

    pub fn screen_mode(&self) -> ScreenMode {
        self.screen_mode
    }

    pub fn is_alternate_screen(&self) -> bool {
        self.screen_mode == ScreenMode::Alternate
    }

    fn current_screen_lines(&self) -> Vec<ScreenLine> {
        let (rows, cols) = self.size();
        let screen = self.parser.screen();
        let mut lines = Vec::with_capacity(rows as usize);

        for row in 0..rows {
            let mut cells = Vec::with_capacity(cols as usize);
            for col in 0..cols {
                let cell = screen.cell(row, col).expect("cell in visible bounds");
                cells.push(ScreenCell {
                    text: cell.contents().to_owned(),
                    has_contents: cell.has_contents(),
                    is_wide: cell.is_wide(),
                    is_wide_continuation: cell.is_wide_continuation(),
                    fg: screen_color(cell.fgcolor()),
                    bg: screen_color(cell.bgcolor()),
                    dim: cell.dim(),
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

    fn alternate_history_snapshot(&self) -> Option<Vec<ScreenLine>> {
        if self.screen_mode != ScreenMode::Alternate || self.alt_scrollback == 0 {
            return None;
        }

        self.alt_history
            .len()
            .checked_sub(self.alt_scrollback + 1)
            .and_then(|index| self.alt_history.get(index).cloned())
    }

    fn normal_history_snapshot(&self) -> Option<Vec<ScreenLine>> {
        if self.screen_mode != ScreenMode::Normal || self.normal_scrollback == 0 {
            return None;
        }

        self.normal_history
            .len()
            .checked_sub(self.normal_scrollback + 1)
            .and_then(|index| self.normal_history.get(index).cloned())
    }

    fn capture_normal_snapshot(&mut self) {
        if self.normal_history_limit == 0 {
            return;
        }

        let snapshot = self.current_screen_lines();
        if snapshot_is_blank(&snapshot) {
            return;
        }
        if self.normal_history.last() == Some(&snapshot) {
            return;
        }

        self.normal_history.push(snapshot);
        if self.normal_history.len() > self.normal_history_limit {
            let overflow = self.normal_history.len() - self.normal_history_limit;
            self.normal_history.drain(0..overflow);
        }
        self.normal_scrollback = self
            .normal_scrollback
            .min(self.normal_history.len().saturating_sub(1));
    }

    fn capture_alternate_snapshot(&mut self) {
        if self.alt_history_limit == 0 {
            return;
        }

        let snapshot = self.current_screen_lines();
        if snapshot_is_blank(&snapshot) {
            return;
        }
        if self.alt_history.last() == Some(&snapshot) {
            return;
        }

        self.alt_history.push(snapshot);
        if self.alt_history.len() > self.alt_history_limit {
            let overflow = self.alt_history.len() - self.alt_history_limit;
            self.alt_history.drain(0..overflow);
        }
        self.alt_scrollback = self.alt_scrollback.min(self.alt_history.len().saturating_sub(1));
    }

    fn track_escape_state_byte(&mut self, byte: u8, rows: u16) {
        match &mut self.escape_state {
            EscapeSequenceState::Ground => {
                if byte == 0x1b {
                    self.escape_state = EscapeSequenceState::Escape;
                }
            }
            EscapeSequenceState::Escape => {
                if byte == b'[' {
                    self.escape_state = EscapeSequenceState::Csi(Vec::new());
                } else if byte == 0x1b {
                    self.escape_state = EscapeSequenceState::Escape;
                } else {
                    self.escape_state = EscapeSequenceState::Ground;
                }
            }
            EscapeSequenceState::Csi(params) => {
                if is_csi_final_byte(byte) {
                    update_screen_state_from_csi(
                        &mut self.screen_mode,
                        &mut self.custom_scroll_region_active,
                        rows,
                        params,
                        byte,
                    );
                    self.escape_state = EscapeSequenceState::Ground;
                } else if byte == 0x1b {
                    self.escape_state = EscapeSequenceState::Escape;
                } else {
                    params.push(byte);
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EscapeSequenceState {
    Ground,
    Escape,
    Csi(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum NormalCapturePhase {
    #[default]
    Inactive,
    InRegion,
    PostRegion,
}

fn is_csi_final_byte(byte: u8) -> bool {
    (0x40..=0x7e).contains(&byte)
}

fn update_screen_state_from_csi(
    screen_mode: &mut ScreenMode,
    custom_scroll_region_active: &mut bool,
    rows: u16,
    params: &[u8],
    final_byte: u8,
) {
    match final_byte {
        b'h' | b'l' => {
            let Some(rest) = params.strip_prefix(b"?") else {
                return;
            };

            let enable = final_byte == b'h';
            for param in rest.split(|byte| *byte == b';') {
                if matches!(param, b"47" | b"1047" | b"1049") {
                    *screen_mode = if enable {
                        ScreenMode::Alternate
                    } else {
                        ScreenMode::Normal
                    };
                    return;
                }
            }
        }
        b'r' => {
            *custom_scroll_region_active = is_custom_scroll_region(params, rows);
        }
        _ => {}
    }
}

fn byte_affects_alternate_frame(byte: u8) -> bool {
    matches!(byte, 0x08 | b'\t' | b'\n' | 0x0d) || (0x20..=0x7e).contains(&byte) || byte >= 0x80
}

fn byte_affects_normal_snapshot(byte: u8) -> bool {
    byte_affects_alternate_frame(byte)
}

fn is_custom_scroll_region(params: &[u8], rows: u16) -> bool {
    if params.is_empty() {
        return false;
    }

    let mut parts = params.split(|byte| *byte == b';');
    let top = parse_csi_param(parts.next()).unwrap_or(1);
    let bottom = parse_csi_param(parts.next()).unwrap_or(rows);

    !(top == 1 && bottom == rows)
}

fn parse_csi_param(param: Option<&[u8]>) -> Option<u16> {
    let param = param?;
    if param.is_empty() {
        return None;
    }

    std::str::from_utf8(param).ok()?.parse().ok()
}

fn screen_color(color: vt100::Color) -> ScreenColor {
    match color {
        vt100::Color::Default => ScreenColor::Default,
        vt100::Color::Idx(index) => ScreenColor::Indexed(index),
        vt100::Color::Rgb(r, g, b) => ScreenColor::Rgb(r, g, b),
    }
}

fn snapshot_rows(lines: &[ScreenLine]) -> Vec<String> {
    lines.iter()
        .map(|line| {
            line.cells
                .iter()
                .map(|cell| {
                    if cell.has_contents {
                        cell.text.clone()
                    } else {
                        " ".to_owned()
                    }
                })
                .collect()
        })
        .collect()
}

fn snapshot_is_blank(lines: &[ScreenLine]) -> bool {
    !lines
        .iter()
        .flat_map(|line| line.cells.iter())
        .any(|cell| cell.has_contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alt_screen_bytes(payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\x1b[?1049h");
        bytes.extend_from_slice(payload);
        bytes.extend_from_slice(b"\x1b[?1049l");
        bytes
    }

    fn decstbm_frame_bytes(frame_label: &str, footer_label: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\x1b[2J\x1b[H");
        bytes.extend_from_slice(b"hist1\r\nhist2\r\nhist3\r\nhist4\r\n");
        bytes.extend_from_slice(footer_label.as_bytes());
        bytes.extend_from_slice(b"\x1b[1;4r");
        bytes.extend_from_slice(b"\x1b[4;1H\r\n");
        bytes.extend_from_slice(frame_label.as_bytes());
        bytes.extend_from_slice(b"\x1b[r");
        bytes.extend_from_slice(b"\x1b[6;1H");
        bytes.extend_from_slice(footer_label.as_bytes());
        bytes
    }

    fn bytes_with_gap(left: &str, gap_cols: u16, right: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(left.as_bytes());
        bytes.extend_from_slice(format!("\x1b[{}C", gap_cols).as_bytes());
        bytes.extend_from_slice(right.as_bytes());
        bytes
    }

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
        assert!(first_line.cells[0].has_contents);
        assert!(!first_line.cells[0].is_wide);
        assert!(!first_line.cells[0].is_wide_continuation);
        assert_eq!(first_line.cells[0].fg, ScreenColor::Default);
        assert_eq!(first_line.cells[0].bg, ScreenColor::Default);
        assert!(!first_line.cells[0].dim);
        assert!(first_line.cells[0].bold);
        assert_eq!(first_line.cells[1].text, "B");
        assert!(first_line.cells[1].has_contents);
        assert!(first_line.cells[1].italic);
        assert_eq!(first_line.cells[2].text, "C");
        assert!(first_line.cells[2].has_contents);
        assert!(first_line.cells[2].underline);
        assert_eq!(first_line.cells[3].text, "D");
        assert!(first_line.cells[3].has_contents);
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

    #[test]
    fn visible_lines_expose_gap_cells_even_when_they_have_no_text_contents() {
        let mut screen = TerminalScreen::new(3, 10, 0);
        screen.process_bytes(&bytes_with_gap("я", 2, "б"));

        let rows = screen.visible_rows();
        let lines = screen.visible_lines();

        assert!(rows[0].starts_with("я  б"));
        assert_eq!(lines[0].cells[0].text, "я");
        assert!(lines[0].cells[0].has_contents);
        assert_eq!(lines[0].cells[1].text, "");
        assert!(!lines[0].cells[1].has_contents);
        assert!(!lines[0].cells[1].is_wide_continuation);
        assert_eq!(lines[0].cells[2].text, "");
        assert!(!lines[0].cells[2].has_contents);
        assert!(!lines[0].cells[2].is_wide_continuation);
        assert_eq!(lines[0].cells[3].text, "б");
        assert!(lines[0].cells[3].has_contents);
    }

    #[test]
    fn visible_lines_mark_wide_character_continuation_cells() {
        let mut screen = TerminalScreen::new(3, 10, 0);
        screen.process_bytes("界a".as_bytes());

        let line = screen.visible_lines().remove(0);

        assert_eq!(line.cells[0].text, "界");
        assert!(line.cells[0].has_contents);
        assert!(line.cells[0].is_wide);
        assert!(!line.cells[0].is_wide_continuation);

        assert_eq!(line.cells[1].text, "");
        assert!(!line.cells[1].has_contents);
        assert!(!line.cells[1].is_wide);
        assert!(line.cells[1].is_wide_continuation);

        assert_eq!(line.cells[2].text, "a");
        assert!(line.cells[2].has_contents);
        assert!(!line.cells[2].is_wide);
        assert!(!line.cells[2].is_wide_continuation);
    }

    #[test]
    fn visible_lines_expose_foreground_and_background_colors() {
        let mut screen = TerminalScreen::new(3, 10, 0);
        screen.process_bytes(b"\x1b[31;47mA\x1b[m");

        let line = screen.visible_lines().remove(0);

        assert_eq!(line.cells[0].text, "A");
        assert_eq!(line.cells[0].fg, ScreenColor::Indexed(1));
        assert_eq!(line.cells[0].bg, ScreenColor::Indexed(7));
    }

    #[test]
    fn visible_lines_expose_dim_attribute() {
        let mut screen = TerminalScreen::new(3, 10, 0);
        screen.process_bytes(b"\x1b[2mA\x1b[m");

        let line = screen.visible_lines().remove(0);

        assert_eq!(line.cells[0].text, "A");
        assert!(line.cells[0].dim);
    }

    #[test]
    fn alternate_screen_sequences_change_explicit_screen_mode() {
        let mut screen = TerminalScreen::new(3, 10, 0);

        assert_eq!(screen.screen_mode(), ScreenMode::Normal);
        assert!(!screen.is_alternate_screen());

        screen.process_bytes(b"\x1b[?1049h");
        assert_eq!(screen.screen_mode(), ScreenMode::Alternate);
        assert!(screen.is_alternate_screen());

        screen.process_bytes(b"\x1b[?1049l");
        assert_eq!(screen.screen_mode(), ScreenMode::Normal);
        assert!(!screen.is_alternate_screen());
    }

    #[test]
    fn alternate_screen_tracking_survives_fragmented_escape_sequences() {
        let mut screen = TerminalScreen::new(3, 10, 0);

        screen.process_bytes(b"\x1b[?");
        assert_eq!(screen.screen_mode(), ScreenMode::Normal);

        screen.process_bytes(b"1049");
        assert_eq!(screen.screen_mode(), ScreenMode::Normal);

        screen.process_bytes(b"h");
        assert_eq!(screen.screen_mode(), ScreenMode::Alternate);

        screen.process_bytes(b"\x1b[?10");
        assert_eq!(screen.screen_mode(), ScreenMode::Alternate);

        screen.process_bytes(b"49l");
        assert_eq!(screen.screen_mode(), ScreenMode::Normal);
    }

    #[test]
    fn alternate_screen_tracking_supports_47_and_1047_variants() {
        let mut screen = TerminalScreen::new(3, 10, 0);

        screen.process_bytes(b"\x1b[?47h");
        assert_eq!(screen.screen_mode(), ScreenMode::Alternate);

        screen.process_bytes(b"\x1b[?47l");
        assert_eq!(screen.screen_mode(), ScreenMode::Normal);

        screen.process_bytes(b"\x1b[?1047h");
        assert_eq!(screen.screen_mode(), ScreenMode::Alternate);

        screen.process_bytes(b"\x1b[?1047l");
        assert_eq!(screen.screen_mode(), ScreenMode::Normal);
    }

    #[test]
    fn leaving_alternate_screen_restores_normal_screen_contents() {
        let mut screen = TerminalScreen::new(4, 20, 10);

        screen.process_bytes(b"shell$ prompt");
        screen.process_bytes(&alt_screen_bytes(b"\x1b[2J\x1b[Hcodex ui"));

        assert_eq!(screen.screen_mode(), ScreenMode::Normal);
        assert!(screen.visible_rows()[0].contains("shell$ prompt"));
        assert!(!screen.visible_rows()[0].contains("codex ui"));
    }

    #[test]
    fn alternate_screen_repaints_do_not_create_useful_line_scrollback_yet() {
        let mut screen = TerminalScreen::new(3, 20, 20);

        screen.process_bytes(b"shell line 1\nshell line 2\n");
        screen.process_bytes(b"\x1b[?1049h");
        screen.process_bytes(b"\x1b[2J\x1b[Hframe1");
        screen.process_bytes(b"\x1b[2J\x1b[Hframe2");
        screen.process_bytes(b"\x1b[2J\x1b[Hframe3");

        let bottom = screen.visible_rows();
        screen.set_scrollback(1);
        let scrolled = screen.visible_rows();

        assert_eq!(screen.screen_mode(), ScreenMode::Alternate);
        assert_ne!(bottom, scrolled);
        assert!(bottom.iter().any(|row| row.contains("frame3")));
        assert!(scrolled.iter().any(|row| row.contains("frame2")));
        assert!(!scrolled.iter().any(|row| row.contains("shell line 1")));
    }

    #[test]
    fn alternate_screen_scrollback_uses_bounded_snapshot_history() {
        let mut screen = TerminalScreen::new(3, 20, 2);

        screen.process_bytes(b"\x1b[?1049h");
        screen.process_bytes(b"\x1b[2J\x1b[Hframe1");
        screen.process_bytes(b"\x1b[2J\x1b[Hframe2");
        screen.process_bytes(b"\x1b[2J\x1b[Hframe3");

        screen.set_scrollback(1);
        let previous = screen.visible_rows();
        screen.set_scrollback(2);
        let clamped = screen.visible_rows();

        assert!(previous.iter().any(|row| row.contains("frame2")));
        assert_eq!(previous, clamped);
        assert_eq!(screen.scrollback(), 1);
    }

    #[test]
    fn alternate_screen_scrollback_captures_multiple_frames_from_single_chunk() {
        let mut screen = TerminalScreen::new(3, 20, 10);

        screen.process_bytes(
            b"\x1b[?1049h\x1b[2J\x1b[Hframe1\x1b[2J\x1b[Hframe2\x1b[2J\x1b[Hframe3",
        );

        let live = screen.visible_rows();
        screen.set_scrollback(1);
        let previous = screen.visible_rows();
        screen.set_scrollback(2);
        let earlier = screen.visible_rows();

        assert!(live.iter().any(|row| row.contains("frame3")));
        assert!(previous.iter().any(|row| row.contains("frame2")));
        assert!(earlier.iter().any(|row| row.contains("frame1")));
    }

    #[test]
    fn alternate_screen_snapshot_history_deduplicates_identical_frames() {
        let mut screen = TerminalScreen::new(3, 20, 10);

        screen.process_bytes(b"\x1b[?1049h\x1b[2J\x1b[Hsame");
        screen.process_bytes(b"\x1b[2J\x1b[Hsame");
        screen.process_bytes(b"\x1b[2J\x1b[Hnext");

        screen.set_scrollback(1);
        let previous = screen.visible_rows();
        screen.set_scrollback(2);

        assert!(previous.iter().any(|row| row.contains("same")));
        assert_eq!(screen.scrollback(), 1);
    }

    #[test]
    fn shows_history_snapshot_is_false_for_live_alternate_screen_and_true_when_scrolled_back() {
        let mut screen = TerminalScreen::new(3, 20, 10);

        screen.process_bytes(b"\x1b[?1049h\x1b[2J\x1b[Hframe1\x1b[2J\x1b[Hframe2");
        assert!(!screen.shows_history_snapshot());

        screen.set_scrollback(1);

        assert!(screen.shows_history_snapshot());
        assert!(screen.visible_rows().iter().any(|row| row.contains("frame1")));
    }

    #[test]
    fn decstbm_region_scroll_should_expose_previous_frame_in_local_scrollback() {
        let mut screen = TerminalScreen::new(6, 20, 20);

        screen.process_bytes(&decstbm_frame_bytes("frame1", "footer1"));
        screen.process_bytes(&decstbm_frame_bytes("frame2", "footer2"));
        screen.process_bytes(&decstbm_frame_bytes("frame3", "footer3"));

        let live = screen.visible_rows();
        screen.set_scrollback(1);
        let previous = screen.visible_rows();
        screen.set_scrollback(2);
        let earlier = screen.visible_rows();

        assert!(live.iter().any(|row| row.contains("frame3")));
        assert!(previous.iter().any(|row| row.contains("frame2")));
        assert!(earlier.iter().any(|row| row.contains("frame1")));
    }

    #[test]
    fn decstbm_partial_repaints_should_not_mix_footer_from_newer_frame_into_older_history() {
        let mut screen = TerminalScreen::new(6, 20, 20);

        screen.process_bytes(&decstbm_frame_bytes("frame1", "footer1"));
        screen.process_bytes(&decstbm_frame_bytes("frame2", "footer2"));
        screen.process_bytes(&decstbm_frame_bytes("frame3", "footer3"));

        screen.set_scrollback(1);
        let previous = screen.visible_rows().join("\n");

        assert!(previous.contains("frame2"));
        assert!(previous.contains("footer2"));
        assert!(!previous.contains("footer3"));
    }
}
