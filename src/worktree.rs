use std::path::{Path, PathBuf};

use anyhow::Result;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
    pub is_main: bool,
}

pub struct WorktreeManager {
    pub gwq_available: bool,
}

impl WorktreeManager {
    pub fn new() -> Self {
        let gwq_available = std::process::Command::new("gwq")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        Self { gwq_available }
    }

    pub fn create(&self, repo_root: &Path, branch_name: &str) -> Result<PathBuf> {
        if !validate_branch_name(branch_name) {
            return Err(anyhow::anyhow!("Invalid branch name: {}", branch_name));
        }

        if self.gwq_available {
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
                // Reject paths with newlines or null bytes before any further use
                if path_str.contains('\n') || path_str.contains('\0') {
                    return Err(anyhow::anyhow!("gwq returned path with invalid characters"));
                }
                let candidate = PathBuf::from(path_str);
                // Validate the returned path is under the repo root
                let real_root = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());
                let real_candidate = candidate.canonicalize().unwrap_or_else(|_| candidate.clone());
                if real_candidate.starts_with(&real_root) {
                    return Ok(candidate);
                }
                return Err(anyhow::anyhow!(
                    "gwq returned path outside repo root: {}",
                    candidate.display()
                ));
            }
            return Err(anyhow::anyhow!(
                "gwq add succeeded but produced no path in stdout"
            ));
        }

        let worktree_path = repo_root
            .join(".glowmux-worktrees")
            .join(branch_name.replace('/', "_"));

        let output = std::process::Command::new("git")
            .args([
                "worktree",
                "add",
                &worktree_path.to_string_lossy(),
                "-b",
                branch_name,
            ])
            .current_dir(repo_root)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("git worktree add failed: {}", stderr));
        }
        Ok(worktree_path)
    }

    pub fn remove(&self, worktree_path: &Path, repo_root: &Path) -> Result<()> {
        let path_str = worktree_path.to_string_lossy();
        if path_str == "/" || path_str.is_empty() {
            return Err(anyhow::anyhow!("Refusing to remove root or empty path"));
        }
        // Validate the path is under the repo root to prevent removing arbitrary dirs
        let real_root = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());
        let real_path = worktree_path.canonicalize().unwrap_or_else(|_| worktree_path.to_path_buf());
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

fn parse_worktree_list(text: &str) -> Vec<WorktreeInfo> {
    let mut result = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch: Option<String> = None;
    let mut is_main = false;

    for line in text.lines() {
        if let Some(path_str) = line.strip_prefix("worktree ") {
            if let Some(path) = current_path.take() {
                let branch = current_branch.take().unwrap_or_else(|| "(unknown)".to_string());
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
            crate::ai_invoke::invoke_ollama(
                &config.ollama.base_url,
                &config.ollama.model,
                &prompt,
                10,
            )
            .await
        }
        _ => crate::ai_invoke::invoke_claude_headless(&prompt, 10).await,
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
mod tests {
    use super::*;

    #[test]
    fn test_validate_branch_name() {
        assert!(validate_branch_name("feat/my-branch"));
        assert!(validate_branch_name("fix_something"));
        assert!(!validate_branch_name(""));
        assert!(!validate_branch_name("has space"));
        assert!(!validate_branch_name("special@char"));
    }

    #[test]
    fn test_parse_worktree_list() {
        let input = "\
worktree /home/user/project
branch refs/heads/main
HEAD abc123

worktree /home/user/project-feat
branch refs/heads/feat/new-thing
";
        let result = parse_worktree_list(input);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].branch, "main");
        assert!(result[0].is_main);
        assert_eq!(result[1].branch, "feat/new-thing");
        assert!(!result[1].is_main);
    }
}
