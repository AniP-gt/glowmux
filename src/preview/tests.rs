use super::*;

#[test]
fn test_preview_initial_state() {
    let preview = Preview::new();
    assert!(!preview.is_active());
    assert!(preview.lines.is_empty());
}

#[test]
fn test_preview_load_text_file() {
    let mut preview = Preview::new();
    preview.load(Path::new("Cargo.toml"), None);
    assert!(preview.is_active());
    assert!(!preview.is_binary);
    assert!(!preview.lines.is_empty());
    assert!(!preview.highlighted_lines.is_empty());
}

#[test]
fn test_preview_close() {
    let mut preview = Preview::new();
    preview.load(Path::new("Cargo.toml"), None);
    assert!(preview.is_active());

    preview.close();
    assert!(!preview.is_active());
    assert!(preview.lines.is_empty());
    assert!(preview.highlighted_lines.is_empty());
}

#[test]
fn test_preview_scroll() {
    let mut preview = Preview::new();
    preview.lines = (0..100).map(|i| format!("line {}", i)).collect();
    preview.scroll_down(10);
    assert_eq!(preview.scroll_offset, 10);
    preview.scroll_up(5);
    assert_eq!(preview.scroll_offset, 5);
    preview.scroll_up(100);
    assert_eq!(preview.scroll_offset, 0);
}

#[test]
fn test_preview_highlight_rust() {
    let mut preview = Preview::new();
    preview.load(Path::new("src/main.rs"), None);
    assert!(!preview.highlighted_lines.is_empty());
    // Highlighted lines should have colored spans
    let first = &preview.highlighted_lines[0];
    assert!(!first.is_empty());
}
