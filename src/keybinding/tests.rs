use super::*;
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn test_parse_ctrl_q() {
    assert_eq!(
        parse_keybinding("ctrl+q"),
        Some((KeyModifiers::CONTROL, KeyCode::Char('q')))
    );
}

#[test]
fn test_parse_ctrl_space() {
    assert_eq!(
        parse_keybinding("ctrl+space"),
        Some((KeyModifiers::CONTROL, KeyCode::Char(' ')))
    );
}

#[test]
fn test_parse_ctrl_comma() {
    assert_eq!(
        parse_keybinding("ctrl+,"),
        Some((KeyModifiers::CONTROL, KeyCode::Char(',')))
    );
}

#[test]
fn test_parse_alt_z() {
    assert_eq!(
        parse_keybinding("alt+z"),
        Some((KeyModifiers::ALT, KeyCode::Char('z')))
    );
}

#[test]
fn test_parse_alt_left() {
    assert_eq!(
        parse_keybinding("alt+left"),
        Some((KeyModifiers::ALT, KeyCode::Left))
    );
}

#[test]
fn test_parse_alt_right() {
    assert_eq!(
        parse_keybinding("alt+right"),
        Some((KeyModifiers::ALT, KeyCode::Right))
    );
}

#[test]
fn test_parse_bare_char() {
    assert_eq!(
        parse_keybinding("?"),
        Some((KeyModifiers::NONE, KeyCode::Char('?')))
    );
}

#[test]
fn test_parse_invalid() {
    assert_eq!(parse_keybinding("invalid"), None);
}

#[test]
fn test_parse_alt_shift_r() {
    assert_eq!(
        parse_keybinding("alt+shift+r"),
        Some((
            KeyModifiers::ALT | KeyModifiers::SHIFT,
            KeyCode::Char('R')
        ))
    );
}

#[test]
fn test_display_alt_shift_r() {
    assert_eq!(keybinding_display("alt+shift+r"), "A-S-R");
}

#[test]
fn test_display_ctrl_q() {
    assert_eq!(keybinding_display("ctrl+q"), "^Q");
}

#[test]
fn test_display_alt_t() {
    assert_eq!(keybinding_display("alt+t"), "A-T");
}

#[test]
fn test_display_alt_left() {
    assert_eq!(keybinding_display("alt+left"), "A-←");
}

#[test]
fn test_validate_defaults_no_duplicates() {
    let kb = KeybindingsConfig::default();
    let warnings = validate_keybindings(&kb);
    assert!(
        warnings.is_empty(),
        "Default config has duplicates: {:?}",
        warnings
    );
}
