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
mod tests;
