use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;

use crate::app::AppEvent;

pub(crate) fn pty_reader_thread(
    mut reader: Box<dyn Read + Send>,
    parser: Arc<Mutex<vt100::Parser>>,
    title: Arc<Mutex<String>>,
    scrollback_count: Arc<std::sync::atomic::AtomicUsize>,
    pane_id: usize,
    event_tx: Sender<AppEvent>,
) {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => {
                let _ = event_tx.send(AppEvent::PtyEof(pane_id));
                break;
            }
            Ok(n) => {
                let data = &buf[..n];

                let newlines = data.iter().filter(|&&b| b == b'\n').count();
                if newlines > 0 {
                    scrollback_count.fetch_add(newlines, std::sync::atomic::Ordering::Relaxed);
                }

                if let Some(path) = extract_osc7(data) {
                    let _ = event_tx.send(AppEvent::CwdChanged(pane_id, path));
                }

                if let Some(new_title) = extract_osc_title(data) {
                    if let Ok(mut t) = title.lock() {
                        *t = new_title;
                    }
                }

                let lines = extract_printable_lines(data);

                let mut parser = parser.lock().unwrap_or_else(|e| e.into_inner());
                parser.process(data);
                drop(parser);
                let _ = event_tx.send(AppEvent::PtyOutput { pane_id, lines });
            }
            Err(_) => {
                break;
            }
        }
    }
}

pub(crate) fn extract_printable_lines(data: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(data);
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek() {
                Some('[') => {
                    chars.next();
                    for ch in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&ch) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(ch) = chars.next() {
                        if ch == '\x07' {
                            break;
                        }
                        if ch == '\x1b' {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {
                    chars.next();
                }
            }
        } else if c == '\r' {
            // ignore CR
        } else if c == '\n' {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                lines.push(trimmed);
            }
            current.clear();
        } else if c.is_control() {
            // skip other control chars
        } else {
            current.push(c);
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        lines.push(trimmed);
    }
    lines
}

pub(crate) fn extract_osc7(data: &[u8]) -> Option<PathBuf> {
    let s = std::str::from_utf8(data).ok()?;

    let marker = "\x1b]7;";
    let start = s.find(marker)?;
    let rest = &s[start + marker.len()..];

    let end = rest.find('\x07').or_else(|| rest.find("\x1b\\"));
    let uri = &rest[..end?];

    if let Some(path_str) = uri.strip_prefix("file://") {
        let path = if path_str.starts_with('/') {
            path_str
        } else if let Some(slash_pos) = path_str.find('/') {
            &path_str[slash_pos..]
        } else {
            return None;
        };

        #[cfg(windows)]
        {
            let path_bytes = path.as_bytes();
            if path_bytes.len() >= 3
                && path_bytes[0] == b'/'
                && path_bytes[1].is_ascii_alphabetic()
                && path_bytes[2] == b'/'
            {
                let drive = path_bytes[1].to_ascii_uppercase() as char;
                let rest = &path[2..];
                let win_path = format!("{}:{}", drive, rest.replace('/', "\\"));
                return Some(PathBuf::from(win_path));
            }
        }
        return Some(PathBuf::from(path));
    }

    None
}

pub(crate) fn extract_osc_title(data: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(data).ok()?;
    for marker in &["\x1b]0;", "\x1b]2;"] {
        if let Some(start) = s.find(marker) {
            let rest = &s[start + marker.len()..];
            let end = rest.find('\x07').or_else(|| rest.find("\x1b\\"));
            if let Some(end) = end {
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}
