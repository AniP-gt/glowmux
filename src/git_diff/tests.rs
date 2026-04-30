use super::*;
use std::ffi::OsString;
use std::fs;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    ENV_LOCK.get_or_init(|| Mutex::new(()))
}

fn with_delta_bin_override<T>(value: OsString, f: impl FnOnce() -> T) -> T {
    let _guard = env_lock().lock().unwrap();
    let previous = std::env::var_os("GLOWMUX_DELTA_BIN");
    std::env::set_var("GLOWMUX_DELTA_BIN", &value);
    let result = f();
    if let Some(previous) = previous {
        std::env::set_var("GLOWMUX_DELTA_BIN", previous);
    } else {
        std::env::remove_var("GLOWMUX_DELTA_BIN");
    }
    result
}

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

#[test]
fn parse_ansi_line_returns_spans() {
    let spans = parse_ansi_line("\u{1b}[31m-old\u{1b}[0m");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].text, "-old");
    assert_eq!(spans[0].fg, Some(ratatui::style::Color::Indexed(1)));
}

#[test]
fn strip_ansi_sequences_removes_color_codes() {
    assert_eq!(strip_ansi_sequences("\u{1b}[31m-old\u{1b}[0m\n"), "-old\n");
}

#[test]
fn load_delta_result_falls_back_when_delta_output_is_not_diff_like() {
    let temp_dir =
        std::env::temp_dir().join(format!("glowmux-delta-non-diff-{}", std::process::id()));
    fs::create_dir_all(&temp_dir).unwrap();
    let script_path = temp_dir.join("fake-delta.sh");
    fs::write(&script_path, b"#!/bin/sh\nprintf 'not a diff\n'").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    with_delta_bin_override(script_path.into_os_string(), || {
        assert!(matches!(
            load_delta_result(b"diff --git a/x b/x\n+ok\n", true),
            Some(None)
        ));
    });
}
