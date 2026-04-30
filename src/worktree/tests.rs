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

#[test]
fn test_ensure_glowmux_in_exclude_creates_file() {
    let tmp = std::env::temp_dir().join(format!(
        "glowmux-test-exclude-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(tmp.join(".git").join("info")).unwrap();
    ensure_glowmux_in_exclude(&tmp, ".glowmux");
    let content = std::fs::read_to_string(tmp.join(".git/info/exclude")).unwrap();
    assert!(content.contains(".glowmux/"));
    // Idempotent: second call doesn't double-write.
    ensure_glowmux_in_exclude(&tmp, ".glowmux");
    let content2 = std::fs::read_to_string(tmp.join(".git/info/exclude")).unwrap();
    let count = content2.matches(".glowmux/").count();
    assert_eq!(
        count, 1,
        "second call should be a no-op, got {} occurrences",
        count
    );
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn test_ensure_glowmux_in_exclude_appends_to_existing() {
    let tmp = std::env::temp_dir().join(format!(
        "glowmux-test-append-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let info_dir = tmp.join(".git").join("info");
    std::fs::create_dir_all(&info_dir).unwrap();
    std::fs::write(info_dir.join("exclude"), "*.swp\nfoo.tmp").unwrap();
    ensure_glowmux_in_exclude(&tmp, ".glowmux");
    let content = std::fs::read_to_string(tmp.join(".git/info/exclude")).unwrap();
    assert!(content.contains("*.swp"));
    assert!(content.contains("foo.tmp"));
    assert!(content.contains(".glowmux/"));
    std::fs::remove_dir_all(&tmp).ok();
}
