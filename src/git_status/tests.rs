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
