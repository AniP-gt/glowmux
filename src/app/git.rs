use super::*;

impl App {
    pub(super) fn focused_pane_git_cwd(&self) -> Option<PathBuf> {
        self.ws()
            .panes
            .get(&self.ws().focused_pane_id)
            .map(|pane| pane.pane_cwd())
    }

    pub(super) fn selected_file_path(&self) -> Option<PathBuf> {
        self.ws()
            .file_tree
            .selected_entry()
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.path.clone())
    }

    pub(super) fn invalidate_git_status_for_tab(&mut self, tab_idx: usize) {
        if let Some(ws) = self.workspaces.get_mut(tab_idx) {
            ws.git_status = None;
        }
    }

    pub(super) fn refresh_git_status_for_tab(&mut self, tab_idx: usize, force: bool) {
        let Some(git_cwd) = self.workspaces.get(tab_idx).and_then(|ws| {
            ws.panes
                .get(&ws.focused_pane_id)
                .map(|pane| pane.pane_cwd())
        }) else {
            return;
        };

        let should_refresh = self
            .workspaces
            .get(tab_idx)
            .and_then(|ws| ws.git_status.as_ref())
            .map(|snapshot| force || snapshot.is_stale(Duration::from_secs(2)))
            .unwrap_or(true);

        if !should_refresh {
            return;
        }

        let snapshot = crate::git_status::collect_snapshot(&git_cwd);
        if let Some(ws) = self.workspaces.get_mut(tab_idx) {
            ws.git_status = snapshot;
        }
    }

    pub(super) fn refresh_git_status_for_active_workspace(&mut self, force: bool) {
        let active_tab = self.active_tab;
        self.refresh_git_status_for_tab(active_tab, force);
    }

    pub fn refresh_git_status_for_render(&mut self, force: bool) {
        self.refresh_git_status_for_active_workspace(force);
    }

    pub(super) fn on_workspace_focus_context_changed(&mut self) {
        if let Some(git_cwd) = self.focused_pane_git_cwd() {
            let current_root = self.ws().file_tree.root_path.clone();
            let current_name = self.ws().name.clone();
            let current_cwd = self.ws().cwd.clone();
            let target_name = dir_name(&git_cwd);
            let show_hidden = self.ws().file_tree.show_hidden;
            if current_root != git_cwd {
                self.ws_mut().file_tree = FileTree::new(git_cwd.clone());
                if self.ws().file_tree.show_hidden != show_hidden {
                    self.ws_mut().file_tree.toggle_hidden();
                }
            }
            if current_cwd != git_cwd {
                self.ws_mut().cwd = git_cwd.clone();
            }
            if current_name != target_name {
                self.ws_mut().name = target_name;
            }
        }
        self.refresh_git_status_for_active_workspace(true);
        self.dirty = true;
    }
}
