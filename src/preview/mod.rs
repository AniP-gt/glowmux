use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::Instant;

use ratatui::style::Color;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

const MAX_PREVIEW_LINES: usize = 500;
const BINARY_CHECK_BYTES: usize = 8192;
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB
const MAX_IMAGE_SIZE: u64 = 20 * 1024 * 1024; // 20MB for images

/// A styled text span for rendering.
#[derive(Debug, Clone)]
pub struct StyledSpan {
    pub text: String,
    pub fg: (u8, u8, u8),
}

/// Classification of a single line in a unified diff. Drives preview color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Header,
    Hunk,
    Added,
    Removed,
    Context,
}

/// One line of rendered diff output.
#[derive(Debug, Clone)]
pub struct DiffStyledSpan {
    pub text: String,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub text: String,
    pub kind: DiffLineKind,
    pub styled_spans: Vec<DiffStyledSpan>,
}

/// File preview state.
pub struct Preview {
    pub file_path: Option<PathBuf>,
    pub diff_git_cwd: Option<PathBuf>,
    pub lines: Vec<String>,
    pub highlighted_lines: Vec<Vec<StyledSpan>>,
    /// When non-empty, the preview renders these diff lines instead of
    /// the regular syntax-highlighted text. Toggled with `d` in filetree mode.
    pub diff_lines: Vec<DiffLine>,
    pub diff_mode: bool,
    /// Vertical scroll position (line index of the top visible line).
    pub scroll_offset: usize,
    /// Horizontal scroll position (char count dropped from the left of
    /// each rendered line). Enables viewing long lines that exceed the
    /// preview panel width.
    pub h_scroll_offset: usize,
    pub is_binary: bool,
    /// Image preview state (set when an image file is loaded).
    pub image_protocol: Option<StatefulProtocol>,
    pub last_diff_loaded_at: Option<Instant>,
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl Preview {
    pub fn new() -> Self {
        Self {
            file_path: None,
            diff_git_cwd: None,
            lines: Vec::new(),
            highlighted_lines: Vec::new(),
            diff_lines: Vec::new(),
            diff_mode: false,
            scroll_offset: 0,
            h_scroll_offset: 0,
            is_binary: false,
            image_protocol: None,
            last_diff_loaded_at: None,
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }

    /// Check if the current preview is an image.
    pub fn is_image(&self) -> bool {
        self.image_protocol.is_some()
    }

    /// Load a file for preview.
    pub fn load(&mut self, path: &Path, picker: Option<&mut Picker>) {
        if self.file_path.as_deref() == Some(path) {
            return;
        }

        self.file_path = Some(path.to_path_buf());
        self.diff_git_cwd = None;
        self.scroll_offset = 0;
        self.h_scroll_offset = 0;
        self.lines.clear();
        self.highlighted_lines.clear();
        self.diff_lines.clear();
        self.diff_mode = false;
        self.last_diff_loaded_at = None;
        self.is_binary = false;
        self.image_protocol = None;

        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => {
                self.lines = vec!["Failed to read file".to_string()];
                return;
            }
        };

        if !metadata.is_file() {
            self.lines = vec!["Not a regular file".to_string()];
            return;
        }

        // Try loading as image first (by extension)
        if is_image_extension(path) {
            if metadata.len() > MAX_IMAGE_SIZE {
                self.lines = vec![format!(
                    "Image too large ({:.1}MB > {:.0}MB)",
                    metadata.len() as f64 / 1024.0 / 1024.0,
                    MAX_IMAGE_SIZE as f64 / 1024.0 / 1024.0
                )];
                return;
            }
            if let Some(picker) = picker {
                match image::ImageReader::open(path)
                    .and_then(|r| r.with_guessed_format())
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.decode().map_err(|e| e.to_string()))
                {
                    Ok(dyn_img) => {
                        self.image_protocol = Some(picker.new_resize_protocol(dyn_img));
                        return;
                    }
                    Err(_) => {
                        // Fall through to text/binary preview
                    }
                }
            }
        }

        if metadata.len() > MAX_FILE_SIZE {
            self.lines = vec![format!(
                "File too large ({:.1}MB > {:.0}MB)",
                metadata.len() as f64 / 1024.0 / 1024.0,
                MAX_FILE_SIZE as f64 / 1024.0 / 1024.0
            )];
            return;
        }

        if is_binary_file(path) {
            self.is_binary = true;
            return;
        }

        // Read text file
        match File::open(path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                self.lines = reader
                    .lines()
                    .take(MAX_PREVIEW_LINES)
                    .filter_map(|l| l.ok())
                    .collect();
            }
            Err(_) => {
                self.lines = vec!["Failed to read file".to_string()];
                return;
            }
        }

        // Apply syntax highlighting
        self.highlight(path);
    }

    /// Apply syntax highlighting to loaded lines.
    fn highlight(&mut self, path: &Path) {
        let syntax = self
            .syntax_set
            .find_syntax_for_file(path)
            .ok()
            .flatten()
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme = &self.theme_set.themes["base16-eighties.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);

        self.highlighted_lines.clear();

        for line in &self.lines {
            let line_with_newline = format!("{}\n", line);
            match highlighter.highlight_line(&line_with_newline, &self.syntax_set) {
                Ok(ranges) => {
                    let spans: Vec<StyledSpan> = ranges
                        .into_iter()
                        .map(|(style, text)| {
                            let fg = style.foreground;
                            StyledSpan {
                                text: text.trim_end_matches('\n').to_string(),
                                fg: (fg.r, fg.g, fg.b),
                            }
                        })
                        .filter(|s| !s.text.is_empty())
                        .collect();
                    self.highlighted_lines.push(spans);
                }
                Err(_) => {
                    // Fallback: plain text
                    self.highlighted_lines.push(vec![StyledSpan {
                        text: line.clone(),
                        fg: (0xe6, 0xed, 0xf3),
                    }]);
                }
            }
        }
    }

    /// Close the preview.
    pub fn close(&mut self) {
        self.file_path = None;
        self.diff_git_cwd = None;
        self.lines.clear();
        self.highlighted_lines.clear();
        self.diff_lines.clear();
        self.diff_mode = false;
        self.scroll_offset = 0;
        self.h_scroll_offset = 0;
        self.is_binary = false;
        self.image_protocol = None;
        self.last_diff_loaded_at = None;
    }

    pub fn toggle_diff_for(&mut self, git_cwd: &Path, prefer_delta: bool) -> bool {
        let Some(path) = self.file_path.clone() else {
            return false;
        };
        if self.diff_mode {
            self.diff_mode = false;
            self.scroll_offset = 0;
            return true;
        }
        if self.diff_lines.is_empty() || self.diff_git_cwd.as_deref() != Some(git_cwd) {
            if let Some(lines) = crate::git_diff::load_diff_for(&path, git_cwd, prefer_delta) {
                self.diff_lines = lines;
                self.diff_git_cwd = Some(git_cwd.to_path_buf());
                self.last_diff_loaded_at = Some(Instant::now());
            } else {
                self.diff_lines.clear();
                self.diff_git_cwd = Some(git_cwd.to_path_buf());
            }
        }
        if self.diff_lines.is_empty() {
            return false;
        }
        self.diff_mode = true;
        self.scroll_offset = 0;
        true
    }

    /// Check if preview is active.
    pub fn is_active(&self) -> bool {
        self.file_path.is_some()
    }

    /// Get the filename for display.
    pub fn filename(&self) -> String {
        self.file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    /// Scroll up by amount.
    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    /// Scroll down by amount.
    pub fn scroll_down(&mut self, amount: usize) {
        let total = if self.diff_mode {
            self.diff_lines.len()
        } else {
            self.lines.len()
        };
        let max_offset = total.saturating_sub(1);
        self.scroll_offset = (self.scroll_offset + amount).min(max_offset);
    }

    /// Scroll left by N chars. Clamps at column 0.
    pub fn scroll_left(&mut self, amount: usize) {
        self.h_scroll_offset = self.h_scroll_offset.saturating_sub(amount);
    }

    /// Scroll right by N chars. Clamped so there's always at least
    /// a bit of text visible (we stop when h_scroll equals the widest
    /// line minus 10 chars — keeps the user from scrolling off into
    /// blank territory).
    pub fn scroll_right(&mut self, amount: usize) {
        let widest = if self.diff_mode {
            self.diff_lines
                .iter()
                .map(|l| l.text.chars().count())
                .max()
                .unwrap_or(0)
        } else {
            self.lines
                .iter()
                .map(|l| l.chars().count())
                .max()
                .unwrap_or(0)
        };
        let max_h = widest.saturating_sub(10);
        self.h_scroll_offset = (self.h_scroll_offset + amount).min(max_h);
    }
}

/// Check if a file has an image extension.
fn is_image_extension(path: &Path) -> bool {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "ico" | "tiff" | "tif"
    )
}

/// Check if a file is likely binary by reading only the first N bytes.
fn is_binary_file(path: &Path) -> bool {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut reader = BufReader::new(file);
    let mut buf = [0u8; BINARY_CHECK_BYTES];
    match reader.read(&mut buf) {
        Ok(n) => buf[..n].contains(&0),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests;
