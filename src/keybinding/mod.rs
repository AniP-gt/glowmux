use crate::config::KeybindingsConfig;
use crossterm::event::{KeyCode, KeyModifiers};

/// Parse a keybinding string into (modifiers, keycode).
/// Supported formats:
///   "ctrl+<char>"   e.g. "ctrl+q", "ctrl+,"
///   "alt+<char>"    e.g. "alt+z", "alt+a"
///   "alt+left"      → (ALT, Left)
///   "alt+right"     → (ALT, Right)
///   "alt+up"        → (ALT, Up)
///   "alt+down"      → (ALT, Down)
///   "<char>"        bare character, e.g. "?" → (NONE, Char('?'))
/// Returns None for unrecognized formats.
pub fn parse_keybinding(s: &str) -> Option<(KeyModifiers, KeyCode)> {
    let s = s.trim().to_lowercase();
    if let Some(rest) = s.strip_prefix("ctrl+") {
        match rest {
            "space" => return Some((KeyModifiers::CONTROL, KeyCode::Char(' '))),
            "enter" => return Some((KeyModifiers::CONTROL, KeyCode::Enter)),
            _ => {}
        }
        let c = rest.chars().next()?;
        if rest.chars().count() == 1 {
            return Some((KeyModifiers::CONTROL, KeyCode::Char(c)));
        }
        return None;
    }
    if let Some(rest) = s.strip_prefix("alt+") {
        match rest {
            "left" => return Some((KeyModifiers::ALT, KeyCode::Left)),
            "right" => return Some((KeyModifiers::ALT, KeyCode::Right)),
            "up" => return Some((KeyModifiers::ALT, KeyCode::Up)),
            "down" => return Some((KeyModifiers::ALT, KeyCode::Down)),
            _ => {
                if let Some(tail) = rest.strip_prefix("shift+") {
                    let c = tail.chars().next()?;
                    if tail.chars().count() == 1 {
                        return Some((
                            KeyModifiers::ALT | KeyModifiers::SHIFT,
                            KeyCode::Char(c.to_ascii_uppercase()),
                        ));
                    }
                    return None;
                }
                let c = rest.chars().next()?;
                if rest.chars().count() == 1 {
                    return Some((KeyModifiers::ALT, KeyCode::Char(c)));
                }
                return None;
            }
        }
    }
    // Bare character
    let c = s.chars().next()?;
    if s.chars().count() == 1 {
        Some((KeyModifiers::NONE, KeyCode::Char(c)))
    } else {
        None
    }
}

/// Format a keybinding string for compact status bar display.
///   "ctrl+q"   → "^Q"
///   "alt+t"    → "A-T"
///   "alt+left" → "A-←"
///   "?"        → "?"
pub fn keybinding_display(s: &str) -> String {
    let lower = s.trim().to_lowercase();
    if let Some(rest) = lower.strip_prefix("ctrl+") {
        match rest {
            "space" => return "^Space".to_string(),
            "enter" => return "^Enter".to_string(),
            _ => {}
        }
        return format!("^{}", rest.to_uppercase());
    }
    if let Some(rest) = lower.strip_prefix("alt+") {
        if let Some(tail) = rest.strip_prefix("shift+") {
            return format!("A-S-{}", tail.to_uppercase());
        }
        let sym = match rest {
            "left" => "←".to_string(),
            "right" => "→".to_string(),
            "up" => "↑".to_string(),
            "down" => "↓".to_string(),
            _ => rest.to_uppercase(),
        };
        return format!("A-{}", sym);
    }
    s.to_string()
}

/// Check all keybinding fields for duplicates.
/// Returns human-readable descriptions for any duplicate (mods, code) pairs.
pub fn validate_keybindings(kb: &KeybindingsConfig) -> Vec<String> {
    use std::collections::HashMap;
    let fields: &[(&str, &str)] = &[
        ("prefix", &kb.prefix),
        ("zoom", &kb.zoom),
        ("layout_cycle", &kb.layout_cycle),
        ("layout_picker", &kb.layout_picker),
        ("pane_left", &kb.pane_left),
        ("pane_right", &kb.pane_right),
        ("pane_up", &kb.pane_up),
        ("pane_down", &kb.pane_down),
        ("quit", &kb.quit),
        ("tab_rename", &kb.tab_rename),
        ("pane_rename", &kb.pane_rename),
        ("tab_new", &kb.tab_new),
        ("tab_next", &kb.tab_next),
        ("tab_prev", &kb.tab_prev),
        ("settings", &kb.settings),
        ("file_tree", &kb.file_tree),
        ("preview_swap", &kb.preview_swap),
        ("split_vertical", &kb.split_vertical),
        ("split_horizontal", &kb.split_horizontal),
        ("pane_close", &kb.pane_close),
        ("pane_create", &kb.pane_create),
        ("clipboard_copy", &kb.clipboard_copy),
        ("ai_title_toggle", &kb.ai_title_toggle),
        ("feature_toggle", &kb.feature_toggle),
        ("pane_next", &kb.pane_next),
        ("pane_prev", &kb.pane_prev),
        ("pane_list", &kb.pane_list),
    ];

    let mut seen: HashMap<(KeyModifiers, KeyCode), Vec<&str>> = HashMap::new();
    for (name, binding) in fields {
        if let Some(parsed) = parse_keybinding(binding) {
            seen.entry(parsed).or_default().push(name);
        }
    }

    let mut warnings = Vec::new();
    for (_, names) in &seen {
        if names.len() > 1 {
            warnings.push(format!("duplicate binding: {}", names.join(", ")));
        }
    }
    warnings
}

#[cfg(test)]
mod tests;
