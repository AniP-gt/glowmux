use super::*;

#[test]
fn test_detect_shell_returns_valid_path() {
    let shell = detect_shell();
    assert!(
        !shell.as_os_str().is_empty(),
        "Shell path should not be empty"
    );
}

#[cfg(windows)]
#[test]
fn test_detect_shell_windows_returns_exe() {
    let shell = detect_shell();
    let ext = shell
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase());
    assert_eq!(ext.as_deref(), Some("exe"), "Windows shell should be .exe");
}

#[cfg(not(windows))]
#[test]
fn test_detect_shell_unix_uses_shell_env() {
    let shell = detect_shell();
    if let Ok(env_shell) = std::env::var("SHELL") {
        assert_eq!(
            shell,
            PathBuf::from(&env_shell),
            "Should use $SHELL env var"
        );
    }
}
