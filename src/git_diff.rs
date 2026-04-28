use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use ratatui::style::Color;

use crate::preview::{DiffLine, DiffLineKind, DiffStyledSpan};

const MAX_PREVIEW_LINES: usize = 500;
const GIT_DIFF_ARGS_PREFIX: [&str; 5] = [
    "--literal-pathspecs",
    "diff",
    "--no-ext-diff",
    "--no-textconv",
    "HEAD",
];
const DELTA_ARGS: [&str; 5] = [
    "--paging=never",
    "--no-gitconfig",
    "--width=variable",
    "--wrap-max-lines=0",
    "--color-only",
];

pub fn load_diff_for(path: &Path, git_cwd: &Path, prefer_delta: bool) -> Option<Vec<DiffLine>> {
    let diff_target = relativize_to_git_cwd(path, git_cwd)?;

    let output = crate::git_exec::git_command()
        .args(GIT_DIFF_ARGS_PREFIX)
        .args(["--", &diff_target])
        .current_dir(git_cwd)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let git_text = String::from_utf8_lossy(&output.stdout).to_string();
    if git_text.trim().is_empty() {
        return None;
    }

    let delta_result = load_delta_result(&output.stdout, prefer_delta)?;
    let text = delta_result
        .as_ref()
        .map(|result| result.plain_text.clone())
        .unwrap_or(git_text);
    let styled_lines = delta_result
        .map(|result| result.styled_lines)
        .unwrap_or_default();

    if text.trim().is_empty() {
        return None;
    }

    let mut lines: Vec<DiffLine> = Vec::with_capacity(MAX_PREVIEW_LINES);
    for (index, raw) in text.lines().take(MAX_PREVIEW_LINES).enumerate() {
        lines.push(DiffLine {
            text: raw.to_string(),
            kind: classify_diff_line(raw),
            styled_spans: styled_lines.get(index).cloned().unwrap_or_default(),
        });
    }

    Some(lines)
}

fn load_delta_result(git_stdout: &[u8], prefer_delta: bool) -> Option<Option<DeltaRenderResult>> {
    let Some(delta_output) = run_delta(git_stdout, prefer_delta) else {
        return Some(None);
    };

    let ansi_text = String::from_utf8_lossy(&delta_output.stdout).to_string();
    let plain_text = strip_ansi_sequences(&ansi_text);
    if plain_text.trim().is_empty() || !looks_like_diff_output(&plain_text) {
        return Some(None);
    }

    Some(Some(DeltaRenderResult {
        plain_text,
        styled_lines: parse_ansi_diff_lines(&ansi_text),
    }))
}

fn run_delta(git_stdout: &[u8], prefer_delta: bool) -> Option<std::process::Output> {
    if !prefer_delta {
        return None;
    }

    let delta_bin = crate::git_exec::delta_binary()?;
    let mut command = std::process::Command::new(delta_bin);
    command.args(DELTA_ARGS);
    command.arg("--true-color=always");
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let mut child = command.spawn().ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(git_stdout).is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    }

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }

    Some(output)
}

fn looks_like_diff_output(text: &str) -> bool {
    text.lines().any(|line| {
        line.starts_with("diff ")
            || line.starts_with("@@")
            || line.starts_with("+++")
            || line.starts_with("---")
            || line.starts_with('+')
            || line.starts_with('-')
    })
}

fn parse_ansi_diff_lines(text: &str) -> Vec<Vec<DiffStyledSpan>> {
    text.lines()
        .take(MAX_PREVIEW_LINES)
        .map(parse_ansi_line)
        .collect()
}

fn parse_ansi_line(line: &str) -> Vec<DiffStyledSpan> {
    if !line.contains('\u{1b}') {
        return Vec::new();
    }

    let mut parser = vt100::Parser::new(1, 2048, 0);
    parser.process(line.as_bytes());
    let screen = parser.screen();
    let mut spans = Vec::new();
    let mut current = SpanState::default();
    let mut current_text = String::new();

    for col in 0..screen.size().1 {
        let Some(cell) = screen.cell(0, col) else {
            continue;
        };
        let symbol = cell.contents();
        if symbol.is_empty() {
            continue;
        }

        let next_state = SpanState {
            fg: vt100_color_to_option(cell.fgcolor()),
            bg: vt100_color_to_option(cell.bgcolor()),
            bold: cell.bold(),
        };

        if !current_text.is_empty() && current != next_state {
            spans.push(current.into_span(std::mem::take(&mut current_text)));
        }

        current = next_state;
        current_text.push_str(symbol);
    }

    if !current_text.is_empty() {
        spans.push(current.into_span(current_text));
    }

    spans
}

fn strip_ansi_sequences(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            result.push(ch);
            continue;
        }

        match chars.peek().copied() {
            Some('[') => {
                chars.next();
                for next in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            Some(_) => {
                chars.next();
            }
            None => break,
        }
    }

    result
}

fn vt100_color_to_option(color: vt100::Color) -> Option<Color> {
    match color {
        vt100::Color::Default => None,
        vt100::Color::Idx(idx) => Some(Color::Indexed(idx)),
        vt100::Color::Rgb(r, g, b) => Some(Color::Rgb(r, g, b)),
    }
}

#[derive(Debug, Clone)]
struct DeltaRenderResult {
    plain_text: String,
    styled_lines: Vec<Vec<DiffStyledSpan>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SpanState {
    fg: Option<Color>,
    bg: Option<Color>,
    bold: bool,
}

impl SpanState {
    fn into_span(self, text: String) -> DiffStyledSpan {
        DiffStyledSpan {
            text,
            fg: self.fg,
            bg: self.bg,
            bold: self.bold,
        }
    }
}

fn relativize_to_git_cwd(path: &Path, git_cwd: &Path) -> Option<String> {
    let absolute_path = canonicalize_or_clone(path);
    let absolute_git_cwd = canonicalize_or_clone(git_cwd);

    let relative = absolute_path.strip_prefix(&absolute_git_cwd).ok()?;
    Some(relative.to_string_lossy().to_string())
}

fn canonicalize_or_clone(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn classify_diff_line(line: &str) -> DiffLineKind {
    if line.starts_with("@@") {
        DiffLineKind::Hunk
    } else if line.starts_with("+++")
        || line.starts_with("---")
        || line.starts_with("diff ")
        || line.starts_with("index ")
        || line.starts_with("new file")
        || line.starts_with("deleted file")
        || line.starts_with("rename ")
        || line.starts_with("similarity ")
    {
        DiffLineKind::Header
    } else if line.starts_with('+') {
        DiffLineKind::Added
    } else if line.starts_with('-') {
        DiffLineKind::Removed
    } else {
        DiffLineKind::Context
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::fs;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_delta_bin_override<T>(value: OsString, f: impl FnOnce() -> T) -> T {
        let _guard = env_lock().lock().unwrap();
        let previous = std::env::var_os("GLOWMUX_DELTA_BIN");
        std::env::set_var("GLOWMUX_DELTA_BIN", &value);
        let result = f();
        if let Some(previous) = previous {
            std::env::set_var("GLOWMUX_DELTA_BIN", previous);
        } else {
            std::env::remove_var("GLOWMUX_DELTA_BIN");
        }
        result
    }

    #[test]
    fn classify_diff_line_matches_expected_kinds() {
        assert_eq!(classify_diff_line("@@ -1,5 +1,7 @@"), DiffLineKind::Hunk);
        assert_eq!(classify_diff_line("+++ b/file.rs"), DiffLineKind::Header);
        assert_eq!(classify_diff_line("--- a/file.rs"), DiffLineKind::Header);
        assert_eq!(
            classify_diff_line("diff --git a/x b/x"),
            DiffLineKind::Header
        );
        assert_eq!(classify_diff_line("+added line"), DiffLineKind::Added);
        assert_eq!(classify_diff_line("-removed line"), DiffLineKind::Removed);
        assert_eq!(classify_diff_line(" context line"), DiffLineKind::Context);
        assert_eq!(classify_diff_line(""), DiffLineKind::Context);
    }

    #[test]
    fn relativize_to_git_cwd_returns_relative_path() {
        let cwd = Path::new("/tmp/repo");
        let path = Path::new("/tmp/repo/src/main.rs");
        assert_eq!(
            relativize_to_git_cwd(path, cwd),
            Some("src/main.rs".to_string())
        );
    }

    #[test]
    fn parse_ansi_line_returns_spans() {
        let spans = parse_ansi_line("\u{1b}[31m-old\u{1b}[0m");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "-old");
        assert_eq!(spans[0].fg, Some(Color::Indexed(1)));
    }

    #[test]
    fn strip_ansi_sequences_removes_color_codes() {
        assert_eq!(strip_ansi_sequences("\u{1b}[31m-old\u{1b}[0m\n"), "-old\n");
    }

    #[test]
    fn load_delta_result_falls_back_when_delta_output_is_not_diff_like() {
        let temp_dir =
            std::env::temp_dir().join(format!("glowmux-delta-non-diff-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let script_path = temp_dir.join("fake-delta.sh");
        fs::write(&script_path, b"#!/bin/sh\nprintf 'not a diff\n'").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
        }

        with_delta_bin_override(script_path.into_os_string(), || {
            assert!(matches!(
                load_delta_result(b"diff --git a/x b/x\n+ok\n", true),
                Some(None)
            ));
        });
    }
}
