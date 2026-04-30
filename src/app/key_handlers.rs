use super::*;

impl App {
    pub(super) fn key_matches(&self, key: crossterm::event::KeyEvent, binding: &str) -> bool {
        crate::keybinding::parse_keybinding(binding)
            .map(|(m, c)| key.modifiers == m && key.code == c)
            .unwrap_or(false)
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        // Rename mode — swallow all input until Enter/Esc.
        if self.rename_input.is_some() {
            return Ok(self.handle_rename_key(key));
        }
        if self.pane_rename_input.is_some() {
            return Ok(self.handle_pane_rename_key(key));
        }

        // Settings panel dialog
        if self.settings_panel.visible {
            return self.handle_settings_panel_key(key);
        }

        // Feature toggle dialog
        if self.feature_toggle.visible {
            return self.handle_feature_toggle_key(key);
        }

        // Layout picker mode
        if self.layout_picker.visible {
            return self.handle_layout_picker_key(key);
        }

        // Pane create dialog
        if self.pane_create_dialog.visible {
            return self.handle_pane_create_key(key);
        }
        // Close confirm dialog
        if self.close_confirm_dialog.visible {
            return self.handle_close_confirm_key(key);
        }
        // Worktree cleanup dialog
        if self
            .worktree_cleanup_dialog
            .as_ref()
            .is_some_and(|d| d.visible)
        {
            return self.handle_worktree_cleanup_key(key);
        }

        // Copy mode modal
        if self.copy_mode.is_some() {
            return self.handle_copy_mode_key(key);
        }

        // Pane list overlay modal
        if self.pane_list_overlay.visible {
            return self.handle_pane_list_key(key);
        }

        // File tree action popup modal
        if self.filetree_action_popup.visible {
            return self.handle_filetree_action_popup_key(key);
        }

        // Prefix key handling
        let prefix_key = crate::keybinding::parse_keybinding(&self.config.keybindings.prefix);
        if let Some((prefix_mods, prefix_code)) = prefix_key {
            if !self.prefix_active {
                if key.modifiers == prefix_mods && key.code == prefix_code {
                    self.prefix_active = true;
                    self.dirty = true;
                    return Ok(true);
                }
            } else {
                self.prefix_active = false;
                self.dirty = true;
                if key.modifiers == prefix_mods && key.code == prefix_code {
                    // Prefix pressed twice: fall through to PTY passthrough
                } else if self.key_matches(key, &self.config.keybindings.quit.clone())
                    || (key.modifiers == KeyModifiers::NONE && key.code == KeyCode::Char('q'))
                {
                    self.should_quit = true;
                    return Ok(true);
                } else if self.key_matches(key, &self.config.keybindings.layout_cycle.clone())
                    || (key.modifiers == KeyModifiers::NONE && key.code == KeyCode::Char(' '))
                {
                    self.cycle_layout_mode();
                    return Ok(true);
                } else if key.code == KeyCode::Char('[') && key.modifiers == KeyModifiers::NONE {
                    self.enter_copy_mode();
                    return Ok(true);
                } else if key.code == KeyCode::Char('w') && key.modifiers == KeyModifiers::NONE {
                    self.open_pane_list_overlay();
                    return Ok(true);
                } else if key.code == KeyCode::Char('e') && key.modifiers == KeyModifiers::NONE {
                    // Prefix + e: horizontal split
                    self.split_focused_pane(SplitDirection::Horizontal)?;
                    return Ok(true);
                } else if key.code == KeyCode::Char('d') && key.modifiers == KeyModifiers::NONE {
                    // Prefix + d: vertical split
                    self.split_focused_pane(SplitDirection::Vertical)?;
                    return Ok(true);
                } else if key.code == KeyCode::Char('x') && key.modifiers == KeyModifiers::NONE {
                    if self.ws().focus_target == FocusTarget::Preview {
                        self.preview_zoomed = false;
                        self.ws_mut().preview.close();
                        self.ws_mut().focus_target = FocusTarget::Pane;
                        return Ok(true);
                    }
                    let multi_pane = self.ws().layout.pane_count() > 1;
                    let multi_tab = self.workspaces.len() > 1;
                    if multi_pane || multi_tab {
                        let pane_id = self.ws().focused_pane_id;
                        if self.config.worktree.close_confirm {
                            let worktree_path = self
                                .ws()
                                .panes
                                .get(&pane_id)
                                .and_then(|p| p.worktree_path.clone());
                            self.close_confirm_dialog = CloseConfirmDialog {
                                visible: true,
                                pane_id,
                                worktree_path,
                                focused: CloseConfirmFocus::No,
                            };
                            self.dirty = true;
                        } else if multi_pane {
                            self.close_focused_pane();
                        } else {
                            self.close_tab(self.active_tab);
                        }
                    }
                    return Ok(true);
                } else if key.code == KeyCode::Char('t') && key.modifiers == KeyModifiers::NONE {
                    // Prefix + t: new tab
                    self.new_tab()?;
                    return Ok(true);
                } else if key.code == KeyCode::Char('n') && key.modifiers == KeyModifiers::NONE {
                    // Prefix + n: open pane create dialog
                    let agent_labels: Vec<String> = self
                        .config
                        .multi_ai
                        .agents
                        .iter()
                        .map(|a| format!("{} ({})", a.name, a.command))
                        .collect();
                    let agent_checks = vec![false; self.config.multi_ai.agents.len()];
                    self.pane_create_dialog = PaneCreateDialog {
                        visible: true,
                        worktree_enabled: self.config.worktree.auto_create,
                        agent: self.config.startup.default_agent.clone(),
                        focused_field: PaneCreateField::BranchName,
                        base_branch: self.config.worktree.base_branch.clone(),
                        agent_checks,
                        agent_labels,
                        ..Default::default()
                    };
                    self.dirty = true;
                    return Ok(true);
                } else if key.code == KeyCode::Char('h') && key.modifiers == KeyModifiers::NONE {
                    if self.ws().focus_target == FocusTarget::Pane {
                        self.focus_pane_in_direction(Direction::Left);
                    }
                    return Ok(true);
                } else if key.code == KeyCode::Char('l') && key.modifiers == KeyModifiers::NONE {
                    if self.ws().focus_target == FocusTarget::Pane {
                        self.focus_pane_in_direction(Direction::Right);
                    }
                    return Ok(true);
                } else if key.code == KeyCode::Char('k') && key.modifiers == KeyModifiers::NONE {
                    if self.ws().focus_target == FocusTarget::Pane {
                        self.focus_pane_in_direction(Direction::Up);
                    }
                    return Ok(true);
                } else if key.code == KeyCode::Char('j') && key.modifiers == KeyModifiers::NONE {
                    if self.ws().focus_target == FocusTarget::Pane {
                        self.focus_pane_in_direction(Direction::Down);
                    }
                    return Ok(true);
                } else if key.code == KeyCode::Char('p') && key.modifiers == KeyModifiers::NONE {
                    // Prefix + p: toggle pane list sidebar
                    self.toggle_pane_list_sidebar();
                    return Ok(true);
                } else if key.code == KeyCode::Char('f') && key.modifiers == KeyModifiers::NONE {
                    // Prefix + f: toggle file tree
                    self.toggle_file_tree();
                    return Ok(true);
                } else if key.code == KeyCode::Char('r') && key.modifiers == KeyModifiers::NONE {
                    // Prefix + r: tab rename
                    if self.pane_rename_input.is_none() {
                        self.rename_input = Some(String::new());
                        if !self.status_bar_visible {
                            self.mark_layout_change();
                        }
                    }
                    return Ok(true);
                } else if key.code == KeyCode::Char('R')
                    && key.modifiers.contains(KeyModifiers::SHIFT)
                {
                    // Prefix + R: pane rename
                    if self.rename_input.is_none() {
                        let target_id = self.zoomed_pane_id.unwrap_or(self.ws().focused_pane_id);
                        self.pane_rename_input = Some((target_id, String::new()));
                        if !self.status_bar_visible {
                            self.mark_layout_change();
                        }
                    }
                    return Ok(true);
                } else if key.code == KeyCode::Char('z') && key.modifiers == KeyModifiers::NONE {
                    if self.ws().focus_target != FocusTarget::Preview {
                        self.toggle_zoom();
                    }
                    return Ok(true);
                } else if key.code == KeyCode::Char(',') && key.modifiers == KeyModifiers::NONE {
                    // Prefix + ,: settings panel
                    self.settings_panel.visible = true;
                    self.settings_panel.selected = 0;
                    self.settings_panel.editing = false;
                    self.settings_panel.edit_buffer.clear();
                    self.dirty = true;
                    return Ok(true);
                } else if key.code == KeyCode::Char('l') && key.modifiers == KeyModifiers::CONTROL {
                    // Prefix + ctrl+l: layout picker (mirrors direct ctrl+l)
                    if self.ws().layout.pane_count() > 1 {
                        self.layout_picker.visible = true;
                        self.layout_picker.selected = 0;
                    }
                    return Ok(true);
                } else if key.code == KeyCode::Tab && key.modifiers == KeyModifiers::NONE {
                    // Prefix + Tab: next tab
                    if !self.workspaces.is_empty() {
                        self.reset_pane_list_sidebar_if_active();
                        self.active_tab = (self.active_tab + 1) % self.workspaces.len();
                        self.on_workspace_focus_context_changed();
                    }
                    return Ok(true);
                } else if key.code == KeyCode::BackTab && key.modifiers == KeyModifiers::SHIFT {
                    // Prefix + Shift+Tab: previous tab
                    if !self.workspaces.is_empty() {
                        self.reset_pane_list_sidebar_if_active();
                        self.active_tab = if self.active_tab == 0 {
                            self.workspaces.len() - 1
                        } else {
                            self.active_tab - 1
                        };
                        self.on_workspace_focus_context_changed();
                    }
                    return Ok(true);
                } else if key.code == KeyCode::Char('a') && key.modifiers == KeyModifiers::NONE {
                    // Prefix + a: toggle AI title generation
                    self.ai_title_enabled = !self.ai_title_enabled;
                    self.config.features.ai_title = self.ai_title_enabled;
                    self.dirty = true;
                    return Ok(true);
                } else if key.code == KeyCode::Char('s') && key.modifiers == KeyModifiers::NONE {
                    // Prefix + s: toggle status bar
                    self.status_bar_visible = !self.status_bar_visible;
                    self.mark_layout_change();
                    return Ok(true);
                } else {
                    // Unknown prefix combo: consume the key silently
                    return Ok(true);
                }
            }
        }

        // Quit (configurable, default Ctrl+Q)
        if self.key_matches(key, &self.config.keybindings.quit) {
            self.should_quit = true;
            return Ok(true);
        }

        // Tab rename (configurable, default Alt+R)
        if self.key_matches(key, &self.config.keybindings.tab_rename)
            && self.pane_rename_input.is_none()
        {
            self.rename_input = Some(String::new());
            if !self.status_bar_visible {
                self.mark_layout_change();
            }
            return Ok(true);
        }

        // Pane rename (configurable, default Alt+Shift+R)
        if self.key_matches(key, &self.config.keybindings.pane_rename)
            && self.rename_input.is_none()
        {
            let target_id = self
                .zoomed_pane_id
                .unwrap_or(self.ws().focused_pane_id);
            self.pane_rename_input = Some((target_id, String::new()));
            if !self.status_bar_visible {
                self.mark_layout_change();
            }
            return Ok(true);
        }

        // Ctrl+C — if text is selected, copy to clipboard instead of sending SIGINT
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
            if let Some(ref sel) = self.selection.clone() {
                let (sr, sc, er, ec) = sel.normalized();
                if sr != er || sc != ec {
                    let text = match sel.target {
                        SelectionTarget::Pane(pane_id) => self
                            .ws()
                            .panes
                            .get(&pane_id)
                            .map(|p| extract_selected_text(p, sr, sc, er, ec))
                            .unwrap_or_default(),
                        SelectionTarget::Preview => {
                            extract_preview_selected_text(&self.ws().preview, sr, sc, er, ec)
                        }
                    };
                    if !text.is_empty() {
                        self.copy_to_clipboard(&text);
                    }
                    self.selection = None;
                    return Ok(true);
                }
            }
            // No selection — fall through to forward Ctrl+C to PTY
        }

        // Clipboard copy — focused pane's visible content (configurable, default Ctrl+Y)
        if self.key_matches(key, &self.config.keybindings.clipboard_copy) {
            let content = {
                let pane_id = self.ws().focused_pane_id;
                self.ws()
                    .panes
                    .get(&pane_id)
                    .map(|p| {
                        p.parser
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .screen()
                            .contents()
                    })
                    .unwrap_or_default()
            };
            if content.trim().is_empty() {
                self.status_flash =
                    Some(("No content to copy".to_string(), std::time::Instant::now()));
            } else {
                self.copy_to_clipboard(&content);
                let lines = content.lines().count();
                self.status_flash = Some((
                    format!("Copied ({} lines)", lines),
                    std::time::Instant::now(),
                ));
            }
            self.dirty = true;
            return Ok(true);
        }

        // New tab (configurable, default Ctrl+T)
        if self.key_matches(key, &self.config.keybindings.tab_new) {
            self.new_tab()?;
            return Ok(true);
        }

        // Next tab (configurable, default Alt+Right)
        if self.key_matches(key, &self.config.keybindings.tab_next) {
            if !self.workspaces.is_empty() {
                self.reset_pane_list_sidebar_if_active();
                self.active_tab = (self.active_tab + 1) % self.workspaces.len();
                self.on_workspace_focus_context_changed();
            }
            return Ok(true);
        }

        // Previous tab (configurable, default Alt+Left)
        if self.key_matches(key, &self.config.keybindings.tab_prev) {
            if !self.workspaces.is_empty() {
                self.reset_pane_list_sidebar_if_active();
                self.active_tab = if self.active_tab == 0 {
                    self.workspaces.len() - 1
                } else {
                    self.active_tab - 1
                };
                self.on_workspace_focus_context_changed();
            }
            return Ok(true);
        }

        // Alt+S — toggle status bar
        if key.modifiers == KeyModifiers::ALT
            && matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S'))
        {
            self.status_bar_visible = !self.status_bar_visible;
            self.mark_layout_change();
            return Ok(true);
        }

        // Toggle pane zoom (configurable, default Alt+Z)
        // When the preview has focus, Alt+Z zooms the preview instead (handled in handle_preview_key).
        if self.key_matches(key, &self.config.keybindings.zoom)
            && self.ws().focus_target != FocusTarget::Preview
        {
            self.toggle_zoom();
            return Ok(true);
        }

        // Toggle AI title generation (configurable, default Alt+A)
        if self.key_matches(key, &self.config.keybindings.ai_title_toggle) {
            self.ai_title_enabled = !self.ai_title_enabled;
            self.config.features.ai_title = self.ai_title_enabled;
            self.dirty = true;
            return Ok(true);
        }

        // Feature toggle dialog (configurable, default '?', pane focus only)
        if self.key_matches(key, &self.config.keybindings.feature_toggle)
            && self.ws().focus_target == FocusTarget::Pane
        {
            self.feature_toggle.visible = true;
            self.feature_toggle.selected = 0;
            self.feature_toggle.pending = self.config.features.clone();
            self.dirty = true;
            return Ok(true);
        }

        // Settings panel (configurable, default Ctrl+,)
        if self.key_matches(key, &self.config.keybindings.settings) {
            self.settings_panel.visible = true;
            self.settings_panel.selected = 0;
            self.settings_panel.editing = false;
            self.settings_panel.edit_buffer.clear();
            self.dirty = true;
            return Ok(true);
        }

        // Layout picker (configurable, default Ctrl+L)
        if self.key_matches(key, &self.config.keybindings.layout_picker) {
            if self.ws().layout.pane_count() > 1 {
                self.layout_picker.visible = true;
                self.layout_picker.selected = 0;
                return Ok(true);
            }
            return Ok(false);
        }

        // Alt+1 .. Alt+9 — jump to tab N
        if key.modifiers == KeyModifiers::ALT {
            if let KeyCode::Char(c) = key.code {
                if let Some(digit) = c.to_digit(10) {
                    if digit >= 1 && (digit as usize) <= self.workspaces.len() {
                        self.reset_pane_list_sidebar_if_active();
                        self.active_tab = (digit as usize) - 1;
                        self.on_workspace_focus_context_changed();
                        return Ok(true);
                    }
                }
            }
        }

        // Directional pane focus (configurable, defaults Alt+h/j/k/l)
        if self.ws().focus_target == FocusTarget::Pane {
            if self.key_matches(key, &self.config.keybindings.pane_left) {
                self.focus_pane_in_direction(Direction::Left);
                return Ok(true);
            }
            if self.key_matches(key, &self.config.keybindings.pane_down) {
                self.focus_pane_in_direction(Direction::Down);
                return Ok(true);
            }
            if self.key_matches(key, &self.config.keybindings.pane_up) {
                self.focus_pane_in_direction(Direction::Up);
                return Ok(true);
            }
            if self.key_matches(key, &self.config.keybindings.pane_right) {
                self.focus_pane_in_direction(Direction::Right);
                return Ok(true);
            }
        }

        // Ctrl+Right / pane_next — next pane (cycle)
        if (key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Right)
            || self.key_matches(key, &self.config.keybindings.pane_next)
        {
            self.focus_next_pane();
            return Ok(true);
        }

        // Ctrl+Left / pane_prev — previous pane (cycle)
        if (key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Left)
            || self.key_matches(key, &self.config.keybindings.pane_prev)
        {
            self.focus_prev_pane();
            return Ok(true);
        }

        // Pane list sidebar mode
        if self.ws().focus_target == FocusTarget::PaneList {
            if self.key_matches(key, &self.config.keybindings.pane_list) {
                self.toggle_pane_list_sidebar();
                return Ok(true);
            }
            return self.handle_pane_list_key(key);
        }

        // Preview mode
        if self.ws().focus_target == FocusTarget::Preview {
            return self.handle_preview_key(key);
        }

        // File tree mode
        if self.ws().focus_target == FocusTarget::FileTree {
            if self.key_matches(key, &self.config.keybindings.file_tree) {
                self.toggle_file_tree();
                return Ok(true);
            }
            return self.handle_file_tree_key(key);
        }

        // Toggle file tree (configurable, default Ctrl+F)
        if self.key_matches(key, &self.config.keybindings.file_tree) {
            self.toggle_file_tree();
            return Ok(true);
        }

        // Toggle pane list sidebar (configurable, default Alt+P)
        if self.key_matches(key, &self.config.keybindings.pane_list) {
            self.toggle_pane_list_sidebar();
            return Ok(true);
        }

        // Swap preview and terminal positions (configurable, default Ctrl+P)
        if self.key_matches(key, &self.config.keybindings.preview_swap) {
            self.layout_swapped = !self.layout_swapped;
            return Ok(true);
        }

        let multi_pane = self.ws().layout.pane_count() > 1;
        let multi_tab = self.workspaces.len() > 1;

        // Vertical split (configurable, default Ctrl+D)
        if self.key_matches(key, &self.config.keybindings.split_vertical) {
            self.split_focused_pane(SplitDirection::Vertical)?;
            return Ok(true);
        }

        // Horizontal split (configurable, default Ctrl+E)
        if self.key_matches(key, &self.config.keybindings.split_horizontal) {
            self.split_focused_pane(SplitDirection::Horizontal)?;
            return Ok(true);
        }

        // Close pane / preview / tab (configurable, default Ctrl+W)
        if self.key_matches(key, &self.config.keybindings.pane_close) {
            if self.ws().focus_target == FocusTarget::Preview {
                self.preview_zoomed = false;
                self.ws_mut().preview.close();
                self.ws_mut().focus_target = FocusTarget::Pane;
                return Ok(true);
            }
            if multi_pane || multi_tab {
                let pane_id = self.ws().focused_pane_id;
                if self.config.worktree.close_confirm {
                    let worktree_path = self
                        .ws()
                        .panes
                        .get(&pane_id)
                        .and_then(|p| p.worktree_path.clone());
                    self.close_confirm_dialog = CloseConfirmDialog {
                        visible: true,
                        pane_id,
                        worktree_path,
                        focused: CloseConfirmFocus::No,
                    };
                    self.dirty = true;
                } else if multi_pane {
                    self.close_focused_pane();
                } else {
                    self.close_tab(self.active_tab);
                }
                return Ok(true);
            } else {
                return Ok(false);
            }
        }

        // Open pane create dialog (configurable, default Ctrl+N)
        if self.key_matches(key, &self.config.keybindings.pane_create) {
            let agent_labels: Vec<String> = self
                .config
                .multi_ai
                .agents
                .iter()
                .map(|a| format!("{} ({})", a.name, a.command))
                .collect();
            let agent_checks = vec![false; self.config.multi_ai.agents.len()];
            self.pane_create_dialog = PaneCreateDialog {
                visible: true,
                worktree_enabled: self.config.worktree.auto_create,
                agent: self.config.startup.default_agent.clone(),
                focused_field: PaneCreateField::BranchName,
                base_branch: self.config.worktree.base_branch.clone(),
                agent_checks,
                agent_labels,
                ..Default::default()
            };
            self.dirty = true;
            return Ok(true);
        }

        Ok(false)
    }

    pub(super) fn handle_rename_key(&mut self, key: KeyEvent) -> bool {
        let Some(buf) = self.rename_input.as_mut() else {
            return false;
        };
        let needs_relayout = !self.status_bar_visible;
        match key.code {
            KeyCode::Esc => {
                self.rename_input = None;
                if needs_relayout {
                    self.mark_layout_change();
                }
            }
            KeyCode::Enter => {
                let trimmed = buf.trim().to_string();
                self.ws_mut().custom_name = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                };
                self.rename_input = None;
                if needs_relayout {
                    self.mark_layout_change();
                }
            }
            _ => {
                if !edit_key_buffer(buf, key, 32) {
                    return true;
                }
            }
        }
        self.dirty = true;
        true
    }

    pub(super) fn handle_pane_rename_key(&mut self, key: KeyEvent) -> bool {
        let needs_relayout = !self.status_bar_visible;
        match key.code {
            KeyCode::Esc => {
                self.pane_rename_input = None;
                if needs_relayout {
                    self.mark_layout_change();
                }
            }
            KeyCode::Enter => {
                if let Some((pane_id, buf)) = self.pane_rename_input.take() {
                    let trimmed = buf.trim().to_string();
                    if trimmed.is_empty() {
                        self.pane_custom_titles.remove(&pane_id);
                    } else {
                        self.pane_custom_titles.insert(pane_id, trimmed);
                    }
                    if needs_relayout {
                        self.mark_layout_change();
                    }
                }
            }
            _ => {
                // INVARIANT: caller confirmed is_some(); neither Esc nor Enter ran.
                let Some((_, buf)) = self.pane_rename_input.as_mut() else {
                    return false;
                };
                if !edit_key_buffer(buf, key, 32) {
                    return true;
                }
            }
        }
        self.dirty = true;
        true
    }

    pub fn pane_display_title(&self, pane_id: usize) -> Option<&str> {
        self.pane_custom_titles
            .get(&pane_id)
            .map(|s| s.as_str())
            .or_else(|| self.ai_titles.get(&pane_id).map(|s| s.as_str()))
    }

    pub(super) fn handle_file_tree_key(&mut self, key: KeyEvent) -> Result<bool> {
        // Ctrl+D / Ctrl+U — half-page scroll (5 lines), take priority over global splits.
        if key.modifiers == KeyModifiers::CONTROL {
            match key.code {
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    for _ in 0..5 {
                        self.ws_mut().file_tree.move_down();
                    }
                    return Ok(true);
                }
                KeyCode::Char('u') | KeyCode::Char('U') => {
                    for _ in 0..5 {
                        self.ws_mut().file_tree.move_up();
                    }
                    return Ok(true);
                }
                _ => {}
            }
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.ws_mut().file_tree.move_down();
                Ok(true)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.ws_mut().file_tree.move_up();
                Ok(true)
            }
            KeyCode::Enter | KeyCode::Char('o') => {
                let path = self.ws_mut().file_tree.toggle_or_select();
                if let Some(path) = path {
                    match self.config.filetree.enter_action.as_str() {
                        "neovim" | "editor" => {
                            self.open_in_editor(&path);
                        }
                        "choose" => {
                            self.filetree_action_popup = FileTreeActionPopup {
                                visible: true,
                                file_path: path,
                                selected: 0,
                            };
                            self.dirty = true;
                        }
                        other => {
                            if other != "preview" {
                                self.status_flash = Some((
                                    format!(
                                        "unknown enter_action '{}'; falling back to preview",
                                        other
                                    ),
                                    std::time::Instant::now(),
                                ));
                            }
                            self.clear_selection_if_preview();
                            let mut picker = self.image_picker.take();
                            self.ws_mut().preview.load(&path, picker.as_mut());
                            self.image_picker = picker;
                            // Shift focus to the preview so j/k/y/Y work immediately
                            self.ws_mut().focus_target = FocusTarget::Preview;
                        }
                    }
                }
                Ok(true)
            }
            KeyCode::Char('.') => {
                self.ws_mut().file_tree.toggle_hidden();
                Ok(true)
            }
            KeyCode::Char('d') => {
                if self.config.features.diff_preview {
                    let Some(path) = self.selected_file_path() else {
                        self.status_flash = Some((
                            "select a file to view diff".to_string(),
                            std::time::Instant::now(),
                        ));
                        return Ok(true);
                    };
                    let Some(git_cwd) = self.focused_pane_git_cwd() else {
                        self.status_flash = Some((
                            "no active pane context for diff".to_string(),
                            std::time::Instant::now(),
                        ));
                        return Ok(true);
                    };

                    self.clear_selection_if_preview();
                    let mut picker = self.image_picker.take();
                    self.ws_mut().preview.load(&path, picker.as_mut());
                    self.image_picker = picker;

                    let prefer_delta = self.config.preview.prefer_delta;
                    let had_diff = self
                        .ws_mut()
                        .preview
                        .toggle_diff_for(&git_cwd, prefer_delta);
                    if !had_diff {
                        self.status_flash = Some((
                            "no diff for selected file".to_string(),
                            std::time::Instant::now(),
                        ));
                    } else {
                        self.ws_mut().focus_target = FocusTarget::Preview;
                    }
                }
                Ok(true)
            }
            KeyCode::Esc => {
                // Return to pane, keep preview open
                self.ws_mut().focus_target = FocusTarget::Pane;
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    pub(super) fn handle_preview_key(&mut self, key: KeyEvent) -> Result<bool> {
        // Close preview (configurable, default Ctrl+W)
        if self.key_matches(key, &self.config.keybindings.pane_close) {
            self.clear_selection_if_preview();
            self.preview_zoomed = false;
            self.ws_mut().preview.close();
            self.ws_mut().focus_target = FocusTarget::Pane;
            return Ok(true);
        }
        // Swap preview/terminal positions (configurable, default Ctrl+P)
        if self.key_matches(key, &self.config.keybindings.preview_swap) {
            self.layout_swapped = !self.layout_swapped;
            return Ok(true);
        }
        // Quit (configurable, default Ctrl+Q)
        if self.key_matches(key, &self.config.keybindings.quit) {
            self.should_quit = true;
            return Ok(true);
        }
        match (key.modifiers, key.code) {
            (_, KeyCode::Char('j')) | (_, KeyCode::Down) => {
                self.ws_mut().preview.scroll_down(1);
                Ok(true)
            }
            (_, KeyCode::Char('k')) | (_, KeyCode::Up) => {
                self.ws_mut().preview.scroll_up(1);
                Ok(true)
            }
            // Ctrl+D / Ctrl+U — half-page scroll (5 lines), overrides global split bindings.
            (KeyModifiers::CONTROL, KeyCode::Char('d'))
            | (KeyModifiers::CONTROL, KeyCode::Char('D')) => {
                self.ws_mut().preview.scroll_down(5);
                Ok(true)
            }
            (KeyModifiers::CONTROL, KeyCode::Char('u'))
            | (KeyModifiers::CONTROL, KeyCode::Char('U')) => {
                self.ws_mut().preview.scroll_up(5);
                Ok(true)
            }
            (_, KeyCode::PageDown) => {
                self.ws_mut().preview.scroll_down(20);
                Ok(true)
            }
            (_, KeyCode::PageUp) => {
                self.ws_mut().preview.scroll_up(20);
                Ok(true)
            }
            // Horizontal scroll — unmodified arrow keys and vim-style h/l.
            // Ctrl+Left/Right remain focus navigation (matched below).
            (KeyModifiers::NONE, KeyCode::Right)
            | (KeyModifiers::NONE, KeyCode::Char('l'))
            | (KeyModifiers::SHIFT, KeyCode::Right) => {
                self.ws_mut().preview.scroll_right(4);
                Ok(true)
            }
            (KeyModifiers::NONE, KeyCode::Left)
            | (KeyModifiers::NONE, KeyCode::Char('h'))
            | (KeyModifiers::SHIFT, KeyCode::Left) => {
                self.ws_mut().preview.scroll_left(4);
                Ok(true)
            }
            (KeyModifiers::NONE, KeyCode::Home) => {
                self.ws_mut().preview.h_scroll_offset = 0;
                Ok(true)
            }
            (_, KeyCode::Esc) => {
                self.preview_zoomed = false;
                // Return focus to the file tree if it's visible, otherwise to the pane.
                self.ws_mut().focus_target = if self.ws().sidebar_mode == SidebarMode::FileTree {
                    FocusTarget::FileTree
                } else {
                    FocusTarget::Pane
                };
                Ok(true)
            }
            (KeyModifiers::CONTROL, KeyCode::Right) => {
                self.focus_next_pane();
                Ok(true)
            }
            (KeyModifiers::CONTROL, KeyCode::Left) => {
                self.focus_prev_pane();
                Ok(true)
            }
            // y — copy filename to clipboard
            (KeyModifiers::NONE, KeyCode::Char('y')) => {
                if let Some(path) = self.ws().preview.file_path.clone() {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if !name.is_empty() {
                        self.copy_to_clipboard(&name);
                        self.status_flash = Some((
                            format!("Copied filename: {}", name),
                            std::time::Instant::now(),
                        ));
                    }
                }
                Ok(true)
            }
            // Y — copy full file path to clipboard (SHIFT+y or plain uppercase Y)
            (KeyModifiers::SHIFT, KeyCode::Char('Y'))
            | (KeyModifiers::NONE, KeyCode::Char('Y')) => {
                if let Some(path) = self.ws().preview.file_path.clone() {
                    let full = path.to_string_lossy().to_string();
                    self.copy_to_clipboard(&full);
                    self.status_flash =
                        Some((format!("Copied path: {}", full), std::time::Instant::now()));
                }
                Ok(true)
            }
            // Alt+Z — toggle preview zoom (full-screen preview)
            (KeyModifiers::ALT, KeyCode::Char('z')) | (KeyModifiers::ALT, KeyCode::Char('Z')) => {
                self.preview_zoomed = !self.preview_zoomed;
                self.mark_layout_change();
                Ok(true)
            }
            _ => Ok(true),
        }
    }
}
