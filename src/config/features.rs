use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct FeaturesConfig {
    pub ai_title: bool,
    pub ai_worktree_name: bool,
    pub auto_worktree: bool,
    pub session_restore: bool,
    pub feature_toggle_ui: bool,
    pub status_dot: bool,
    pub status_bg_color: bool,
    pub status_bar: bool,
    pub zoom: bool,
    // Spec v5 additions
    pub worktree: bool,
    pub worktree_ai_name: bool,
    pub file_tree: bool,
    pub file_preview: bool,
    pub diff_preview: bool,
    pub cd_tracking: bool,
    pub responsive_layout: bool,
    pub session_persist: bool,
    pub context_copy: bool,
    pub layout_picker: bool,
    pub startup_panes: bool,
}

impl Default for FeaturesConfig {
    fn default() -> Self {
        Self {
            ai_title: true,
            ai_worktree_name: false,
            auto_worktree: false,
            session_restore: false,
            feature_toggle_ui: false,
            status_dot: true,
            status_bg_color: true,
            status_bar: true,
            zoom: true,
            worktree: true,
            worktree_ai_name: false,
            file_tree: true,
            file_preview: true,
            diff_preview: false,
            cd_tracking: true,
            responsive_layout: true,
            session_persist: false,
            context_copy: true,
            layout_picker: true,
            startup_panes: true,
        }
    }
}

impl FeaturesConfig {
    pub fn get_by_key(&self, key: &str) -> bool {
        match key {
            "status_dot" => self.status_dot,
            "status_bg_color" => self.status_bg_color,
            "status_bar" => self.status_bar,
            "ai_title" => self.ai_title,
            "zoom" => self.zoom,
            "worktree" => self.worktree,
            "worktree_ai_name" => self.worktree_ai_name,
            "file_tree" => self.file_tree,
            "file_preview" => self.file_preview,
            "diff_preview" => self.diff_preview,
            "cd_tracking" => self.cd_tracking,
            "responsive_layout" => self.responsive_layout,
            "session_persist" => self.session_persist,
            "context_copy" => self.context_copy,
            "layout_picker" => self.layout_picker,
            "startup_panes" => self.startup_panes,
            _ => false,
        }
    }

    pub fn set_by_key(&mut self, key: &str, value: bool) {
        match key {
            "status_dot" => self.status_dot = value,
            "status_bg_color" => self.status_bg_color = value,
            "status_bar" => self.status_bar = value,
            "ai_title" => self.ai_title = value,
            "zoom" => self.zoom = value,
            "worktree" => self.worktree = value,
            "worktree_ai_name" => self.worktree_ai_name = value,
            "file_tree" => self.file_tree = value,
            "file_preview" => self.file_preview = value,
            "diff_preview" => self.diff_preview = value,
            "cd_tracking" => self.cd_tracking = value,
            "responsive_layout" => self.responsive_layout = value,
            "session_persist" => self.session_persist = value,
            "context_copy" => self.context_copy = value,
            "layout_picker" => self.layout_picker = value,
            "startup_panes" => self.startup_panes = value,
            _ => {}
        }
    }
}
