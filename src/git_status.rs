use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const GIT_STATUS_ARGS: [&str; 3] = ["status", "--porcelain=v1", "--ignored"];
const GIT_STATUS_UNTRACKED_ARG: &str = "--untracked-files=normal";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitFileState {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Ignored,
    Conflicted,
}

#[derive(Debug, Clone)]
pub struct GitStatusSnapshot {
    pub states: HashMap<PathBuf, GitFileState>,
    pub collected_at: Instant,
}

impl GitStatusSnapshot {
    pub fn state_for(&self, path: &Path) -> Option<GitFileState> {
        let canonical = canonicalize_or_clone(path);
        self.states.get(&canonical).copied()
    }

    pub fn is_stale(&self, max_age: Duration) -> bool {
        self.collected_at.elapsed() >= max_age
    }
}

pub fn collect_snapshot(git_cwd: &Path) -> Option<GitStatusSnapshot> {
    let repo_root = resolve_repo_root(git_cwd)?;
    let output = crate::git_exec::git_command()
        .arg("-c")
        .arg("core.fsmonitor=false")
        .args(GIT_STATUS_ARGS)
        .arg(GIT_STATUS_UNTRACKED_ARG)
        .current_dir(&repo_root)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let states = parse_porcelain_status(&stdout, &repo_root);
    Some(GitStatusSnapshot {
        states,
        collected_at: Instant::now(),
    })
}

pub fn resolve_repo_root(git_cwd: &Path) -> Option<PathBuf> {
    let cwd = canonicalize_or_clone(git_cwd);
    let output = crate::git_exec::git_command()
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&cwd)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if root.is_empty() {
        return None;
    }

    Some(canonicalize_or_clone(Path::new(&root)))
}

fn parse_porcelain_status(stdout: &str, repo_root: &Path) -> HashMap<PathBuf, GitFileState> {
    let mut states = HashMap::new();

    for line in stdout.lines().filter(|line| line.len() >= 3) {
        let status_code = &line[..2];
        let path_part = line[3..].trim();

        if path_part.is_empty() {
            continue;
        }

        let path = normalize_path_part(path_part);
        let absolute_path = canonicalize_or_clone(&repo_root.join(path));
        let state = parse_state(status_code);
        states.insert(absolute_path, state);
    }

    states
}

fn normalize_path_part(path_part: &str) -> &str {
    path_part.rsplit(" -> ").next().unwrap_or(path_part)
}

fn parse_state(status_code: &str) -> GitFileState {
    match status_code {
        "??" => GitFileState::Untracked,
        "!!" => GitFileState::Ignored,
        code if code.contains('U') || code == "AA" || code == "DD" => GitFileState::Conflicted,
        code if code.contains('R') => GitFileState::Renamed,
        code if code.contains('D') => GitFileState::Deleted,
        code if code.contains('A') => GitFileState::Added,
        _ => GitFileState::Modified,
    }
}

fn canonicalize_or_clone(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_state_handles_common_git_codes() {
        assert_eq!(parse_state("??"), GitFileState::Untracked);
        assert_eq!(parse_state("!!"), GitFileState::Ignored);
        assert_eq!(parse_state("UU"), GitFileState::Conflicted);
        assert_eq!(parse_state("R "), GitFileState::Renamed);
        assert_eq!(parse_state(" D"), GitFileState::Deleted);
        assert_eq!(parse_state("A "), GitFileState::Added);
        assert_eq!(parse_state(" M"), GitFileState::Modified);
    }

    #[test]
    fn normalize_path_part_prefers_rename_target() {
        assert_eq!(normalize_path_part("old.rs -> new.rs"), "new.rs");
        assert_eq!(normalize_path_part("src/main.rs"), "src/main.rs");
    }
}
