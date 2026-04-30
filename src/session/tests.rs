use super::*;

#[test]
fn test_session_roundtrip() {
    let data = SessionData {
        version: 1,
        workspaces: vec![WorkspaceSnapshot {
            name: "test".to_string(),
            cwd: PathBuf::from("/tmp"),
            panes: vec![PaneSnapshot {
                id: 1,
                cwd: PathBuf::from("/tmp"),
                title: "shell".to_string(),
                worktree_path: None,
                branch: None,
            }],
            layout_mode: "Auto".to_string(),
        }],
        active_tab: 0,
    };
    let json = serde_json::to_string(&data).unwrap();
    let loaded: SessionData = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.workspaces.len(), 1);
    assert_eq!(loaded.workspaces[0].panes[0].id, 1);
}
