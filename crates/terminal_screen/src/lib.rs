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
    #[cfg(test)]
    debug_alternate_capture_count: usize,
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
            #[cfg(test)]
            debug_alternate_capture_count: 0,
        }
    }

    pub fn process_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            let alternate_byte_affects_frame =
                alternate_byte_affects_frame_in_state(byte, &self.escape_state);

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

            let previous_mode = self.screen_mode;
            let rows = self.size().0;
            let escape_action = self.track_escape_state_byte(byte, rows);

            if previous_mode == ScreenMode::Alternate && escape_action.leave_alternate_screen {
                self.maybe_capture_alternate_snapshot(AlternateCaptureReason::LeaveAlternateScreen);
            } else if self.screen_mode == ScreenMode::Alternate
                && escape_action.alternate_repaint_boundary
            {
                self.maybe_capture_alternate_snapshot(AlternateCaptureReason::RepaintBoundary);
            }

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
                if alternate_byte_affects_frame {
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

        self.maybe_capture_alternate_snapshot(AlternateCaptureReason::EndOfChunk);
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
            self.maybe_capture_alternate_snapshot(AlternateCaptureReason::Resize);
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
        #[cfg(test)]
        {
            self.debug_alternate_capture_count += 1;
        }

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
        self.alt_scrollback = self
            .alt_scrollback
            .min(self.alt_history.len().saturating_sub(1));
    }

    fn maybe_capture_alternate_snapshot(&mut self, _reason: AlternateCaptureReason) {
        if !self.alt_dirty_since_snapshot {
            return;
        }

        if self.screen_mode != ScreenMode::Alternate
            && !matches!(_reason, AlternateCaptureReason::LeaveAlternateScreen)
        {
            return;
        }

        self.capture_alternate_snapshot();
        self.alt_dirty_since_snapshot = false;
    }

    #[cfg(test)]
    fn debug_alternate_capture_count(&self) -> usize {
        self.debug_alternate_capture_count
    }

    fn track_escape_state_byte(&mut self, byte: u8, rows: u16) -> EscapeSequenceAction {
        let mut action = EscapeSequenceAction::default();

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
                    let previous_mode = self.screen_mode;
                    update_screen_state_from_csi(
                        &mut self.screen_mode,
                        &mut self.custom_scroll_region_active,
                        rows,
                        params,
                        byte,
                    );
                    action.leave_alternate_screen = previous_mode == ScreenMode::Alternate
                        && self.screen_mode == ScreenMode::Normal;
                    action.alternate_repaint_boundary = previous_mode == ScreenMode::Alternate
                        && is_alternate_repaint_boundary(params, byte);
                    self.escape_state = EscapeSequenceState::Ground;
                } else if byte == 0x1b {
                    self.escape_state = EscapeSequenceState::Escape;
                } else {
                    params.push(byte);
                }
            }
        }

        action
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EscapeSequenceState {
    Ground,
    Escape,
    Csi(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct EscapeSequenceAction {
    leave_alternate_screen: bool,
    alternate_repaint_boundary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlternateCaptureReason {
    EndOfChunk,
    LeaveAlternateScreen,
    Resize,
    RepaintBoundary,
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

fn alternate_byte_affects_frame_in_state(byte: u8, state: &EscapeSequenceState) -> bool {
    match state {
        EscapeSequenceState::Ground => byte_affects_alternate_frame(byte),
        EscapeSequenceState::Escape => false,
        EscapeSequenceState::Csi(_) => {
            is_csi_final_byte(byte) && csi_final_byte_affects_alternate_frame(byte)
        }
    }
}

fn csi_final_byte_affects_alternate_frame(byte: u8) -> bool {
    matches!(
        byte,
        b'@' | b'A'
            | b'B'
            | b'C'
            | b'D'
            | b'E'
            | b'F'
            | b'G'
            | b'H'
            | b'J'
            | b'K'
            | b'L'
            | b'M'
            | b'P'
            | b'S'
            | b'T'
            | b'X'
            | b'd'
            | b'f'
            | b'h'
            | b'l'
            | b'm'
            | b'r'
    )
}

fn is_alternate_repaint_boundary(params: &[u8], final_byte: u8) -> bool {
    matches!(final_byte, b'J') && clears_entire_screen(params)
}

fn clears_entire_screen(params: &[u8]) -> bool {
    params.is_empty() || matches!(params, b"2" | b"3")
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
    lines
        .iter()
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
mod tests;
