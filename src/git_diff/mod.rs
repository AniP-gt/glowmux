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

pub(crate) fn load_delta_result(
    git_stdout: &[u8],
    prefer_delta: bool,
) -> Option<Option<DeltaRenderResult>> {
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

pub(crate) fn parse_ansi_line(line: &str) -> Vec<DiffStyledSpan> {
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

pub(crate) fn strip_ansi_sequences(text: &str) -> String {
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
pub(crate) struct DeltaRenderResult {
    pub(crate) plain_text: String,
    pub(crate) styled_lines: Vec<Vec<DiffStyledSpan>>,
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

pub(crate) fn relativize_to_git_cwd(path: &Path, git_cwd: &Path) -> Option<String> {
    let absolute_path = canonicalize_or_clone(path);
    let absolute_git_cwd = canonicalize_or_clone(git_cwd);

    let relative = absolute_path.strip_prefix(&absolute_git_cwd).ok()?;
    Some(relative.to_string_lossy().to_string())
}

fn canonicalize_or_clone(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn classify_diff_line(line: &str) -> DiffLineKind {
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
mod tests;
