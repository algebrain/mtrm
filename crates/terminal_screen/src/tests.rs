
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

    screen.process_bytes(b"\x1b[?1049h\x1b[2J\x1b[Hframe1\x1b[2J\x1b[Hframe2\x1b[2J\x1b[Hframe3");

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
fn alternate_screen_repaint_does_not_capture_snapshot_on_every_escape_boundary() {
    let mut screen = TerminalScreen::new(24, 80, 100);

    screen.process_bytes(b"\x1b[?1049h\x1b[2J\x1b[Hframe1\x1b[10;10Hvalue\x1b[20;1Hfooter");

    assert!(
        screen.debug_alternate_capture_count() <= 2,
        "capture count was {}",
        screen.debug_alternate_capture_count()
    );
}

#[test]
fn fragmented_alternate_repaint_keeps_history_without_capture_storm() {
    let mut screen = TerminalScreen::new(4, 20, 20);

    screen.process_bytes(b"\x1b[?1049h");
    screen.process_bytes(b"\x1b[2J\x1b[Hframe1");
    screen.process_bytes(b"\x1b[10;10Hvalue");
    screen.process_bytes(b"\x1b[20;1Hfooter");

    assert!(
        screen.debug_alternate_capture_count() <= 4,
        "capture count was {}",
        screen.debug_alternate_capture_count()
    );

    screen.set_scrollback(1);
    let previous = screen.visible_rows().join("\n");
    assert!(previous.contains("frame1"));
}

#[test]
fn shows_history_snapshot_is_false_for_live_alternate_screen_and_true_when_scrolled_back() {
    let mut screen = TerminalScreen::new(3, 20, 10);

    screen.process_bytes(b"\x1b[?1049h\x1b[2J\x1b[Hframe1\x1b[2J\x1b[Hframe2");
    assert!(!screen.shows_history_snapshot());

    screen.set_scrollback(1);

    assert!(screen.shows_history_snapshot());
    assert!(
        screen
            .visible_rows()
            .iter()
            .any(|row| row.contains("frame1"))
    );
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
