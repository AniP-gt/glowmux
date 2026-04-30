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

fn temp_dir(name: &str) -> std::path::PathBuf {
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

    let joined = std::env::join_paths([std::path::PathBuf::from("."), dir.clone()]).unwrap();
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
        assert_eq!(cmd.get_program(), std::path::PathBuf::from("/tmp/custom-git"));
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
