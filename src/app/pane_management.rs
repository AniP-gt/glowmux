use super::*;

impl App {
    pub(super) const MAX_PANES: usize = 16;

    pub(super) const MIN_PANE_WIDTH: u16 = 20;

    pub(super) const MIN_PANE_HEIGHT: u16 = 5;

    pub(super) fn new_tab(&mut self) -> Result<()> {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let name = dir_name(&cwd);
        let pane_id = self.next_pane_id;
        self.next_pane_id = self.next_pane_id.wrapping_add(1);

        let ws = Workspace::new(name, cwd, pane_id, 10, 40, self.event_tx.clone())?;
        self.workspaces.push(ws);
        self.active_tab = self.workspaces.len() - 1;
        self.on_workspace_focus_context_changed();
        Ok(())
    }

    pub(super) fn cleanup_pane_runtime_state(&mut self, pane_id: usize) {
        self.claude_monitor.remove(pane_id);
        self.pane_states.remove(&pane_id);
        self.pane_output_rings.remove(&pane_id);
        self.ai_title_requested_once.remove(&pane_id);
        self.ai_titles.remove(&pane_id);
        self.last_ai_title_request.remove(&pane_id);
        self.ai_title_in_flight.remove(&pane_id);
        self.pane_custom_titles.remove(&pane_id);
        if matches!(self.pane_rename_input, Some((id, _)) if id == pane_id) {
            self.pane_rename_input = None;
        }
        self.invalidate_git_status_for_tab(self.active_tab);
    }

    pub(super) fn close_tab(&mut self, index: usize) {
        if self.workspaces.len() <= 1 {
            return;
        }
        self.reset_pane_list_sidebar_if_active();
        let pane_ids: Vec<usize> = self.workspaces[index].panes.keys().copied().collect();
        for pane_id in pane_ids {
            self.cleanup_pane_runtime_state(pane_id);
        }
        self.workspaces[index].shutdown();
        self.workspaces.remove(index);
        if self.active_tab >= self.workspaces.len() {
            self.active_tab = self.workspaces.len() - 1;
        }
    }

    pub(super) fn toggle_file_tree(&mut self) {
        let ws = self.ws_mut();
        match (ws.sidebar_mode, ws.focus_target) {
            (SidebarMode::FileTree, FocusTarget::FileTree) => {
                ws.sidebar_mode = SidebarMode::None;
                ws.focus_target = if ws.preview.is_active() {
                    FocusTarget::Preview
                } else {
                    FocusTarget::Pane
                };
            }
            (SidebarMode::FileTree, _) => {
                ws.focus_target = FocusTarget::FileTree;
                return;
            }
            _ => {
                ws.sidebar_mode = SidebarMode::FileTree;
                ws.focus_target = FocusTarget::FileTree;
            }
        }
        self.mark_layout_change();
    }

    pub(super) fn reset_pane_list_sidebar_if_active(&mut self) {
        let was_active = {
            let ws = self.ws_mut();
            if ws.sidebar_mode == SidebarMode::PaneList {
                ws.sidebar_mode = SidebarMode::None;
                ws.focus_target = FocusTarget::Pane;
                true
            } else {
                false
            }
        };
        if was_active {
            self.mark_layout_change();
        }
    }

    pub(super) fn toggle_pane_list_sidebar(&mut self) {
        let sidebar = self.ws().sidebar_mode;
        let focus = self.ws().focus_target;
        match (sidebar, focus) {
            (SidebarMode::PaneList, FocusTarget::PaneList) => {
                self.ws_mut().sidebar_mode = SidebarMode::None;
                self.ws_mut().focus_target = FocusTarget::Pane;
                self.mark_layout_change();
            }
            (SidebarMode::PaneList, _) => {
                self.ws_mut().focus_target = FocusTarget::PaneList;
                self.dirty = true;
            }
            _ => {
                let pane_ids = self.ws().layout.collect_pane_ids();
                let focused = self.ws().focused_pane_id;
                let selected = pane_ids
                    .iter()
                    .position(|&id| id == focused)
                    .unwrap_or(0)
                    .min(pane_ids.len().saturating_sub(1));
                self.pane_list_overlay.pane_ids = pane_ids;
                self.pane_list_overlay.selected = selected;
                // Do NOT set pane_list_overlay.visible = true
                self.ws_mut().sidebar_mode = SidebarMode::PaneList;
                self.ws_mut().focus_target = FocusTarget::PaneList;
                self.mark_layout_change();
            }
        }
    }

    pub(super) fn split_focused_pane(&mut self, direction: SplitDirection) -> Result<()> {
        if self.zoomed_pane_id.is_some() {
            return Ok(());
        }
        if self.ws().layout.pane_count() >= Self::MAX_PANES {
            return Ok(());
        }

        if let Some(&(_, rect)) = self
            .ws()
            .last_pane_rects
            .iter()
            .find(|(id, _)| *id == self.ws().focused_pane_id)
        {
            match direction {
                SplitDirection::Vertical => {
                    if rect.width / 2 < Self::MIN_PANE_WIDTH {
                        return Ok(());
                    }
                }
                SplitDirection::Horizontal => {
                    if rect.height / 2 < Self::MIN_PANE_HEIGHT {
                        return Ok(());
                    }
                }
            }
        }

        let new_id = self.next_pane_id;
        self.next_pane_id = self.next_pane_id.wrapping_add(1);

        // Inherit CWD from the focused pane
        let parent_cwd = self
            .ws()
            .panes
            .get(&self.ws().focused_pane_id)
            .map(|p| p.cwd.clone());

        let pane = Pane::new_with_cwd(new_id, 10, 40, self.event_tx.clone(), parent_cwd)?;
        let ws = self.ws_mut();
        ws.panes.insert(new_id, pane);
        ws.layout.split_pane(ws.focused_pane_id, new_id, direction);
        // Focus moves to the freshly-created pane so the user can type
        // in it immediately after splitting.
        ws.focused_pane_id = new_id;

        self.on_workspace_focus_context_changed();
        self.mark_layout_change();
        Ok(())
    }

    pub(super) fn close_focused_pane(&mut self) {
        // If zoomed, restore the saved layout first so remove_pane operates on the
        // real multi-pane tree, not the single-leaf zoom overlay.
        if self.zoomed_pane_id.is_some() {
            if let Some(saved_layout) = self.pre_zoom_layout.take() {
                self.ws_mut().layout = saved_layout;
            }
            self.zoomed_pane_id = None;
        }
        let focused = self.ws().focused_pane_id;
        let ws = self.ws_mut();
        if ws.layout.pane_count() <= 1 {
            return;
        }

        let pane_ids = ws.layout.collect_pane_ids();
        let current_idx = pane_ids.iter().position(|&id| id == focused);

        ws.layout.remove_pane(focused);

        if let Some(mut pane) = ws.panes.remove(&focused) {
            pane.kill();
        }

        self.cleanup_pane_runtime_state(focused);
        let ws = self.ws_mut();

        let remaining_ids = ws.layout.collect_pane_ids();
        if let Some(idx) = current_idx {
            let new_idx = if idx >= remaining_ids.len() {
                remaining_ids.len().saturating_sub(1)
            } else {
                idx
            };
            ws.focused_pane_id = remaining_ids[new_idx];
        } else if let Some(&first) = remaining_ids.first() {
            ws.focused_pane_id = first;
        }

        self.on_workspace_focus_context_changed();
        self.mark_layout_change();

        // Refresh pane list overlay if the closed pane was tracked
        let closed_id = focused;
        if self.pane_list_overlay.pane_ids.contains(&closed_id) {
            let live_ids = self.ws().layout.collect_pane_ids();
            self.pane_list_overlay.pane_ids = live_ids;
            let new_len = self.pane_list_overlay.pane_ids.len();
            if new_len == 0 {
                self.pane_list_overlay.selected = 0;
            } else if self.pane_list_overlay.selected >= new_len {
                self.pane_list_overlay.selected = new_len - 1;
            }
        }
    }

    pub(super) fn toggle_zoom(&mut self) {
        if self.zoomed_pane_id.is_some() {
            if let Some(saved_layout) = self.pre_zoom_layout.take() {
                self.ws_mut().layout = saved_layout;
            }
            self.zoomed_pane_id = None;
            self.pre_zoom_layout = None;
        } else {
            let focused = self.ws().focused_pane_id;
            if self.ws().layout.pane_count() > 1 {
                self.pre_zoom_layout = Some(self.ws().layout.clone_layout());
                self.ws_mut().layout = LayoutNode::Leaf { pane_id: focused };
                self.zoomed_pane_id = Some(focused);
            }
        }
        self.mark_layout_change();
    }

    pub(super) fn focus_pane_in_direction(&mut self, dir: Direction) {
        let focused = self.ws().focused_pane_id;

        let Some(&(_, current_rect)) = self
            .ws()
            .last_pane_rects
            .iter()
            .find(|(id, _)| *id == focused)
        else {
            return;
        };

        let cx = current_rect.x as i32 + current_rect.width as i32 / 2;
        let cy = current_rect.y as i32 + current_rect.height as i32 / 2;

        let mut best_id: Option<usize> = None;
        let mut best_dist = i32::MAX;

        for &(pane_id, rect) in &self.ws().last_pane_rects {
            if pane_id == focused {
                continue;
            }

            let px = rect.x as i32 + rect.width as i32 / 2;
            let py = rect.y as i32 + rect.height as i32 / 2;

            let is_candidate = match dir {
                Direction::Left => {
                    px < cx && rect.x as i32 + rect.width as i32 <= current_rect.x as i32 + 2
                }
                Direction::Right => {
                    px > cx
                        && rect.x as i32 >= current_rect.x as i32 + current_rect.width as i32 - 2
                }
                Direction::Up => {
                    py < cy && rect.y as i32 + rect.height as i32 <= current_rect.y as i32 + 2
                }
                Direction::Down => {
                    py > cy
                        && rect.y as i32 >= current_rect.y as i32 + current_rect.height as i32 - 2
                }
            };

            if !is_candidate {
                continue;
            }

            let dist = (px - cx).abs() + (py - cy).abs();
            if dist < best_dist {
                best_dist = dist;
                best_id = Some(pane_id);
            }
        }

        if let Some(new_id) = best_id {
            self.dismiss_done_on_focus(new_id);
            self.ws_mut().focused_pane_id = new_id;
            self.on_workspace_focus_context_changed();
        }
    }

    /// Cycle focus forward: FileTree → Preview → Pane1 → Pane2 → ... → FileTree
    pub(super) fn focus_next_pane(&mut self) {
        let ws = self.ws_mut();
        let ids = ws.layout.collect_pane_ids();
        let tree_visible = ws.sidebar_mode == SidebarMode::FileTree;
        let preview_active = ws.preview.is_active();
        let _swapped = false; // preview position doesn't affect focus order

        let mut closed_pane_list_sidebar = false;
        match ws.focus_target {
            FocusTarget::FileTree => {
                // File tree → preview (if active) or first pane
                if preview_active {
                    ws.focus_target = FocusTarget::Preview;
                } else {
                    ws.focus_target = FocusTarget::Pane;
                }
            }
            FocusTarget::Preview => {
                // Preview → first pane
                ws.focus_target = FocusTarget::Pane;
            }
            FocusTarget::Pane => {
                if let Some(idx) = ids.iter().position(|&id| id == ws.focused_pane_id) {
                    if idx + 1 < ids.len() {
                        ws.focused_pane_id = ids[idx + 1];
                    } else if tree_visible {
                        ws.focus_target = FocusTarget::FileTree;
                    } else if preview_active {
                        ws.focus_target = FocusTarget::Preview;
                    } else {
                        ws.focused_pane_id = ids[0];
                    }
                }
            }
            FocusTarget::PaneList => {
                ws.focus_target = FocusTarget::Pane;
                ws.sidebar_mode = SidebarMode::None;
                closed_pane_list_sidebar = true;
            }
        }
        if closed_pane_list_sidebar {
            self.mark_layout_change();
        }
        let new_id = self.ws().focused_pane_id;
        self.dismiss_done_on_focus(new_id);
        if self.ws().focus_target == FocusTarget::Pane {
            self.on_workspace_focus_context_changed();
        } else {
            self.dirty = true;
        }
    }

    /// Cycle focus backward
    pub(super) fn focus_prev_pane(&mut self) {
        let ws = self.ws_mut();
        let ids = ws.layout.collect_pane_ids();
        let tree_visible = ws.sidebar_mode == SidebarMode::FileTree;
        let preview_active = ws.preview.is_active();

        let mut closed_pane_list_sidebar = false;
        match ws.focus_target {
            FocusTarget::FileTree => {
                // File tree → last pane
                ws.focus_target = FocusTarget::Pane;
                if let Some(&last) = ids.last() {
                    ws.focused_pane_id = last;
                }
            }
            FocusTarget::Preview => {
                // Preview → file tree (if visible) or last pane
                if tree_visible {
                    ws.focus_target = FocusTarget::FileTree;
                } else {
                    ws.focus_target = FocusTarget::Pane;
                    if let Some(&last) = ids.last() {
                        ws.focused_pane_id = last;
                    }
                }
            }
            FocusTarget::Pane => {
                if let Some(idx) = ids.iter().position(|&id| id == ws.focused_pane_id) {
                    if idx > 0 {
                        ws.focused_pane_id = ids[idx - 1];
                    } else if preview_active {
                        ws.focus_target = FocusTarget::Preview;
                    } else if tree_visible {
                        ws.focus_target = FocusTarget::FileTree;
                    } else {
                        ws.focused_pane_id = ids[ids.len() - 1];
                    }
                }
            }
            FocusTarget::PaneList => {
                ws.focus_target = FocusTarget::Pane;
                ws.sidebar_mode = SidebarMode::None;
                closed_pane_list_sidebar = true;
            }
        }
        if closed_pane_list_sidebar {
            self.mark_layout_change();
        }
        let new_id = self.ws().focused_pane_id;
        self.dismiss_done_on_focus(new_id);
        if self.ws().focus_target == FocusTarget::Pane {
            self.on_workspace_focus_context_changed();
        } else {
            self.dirty = true;
        }
    }

    /// Scroll a pane based on scrollbar click position.
    pub(super) fn scroll_pane_to_click(&self, pane_id: usize, click_row: u16, inner: &Rect) {
        if let Some(pane) = self.ws().panes.get(&pane_id) {
            let (_, total_lines) = pane.scrollbar_info();
            let visible_rows = inner.height as usize;
            if total_lines <= visible_rows {
                return;
            }
            let max_scroll = total_lines.saturating_sub(visible_rows);
            // click_row relative to inner area: top = max scroll, bottom = 0
            let relative_y = click_row.saturating_sub(inner.y) as f32;
            let ratio = relative_y / inner.height.max(1) as f32;
            let target_scroll = ((1.0 - ratio) * max_scroll as f32) as usize;
            let mut parser = pane.parser.lock().unwrap_or_else(|e| e.into_inner());
            parser.screen_mut().set_scrollback(target_scroll);
        }
    }
}
