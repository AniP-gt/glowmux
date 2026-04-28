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

pub fn delta_binary() -> Option<OsString> {
    if let Some(bin) = std::env::var_os("GLOWMUX_DELTA_BIN") {
        return Some(bin);
    }

    resolve_named_binary(std::env::var_os("PATH"), delta_binary_names())
}

fn resolve_from_path(path_var: Option<OsString>) -> Option<OsString> {
    resolve_named_binary(path_var, git_binary_names())
}

fn resolve_named_binary(path_var: Option<OsString>, names: &[&str]) -> Option<OsString> {
    let path_var = path_var?;

    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() || dir == Path::new(".") || !dir.is_absolute() {
            continue;
        }

        for candidate in names.iter().map(|name| dir.join(name)) {
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
fn delta_binary_names() -> &'static [&'static str] {
    &["delta.exe", "delta.cmd", "delta.bat"]
}

#[cfg(not(windows))]
fn delta_binary_names() -> &'static [&'static str] {
    &["delta"]
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
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_git_bin_override<T>(value: &str, f: impl FnOnce() -> T) -> T {
        let _guard = env_lock().lock().unwrap();
        let previous = std::env::var_os("GLOWMUX_GIT_BIN");
        std::env::set_var("GLOWMUX_GIT_BIN", value);
        let result = f();
        if let Some(previous) = previous {
            std::env::set_var("GLOWMUX_GIT_BIN", previous);
        } else {
            std::env::remove_var("GLOWMUX_GIT_BIN");
        }
        result
    }

    fn with_delta_bin_override<T>(value: &str, f: impl FnOnce() -> T) -> T {
        let _guard = env_lock().lock().unwrap();
        let previous = std::env::var_os("GLOWMUX_DELTA_BIN");
        std::env::set_var("GLOWMUX_DELTA_BIN", value);
        let result = f();
        if let Some(previous) = previous {
            std::env::set_var("GLOWMUX_DELTA_BIN", previous);
        } else {
            std::env::remove_var("GLOWMUX_DELTA_BIN");
        }
        result
    }

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
        with_git_bin_override("/opt/homebrew/bin/git", || {
            assert_eq!(git_binary(), OsString::from("/opt/homebrew/bin/git"));
        });
    }

    #[test]
    fn git_command_uses_configured_binary() {
        with_git_bin_override("/tmp/custom-git", || {
            let cmd = git_command();
            assert_eq!(cmd.get_program(), PathBuf::from("/tmp/custom-git"));
        });
    }

    #[test]
    fn delta_binary_allows_override() {
        with_delta_bin_override("/opt/homebrew/bin/delta", || {
            assert_eq!(
                delta_binary(),
                Some(OsString::from("/opt/homebrew/bin/delta"))
            );
        });
    }
}
