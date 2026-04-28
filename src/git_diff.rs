use std::path::{Path, PathBuf};

use crate::preview::{DiffLine, DiffLineKind};

const MAX_PREVIEW_LINES: usize = 500;
const GIT_DIFF_ARGS_PREFIX: [&str; 5] = [
    "--literal-pathspecs",
    "diff",
    "--no-ext-diff",
    "--no-textconv",
    "HEAD",
];

pub fn load_diff_for(path: &Path, git_cwd: &Path) -> Option<Vec<DiffLine>> {
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

    let text = String::from_utf8_lossy(&output.stdout);
    if text.trim().is_empty() {
        return None;
    }

    let mut lines: Vec<DiffLine> = Vec::with_capacity(MAX_PREVIEW_LINES);
    for raw in text.lines().take(MAX_PREVIEW_LINES) {
        lines.push(DiffLine {
            text: raw.to_string(),
            kind: classify_diff_line(raw),
        });
    }

    Some(lines)
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
}
