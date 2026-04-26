use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneSnapshot {
    pub id: usize,
    pub cwd: PathBuf,
    pub title: String,
    pub worktree_path: Option<PathBuf>,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub name: String,
    pub cwd: PathBuf,
    pub panes: Vec<PaneSnapshot>,
    pub layout_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub version: u32,
    pub workspaces: Vec<WorkspaceSnapshot>,
    pub active_tab: usize,
}

impl SessionData {
    pub fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("glowmux: session dir create error: {}", e);
                return;
            }
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                let tmp_path = path.with_extension("json.tmp");
                if let Err(e) = std::fs::write(&tmp_path, &json) {
                    eprintln!("glowmux: session write error: {}", e);
                    return;
                }
                if let Err(e) = std::fs::rename(&tmp_path, path) {
                    eprintln!("glowmux: session rename error: {}", e);
                    let _ = std::fs::remove_file(&tmp_path);
                }
            }
            Err(e) => eprintln!("glowmux: session serialize error: {}", e),
        }
    }

    pub fn load(path: &Path) -> Option<SessionData> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn session_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("glowmux").join("session.json"))
    }
}

#[cfg(test)]
mod tests {
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
}
