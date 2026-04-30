use std::path::{Path, PathBuf};

use anyhow::Result;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
    pub is_main: bool,
}

/// Options controlling how a new worktree is created. Mirrors the relevant
/// fields of [`crate::config::WorktreeConfig`] so the manager can be used
/// without holding a reference to the whole config.
#[derive(Debug, Clone, Default)]
pub struct WorktreeCreateOptions {
    /// Use the `gwq` CLI when available.
    pub prefer_gwq: bool,
    /// Subdirectory under the repo root for git-worktree-managed worktrees
    /// (e.g. ".glowmux"). Used only when falling back to `git worktree`.
    pub worktree_dir: String,
    /// Base branch passed to `git worktree add ... <base>`. When empty,
    /// no base argument is passed (git uses HEAD).
    pub base_branch: String,
}

pub struct WorktreeManager {
    pub gwq_available: bool,
}

impl WorktreeManager {
    pub fn new() -> Self {
        let gwq_available = which_gwq_exists();
        Self { gwq_available }
    }

    /// Create a worktree honoring [`WorktreeCreateOptions`]. Falls back to
    /// `git worktree` automatically when gwq is unavailable.
    pub fn create_with_options(
        &self,
        repo_root: &Path,
        branch_name: &str,
        opts: &WorktreeCreateOptions,
    ) -> Result<PathBuf> {
        if !validate_branch_name(branch_name) {
            return Err(anyhow::anyhow!("Invalid branch name: {}", branch_name));
        }
        // Defense in depth: reject config-sourced base branches that could be
        // interpreted as git flags (anything with `--`, spaces, etc.) before
        // they ever reach the git CLI.
        if !opts.base_branch.is_empty() && !validate_branch_name(&opts.base_branch) {
            return Err(anyhow::anyhow!("Invalid base branch: {}", opts.base_branch));
        }

        let use_gwq = opts.prefer_gwq && self.gwq_available;

        let result_path = if use_gwq {
            create_with_gwq(repo_root, branch_name)?
        } else {
            let dir = if opts.worktree_dir.is_empty() {
                ".glowmux"
            } else {
                opts.worktree_dir.as_str()
            };
            create_with_git(repo_root, branch_name, dir, &opts.base_branch)?
        };

        // Always exclude the local worktree dir from git status so it
        // doesn't show up as an untracked path. Safe no-op if already there.
        ensure_glowmux_in_exclude(repo_root, &opts.worktree_dir);

        Ok(result_path)
    }

    pub fn remove(&self, worktree_path: &Path, repo_root: &Path) -> Result<()> {
        let path_str = worktree_path.to_string_lossy();
        if path_str == "/" || path_str.is_empty() {
            return Err(anyhow::anyhow!("Refusing to remove root or empty path"));
        }
        // Validate the path is under the repo root to prevent removing arbitrary dirs
        let real_root = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.to_path_buf());
        let real_path = worktree_path
            .canonicalize()
            .unwrap_or_else(|_| worktree_path.to_path_buf());
        if !real_path.starts_with(&real_root) {
            return Err(anyhow::anyhow!(
                "Refusing to remove path outside repo root: {}",
                worktree_path.display()
            ));
        }
        let output = std::process::Command::new("git")
            .args([
                "worktree",
                "remove",
                "--force",
                &worktree_path.to_string_lossy(),
            ])
            .current_dir(repo_root)
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("git worktree remove failed: {}", stderr));
        }
        Ok(())
    }

    pub fn list(&self, repo_root: &Path) -> Result<Vec<WorktreeInfo>> {
        let output = std::process::Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(repo_root)
            .output()?;
        if !output.status.success() {
            return Err(anyhow::anyhow!("git worktree list failed"));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(parse_worktree_list(&text))
    }

    pub fn check_merged(&self, worktree_path: &Path, main_branch: &str) -> bool {
        let origin_ref = format!("origin/{}", main_branch);
        std::process::Command::new("git")
            .args(["merge-base", "--is-ancestor", "HEAD", &origin_ref])
            .current_dir(worktree_path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

pub(crate) fn parse_worktree_list(text: &str) -> Vec<WorktreeInfo> {
    let mut result = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch: Option<String> = None;
    let mut is_main = false;

    for line in text.lines() {
        if let Some(path_str) = line.strip_prefix("worktree ") {
            if let Some(path) = current_path.take() {
                let branch = current_branch
                    .take()
                    .unwrap_or_else(|| "(unknown)".to_string());
                result.push(WorktreeInfo {
                    path,
                    branch,
                    is_main,
                });
            } else {
                current_branch = None;
            }
            current_path = Some(PathBuf::from(path_str));
            is_main = result.is_empty();
        } else if let Some(branch_ref) = line.strip_prefix("branch refs/heads/") {
            current_branch = Some(branch_ref.to_string());
        } else if line == "detached" {
            current_branch = Some("(detached HEAD)".to_string());
        } else if line == "bare" || line.starts_with("HEAD") {
            // is_main already set based on whether this is the first entry
        }
    }
    if let Some(path) = current_path {
        let branch = current_branch.unwrap_or_else(|| "(unknown)".to_string());
        result.push(WorktreeInfo {
            path,
            branch,
            is_main,
        });
    }
    result
}

pub fn validate_branch_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '/' || c == '_')
}

/// Probe whether the `gwq` CLI is on PATH. Returns false if it is not
/// installed, can't be executed, or doesn't respond to `--version`.
pub fn which_gwq_exists() -> bool {
    std::process::Command::new("gwq")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Create a worktree using `gwq add`. Returns the path of the new
/// worktree as reported by gwq's stdout.
fn create_with_gwq(repo_root: &Path, branch_name: &str) -> Result<PathBuf> {
    let output = std::process::Command::new("gwq")
        .args(["add", &repo_root.to_string_lossy(), branch_name])
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("gwq add failed: {}", stderr));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some(path_str) = stdout.lines().find(|l| l.starts_with('/')) {
        let path_str = path_str.trim();
        if path_str.contains('\n') || path_str.contains('\0') {
            return Err(anyhow::anyhow!("gwq returned path with invalid characters"));
        }
        let candidate = PathBuf::from(path_str);
        let real_root = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.to_path_buf());
        let real_candidate = candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.clone());
        if real_candidate.starts_with(&real_root) {
            return Ok(candidate);
        }
        return Err(anyhow::anyhow!(
            "gwq returned path outside repo root: {}",
            candidate.display()
        ));
    }
    Err(anyhow::anyhow!(
        "gwq add succeeded but produced no path in stdout"
    ))
}

/// Create a worktree under `{repo_root}/{worktree_dir}/{branch_with_slashes_underscored}`
/// using `git worktree add -b {branch} {path} [{base_branch}]`.
fn create_with_git(
    repo_root: &Path,
    branch_name: &str,
    worktree_dir: &str,
    base_branch: &str,
) -> Result<PathBuf> {
    // Reject base branch values that could be parsed as git flags (e.g.
    // anything containing "--"). validate_branch_name only allows
    // alphanumerics, '-', '/', '_' so this also blocks shell-meta etc.
    if !base_branch.is_empty() && !validate_branch_name(base_branch) {
        return Err(anyhow::anyhow!("Invalid base branch: {}", base_branch));
    }

    let worktree_path = repo_root
        .join(worktree_dir)
        .join(branch_name.replace('/', "_"));

    let mut args: Vec<String> = vec![
        "worktree".to_string(),
        "add".to_string(),
        worktree_path.to_string_lossy().to_string(),
        "-b".to_string(),
        branch_name.to_string(),
    ];
    if !base_branch.is_empty() {
        args.push(base_branch.to_string());
    }

    let output = std::process::Command::new("git")
        .args(&args)
        .current_dir(repo_root)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("git worktree add failed: {}", stderr));
    }
    Ok(worktree_path)
}

/// Append `{worktree_dir}/` to `.git/info/exclude` if it isn't already there.
/// Creates the file (and parent dir) when missing. Failures are silent — the
/// worktree itself is still usable, the user just sees the dir in `git status`.
pub fn ensure_glowmux_in_exclude(repo_root: &Path, worktree_dir: &str) {
    // Skip silently when the repo isn't a git checkout — creating .git/info
    // in a non-repo would be surprising and pollutes random directories.
    if !repo_root.join(".git").exists() {
        return;
    }

    let dir = if worktree_dir.is_empty() {
        ".glowmux"
    } else {
        worktree_dir
    };
    let entry = format!("{}/", dir.trim_end_matches('/'));

    let exclude_path = repo_root.join(".git").join("info").join("exclude");
    if let Some(parent) = exclude_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let existing = std::fs::read_to_string(&exclude_path).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == entry) {
        return;
    }
    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str("# Added by glowmux\n");
    content.push_str(&entry);
    content.push('\n');
    let _ = std::fs::write(&exclude_path, content);
}

pub async fn generate_branch_name(
    context: &str,
    config: &crate::config::AiConfig,
) -> Option<String> {
    let prompt = format!(
        "Generate a git branch name from the context below. \
         Format: feat/xxx or fix/xxx. Use only alphanumeric, hyphen, slash. \
         Return the branch name only, nothing else.\n\nContext:\n{}",
        context
    );

    let raw = match config.provider.as_str() {
        "ollama" => {
            crate::ai::invoke::invoke_ollama(
                &config.ollama.base_url,
                &config.ollama.model,
                &prompt,
                10,
            )
            .await
        }
        _ => crate::ai::invoke::invoke_claude_headless(&prompt, 10).await,
    }?;

    let sanitized: String = raw
        .trim()
        .chars()
        .filter(|&c| c.is_ascii_alphanumeric() || c == '-' || c == '/' || c == '_')
        .collect();

    if sanitized.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}

#[cfg(test)]
mod tests;
