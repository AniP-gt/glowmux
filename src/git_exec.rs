use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub fn git_binary() -> OsString {
    if let Some(bin) = std::env::var_os("GLOWMUX_GIT_BIN") {
        return bin;
    }

    for candidate in fallback_candidates() {
        if candidate.is_file() {
            return candidate.into_os_string();
        }
    }

    if let Some(bin) = resolve_from_path(std::env::var_os("PATH")) {
        return bin;
    }

    OsString::from("git")
}

pub fn git_command() -> std::process::Command {
    std::process::Command::new(git_binary())
}

fn resolve_from_path(path_var: Option<OsString>) -> Option<OsString> {
    let path_var = path_var?;

    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() || dir == Path::new(".") || !dir.is_absolute() {
            continue;
        }

        for candidate in git_binary_names().iter().map(|name| dir.join(name)) {
            if candidate.is_file() {
                return Some(candidate.into_os_string());
            }
        }
    }

    None
}

#[cfg(windows)]
fn git_binary_names() -> &'static [&'static str] {
    &["git.exe", "git.cmd", "git.bat"]
}

#[cfg(not(windows))]
fn git_binary_names() -> &'static [&'static str] {
    &["git"]
}

#[cfg(windows)]
fn fallback_candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from(r"C:\Program Files\Git\cmd\git.exe"),
        PathBuf::from(r"C:\Program Files\Git\bin\git.exe"),
    ]
}

#[cfg(not(windows))]
fn fallback_candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/usr/bin/git"),
        PathBuf::from("/opt/homebrew/bin/git"),
        PathBuf::from("/usr/local/bin/git"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("glowmux-git-exec-{name}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_from_path_skips_relative_entries() {
        let dir = temp_dir("absolute");
        let git_path = dir.join(git_binary_names()[0]);
        std::fs::write(&git_path, b"").unwrap();

        let joined = std::env::join_paths([PathBuf::from("."), dir.clone()]).unwrap();
        assert_eq!(
            resolve_from_path(Some(joined)),
            Some(git_path.into_os_string())
        );
    }

    #[test]
    fn git_binary_allows_override() {
        std::env::set_var("GLOWMUX_GIT_BIN", "/opt/homebrew/bin/git");
        assert_eq!(git_binary(), OsString::from("/opt/homebrew/bin/git"));
        std::env::remove_var("GLOWMUX_GIT_BIN");
    }

    #[test]
    fn git_command_uses_configured_binary() {
        std::env::set_var("GLOWMUX_GIT_BIN", "/tmp/custom-git");
        let cmd = git_command();
        assert_eq!(cmd.get_program(), PathBuf::from("/tmp/custom-git"));
        std::env::remove_var("GLOWMUX_GIT_BIN");
    }
}
