use super::*;

#[test]
fn test_scan_directory_skips_hidden() {
    let entries = scan_directory_filtered(Path::new("."), 0, 1, false);
    for entry in &entries {
        assert!(
            !entry.name.starts_with('.'),
            "Hidden entry should be skipped: {}",
            entry.name
        );
    }
}

#[test]
fn test_scan_directory_skips_git() {
    let entries = scan_directory_filtered(Path::new("."), 0, 1, true);
    for entry in &entries {
        assert!(entry.name != ".git", ".git should always be skipped");
    }
}

#[test]
fn test_scan_directory_shows_dotfiles_when_enabled() {
    let entries = scan_directory_filtered(Path::new("."), 0, 1, true);
    let has_dotfile = entries.iter().any(|e| e.name.starts_with('.'));
    // Project has .claude, .gitignore, etc.
    assert!(has_dotfile, "Should show dotfiles when show_hidden=true");
}

#[test]
fn test_scan_directory_dirs_before_files() {
    let entries = scan_directory_filtered(Path::new("."), 0, 1, false);
    let mut seen_file = false;
    for entry in &entries {
        if !entry.is_dir {
            seen_file = true;
        }
        if entry.is_dir && seen_file {
            panic!("Directory {} found after files", entry.name);
        }
    }
}

#[test]
fn test_file_tree_navigation() {
    let mut tree = FileTree::new(PathBuf::from("."));
    let initial = tree.selected_index;
    assert_eq!(initial, 0);

    if tree.visible_entries().len() > 1 {
        tree.move_down();
        assert_eq!(tree.selected_index, 1);
        tree.move_up();
        assert_eq!(tree.selected_index, 0);
    }

    // Moving up at 0 should stay at 0
    tree.move_up();
    assert_eq!(tree.selected_index, 0);
}
