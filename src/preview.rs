use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::easy::HighlightLines;

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
pub struct DiffLine {
    pub text: String,
    pub kind: DiffLineKind,
}

/// File preview state.
pub struct Preview {
    pub file_path: Option<PathBuf>,
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
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl Preview {
    pub fn new() -> Self {
        Self {
            file_path: None,
            lines: Vec::new(),
            highlighted_lines: Vec::new(),
            diff_lines: Vec::new(),
            diff_mode: false,
            scroll_offset: 0,
            h_scroll_offset: 0,
            is_binary: false,
            image_protocol: None,
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
        self.scroll_offset = 0;
        self.h_scroll_offset = 0;
        self.lines.clear();
        self.highlighted_lines.clear();
        self.diff_lines.clear();
        self.diff_mode = false;
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
        self.lines.clear();
        self.highlighted_lines.clear();
        self.diff_lines.clear();
        self.diff_mode = false;
        self.scroll_offset = 0;
        self.h_scroll_offset = 0;
        self.is_binary = false;
        self.image_protocol = None;
    }

    /// Toggle diff preview mode. Loads the diff for the currently-previewed
    /// file the first time it's enabled (or after the file has changed).
    /// Returns true if diff content was found, false if there's no diff to show.
    pub fn toggle_diff(&mut self) -> bool {
        let Some(path) = self.file_path.clone() else {
            return false;
        };
        if self.diff_mode {
            // Switching off — keep cached lines so re-toggling is cheap.
            self.diff_mode = false;
            self.scroll_offset = 0;
            return true;
        }
        if self.diff_lines.is_empty() {
            if let Some(lines) = load_diff_for(&path) {
                self.diff_lines = lines;
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
        let total = if self.diff_mode { self.diff_lines.len() } else { self.lines.len() };
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

/// Run `git diff HEAD -- {path}` and parse the output into colored diff lines.
/// Returns None when the file has no diff, the path is not under a git repo,
/// or the command fails — callers should fall back to normal preview.
pub fn load_diff_for(path: &Path) -> Option<Vec<DiffLine>> {
    let path_str = path.to_string_lossy();
    let cwd = path.parent().unwrap_or_else(|| Path::new("."));

    let output = std::process::Command::new("git")
        .args(["diff", "HEAD", "--", path_str.as_ref()])
        .current_dir(cwd)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    if text.trim().is_empty() {
        return None;
    }

    let mut lines: Vec<DiffLine> = Vec::with_capacity(MAX_PREVIEW_LINES);
    for raw in text.lines().take(MAX_PREVIEW_LINES) {
        let kind = classify_diff_line(raw);
        lines.push(DiffLine {
            text: raw.to_string(),
            kind,
        });
    }
    Some(lines)
}

fn classify_diff_line(line: &str) -> DiffLineKind {
    if line.starts_with("@@") {
        DiffLineKind::Hunk
    } else if line.starts_with("+++") || line.starts_with("---")
        || line.starts_with("diff ") || line.starts_with("index ")
        || line.starts_with("new file") || line.starts_with("deleted file")
        || line.starts_with("rename ") || line.starts_with("similarity ")
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
mod tests {
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

    #[test]
    fn test_classify_diff_line() {
        assert_eq!(classify_diff_line("@@ -1,5 +1,7 @@"), DiffLineKind::Hunk);
        assert_eq!(classify_diff_line("+++ b/file.rs"), DiffLineKind::Header);
        assert_eq!(classify_diff_line("--- a/file.rs"), DiffLineKind::Header);
        assert_eq!(classify_diff_line("diff --git a/x b/x"), DiffLineKind::Header);
        assert_eq!(classify_diff_line("+added line"), DiffLineKind::Added);
        assert_eq!(classify_diff_line("-removed line"), DiffLineKind::Removed);
        assert_eq!(classify_diff_line(" context line"), DiffLineKind::Context);
        assert_eq!(classify_diff_line(""), DiffLineKind::Context);
    }
}
