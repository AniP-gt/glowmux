use super::*;

impl App {
    pub fn drain_pty_events(&mut self) -> bool {
        let mut had_events = false;
        while let Ok(event) = self.event_rx.try_recv() {
            had_events = true;
            match event {
                AppEvent::PtyEof(pane_id) => {
                    let worktree_path = self
                        .find_pane(pane_id)
                        .and_then(|p| p.worktree_path.clone());
                    let branch = self.find_pane(pane_id).and_then(|p| p.branch_name.clone());

                    for ws in &mut self.workspaces {
                        if let Some(pane) = ws.panes.get_mut(&pane_id) {
                            pane.exited = true;
                            break;
                        }
                    }

                    if let (Some(wt_path), Some(_branch)) = (worktree_path, branch) {
                        let close_worktree = self.config.worktree.close_worktree.clone();
                        if close_worktree != "never" {
                            if let Some(handle) = &self.tokio_handle {
                                let tx = self.event_tx.clone();
                                let main_branch = self.config.worktree.main_branch.clone();
                                handle.spawn(async move {
                                    let wt = wt_path.clone();
                                    let merged = tokio::task::spawn_blocking(move || {
                                        crate::worktree::WorktreeManager::new()
                                            .check_merged(&wt, &main_branch)
                                    })
                                    .await
                                    .unwrap_or(false);
                                    if merged {
                                        let _ = tx.send(AppEvent::WorktreeMerged {
                                            worktree_path: wt_path,
                                        });
                                    }
                                });
                            }
                        }
                    }
                }
                AppEvent::CwdChanged(pane_id, new_cwd) => {
                    // Security: resolve symlinks and relative components.
                    // Reject paths that don't resolve to a real directory
                    // (prevents OSC 7 escape sequence path injection).
                    let new_cwd = match new_cwd.canonicalize() {
                        Ok(p) if p.is_dir() => p,
                        _ => continue,
                    };
                    let mut preview_closed_by_cwd = false;
                    for ws in &mut self.workspaces {
                        if ws.panes.contains_key(&pane_id) {
                            // Update pane's cwd
                            if let Some(pane) = ws.panes.get_mut(&pane_id) {
                                pane.cwd = new_cwd.clone();
                            }
                            if ws.focused_pane_id == pane_id {
                                let prev_show_hidden = ws.file_tree.show_hidden;
                                ws.file_tree = FileTree::new(new_cwd.clone());
                                // FileTree::new defaults to show_hidden=true
                                // Only toggle if the previous state was different
                                if ws.file_tree.show_hidden != prev_show_hidden {
                                    ws.file_tree.toggle_hidden();
                                }
                                ws.cwd = new_cwd;
                                ws.name = dir_name(&ws.cwd);
                                ws.git_status = None;
                                ws.preview.close();
                                ws.git_status = None;
                                preview_closed_by_cwd = true;
                            }
                            break;
                        }
                    }
                    if preview_closed_by_cwd {
                        self.preview_zoomed = false;
                        self.refresh_git_status_for_render(true);
                    }
                    self.refresh_git_status_for_render(true);
                }
                AppEvent::PtyOutput { pane_id, lines } => {
                    if self.find_pane(pane_id).is_none() {
                        self.cleanup_pane_runtime_state(pane_id);
                        continue;
                    }
                    // Accumulate meaningful lines into the ring buffer.
                    // Filter out Claude Code UI noise lines (bypass/status bars).
                    if self.ai_title_enabled && !lines.is_empty() {
                        let mut shell_prompt_seen = false;
                        {
                            let ring = self.pane_output_rings.entry(pane_id).or_default();
                            for line in &lines {
                                if ai_title::is_noise_line(line) {
                                    continue;
                                }
                                if ai_title::is_shell_prompt(line) {
                                    shell_prompt_seen = true;
                                    continue; // don't add the prompt line itself to the ring
                                }
                                ring.push_back(line.clone());
                                if ring.len() > 100 {
                                    ring.pop_front();
                                }
                            }
                        }
                        // Fallback trigger: shell prompt detected (for non-Claude panes
                        // that don't send HookEvent::Stop)
                        if shell_prompt_seen {
                            let interval = self.config.ai_title_engine.update_interval_sec;
                            let should_request = should_request_ai_title(
                                self.ai_title_requested_once.contains(&pane_id),
                                self.ai_title_in_flight.contains(&pane_id),
                                self.last_ai_title_request.get(&pane_id).copied(),
                                interval,
                            );
                            if should_request {
                                if let Some(ring) = self.pane_output_rings.get(&pane_id) {
                                    if !ring.is_empty() {
                                        let output =
                                            ring.iter().cloned().collect::<Vec<_>>().join("\n");
                                        let tx = self.event_tx.clone();
                                        if let Some(handle) = &self.tokio_handle {
                                            self.ai_title_requested_once.insert(pane_id);
                                            self.last_ai_title_request
                                                .insert(pane_id, Instant::now());
                                            self.ai_title_in_flight.insert(pane_id);
                                            let config = self.config.ai_title_engine.clone();
                                            let prompt_template =
                                                self.config.ai.title.prompt.clone();
                                            let ollama_url = self.config.ai.ollama.base_url.clone();
                                            let ollama_model = self.config.ai.ollama.model.clone();
                                            let gemini_api_key =
                                                self.config.ai.gemini.api_key.clone();
                                            let gemini_model = self.config.ai.gemini.model.clone();
                                            handle.spawn(async move {
                                                let title = ai_title::generate_title(
                                                    &output,
                                                    &config,
                                                    &prompt_template,
                                                    &ollama_url,
                                                    &ollama_model,
                                                    &gemini_api_key,
                                                    &gemini_model,
                                                )
                                                .await;
                                                let _ = tx.send(AppEvent::AiTitleGenerated {
                                                    pane_id,
                                                    title,
                                                });
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                    self.dirty = true;
                }
                AppEvent::HookReceived {
                    pane_id,
                    event,
                    context,
                } => {
                    let pane_exists = self
                        .workspaces
                        .iter()
                        .any(|ws| ws.panes.contains_key(&pane_id));
                    if pane_exists {
                        self.claude_monitor.bind_session(
                            pane_id,
                            context.transcript_path.as_deref(),
                            context.session_id.as_deref(),
                        );
                        let state = self.pane_states.entry(pane_id).or_default();
                        match event {
                            HookEvent::Stop => {
                                state.status = PaneStatus::Done;
                                state.dismissed = false;
                                // Claude Code returned to prompt — good time to generate title
                                if self.ai_title_enabled {
                                    let interval = self.config.ai_title_engine.update_interval_sec;
                                    let should_request = should_request_ai_title(
                                        self.ai_title_requested_once.contains(&pane_id),
                                        self.ai_title_in_flight.contains(&pane_id),
                                        self.last_ai_title_request.get(&pane_id).copied(),
                                        interval,
                                    );
                                    if should_request {
                                        if let Some(ring) = self.pane_output_rings.get(&pane_id) {
                                            if !ring.is_empty() {
                                                let output = ring
                                                    .iter()
                                                    .cloned()
                                                    .collect::<Vec<_>>()
                                                    .join("\n");
                                                let tx = self.event_tx.clone();
                                                if let Some(handle) = &self.tokio_handle {
                                                    self.ai_title_requested_once.insert(pane_id);
                                                    self.last_ai_title_request
                                                        .insert(pane_id, Instant::now());
                                                    self.ai_title_in_flight.insert(pane_id);
                                                    let config =
                                                        self.config.ai_title_engine.clone();
                                                    let prompt_template =
                                                        self.config.ai.title.prompt.clone();
                                                    let ollama_url =
                                                        self.config.ai.ollama.base_url.clone();
                                                    let ollama_model =
                                                        self.config.ai.ollama.model.clone();
                                                    let gemini_api_key =
                                                        self.config.ai.gemini.api_key.clone();
                                                    let gemini_model =
                                                        self.config.ai.gemini.model.clone();
                                                    handle.spawn(async move {
                                                        let title = ai_title::generate_title(
                                                            &output,
                                                            &config,
                                                            &prompt_template,
                                                            &ollama_url,
                                                            &ollama_model,
                                                            &gemini_api_key,
                                                            &gemini_model,
                                                        )
                                                        .await;
                                                        let _ =
                                                            tx.send(AppEvent::AiTitleGenerated {
                                                                pane_id,
                                                                title,
                                                            });
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            HookEvent::UserPromptSubmit | HookEvent::PreToolUse => {
                                state.status = PaneStatus::Running;
                                state.dismissed = false;
                            }
                            HookEvent::Notification => {
                                state.status = PaneStatus::Waiting;
                                state.dismissed = false;
                            }
                        }
                    }
                }
                AppEvent::AiTitleGenerated { pane_id, title } => {
                    if self.find_pane(pane_id).is_none() {
                        self.cleanup_pane_runtime_state(pane_id);
                        continue;
                    }
                    self.ai_title_in_flight.remove(&pane_id);
                    if let Some(t) = title {
                        self.ai_titles.insert(pane_id, t);
                    }
                }
                AppEvent::BranchNameGenerated { branch } => {
                    self.pane_create_dialog.generating_name = false;
                    if !branch.is_empty() {
                        self.pane_create_dialog.branch_name = branch;
                        self.pane_create_dialog.error_msg = None;
                    } else {
                        self.pane_create_dialog.error_msg =
                            Some("AI generation failed or timed out".to_string());
                    }
                }
                AppEvent::WorktreeCreated {
                    pane_id,
                    cwd,
                    branch_name: _,
                } => {
                    for ws in &mut self.workspaces {
                        if let Some(pane) = ws.panes.get_mut(&pane_id) {
                            pane.worktree_path = Some(cwd.clone());
                            if ws.focused_pane_id == pane_id {
                                ws.git_status = None;
                            }
                            // Shell-quote the path to handle spaces and special characters
                            let path_str = cwd.to_string_lossy();
                            let quoted = format!("'{}'", path_str.replace('\'', "'\\''"));
                            let cd_cmd = format!("cd {}\n", quoted);
                            let _ = pane.write_input(cd_cmd.as_bytes());
                            // Launch pending agent command after cd
                            if let Some(agent_cmd) = pane.pending_agent.take() {
                                let cmd = format!("{}\n", agent_cmd);
                                let _ = pane.write_input(cmd.as_bytes());
                            }
                            break;
                        }
                    }
                    self.refresh_git_status_for_render(true);
                    // Refresh worktree list asynchronously to avoid blocking the UI thread
                    let repo_root = self.ws().cwd.clone();
                    if let Some(handle) = &self.tokio_handle {
                        let tx = self.event_tx.clone();
                        handle.spawn(async move {
                            if let Ok(worktrees) = tokio::task::spawn_blocking(move || {
                                crate::worktree::WorktreeManager::new().list(&repo_root)
                            })
                            .await
                            .unwrap_or(Err(anyhow::anyhow!("task failed")))
                            {
                                let _ = tx.send(AppEvent::WorktreesListed { worktrees });
                            }
                        });
                    }
                }
                AppEvent::WorktreeCreateFailed {
                    pane_id: _,
                    branch_name: _,
                    error,
                } => {
                    eprintln!("glowmux: worktree create failed: {}", error);
                    // Surface error in the dialog if it's still open, otherwise show in status
                    if self.pane_create_dialog.visible {
                        self.pane_create_dialog.error_msg =
                            Some(format!("Worktree error: {}", error));
                    }
                    self.dirty = true;
                }
                AppEvent::WorktreeMerged { worktree_path } => {
                    let close_worktree = self.config.worktree.close_worktree.clone();
                    match close_worktree.as_str() {
                        "auto" => {
                            let repo_root = self.ws().cwd.clone();
                            if let Some(handle) = &self.tokio_handle {
                                let path = worktree_path;
                                let root = repo_root;
                                handle.spawn(async move {
                                    let _ = tokio::task::spawn_blocking(move || {
                                        crate::worktree::WorktreeManager::new().remove(&path, &root)
                                    })
                                    .await;
                                });
                            }
                        }
                        "ask" => {
                            let branch = self
                                .workspaces
                                .iter()
                                .flat_map(|ws| ws.panes.values())
                                .find(|p| p.worktree_path.as_ref() == Some(&worktree_path))
                                .and_then(|p| p.branch_name.clone())
                                .unwrap_or_default();
                            self.worktree_cleanup_dialog = Some(WorktreeCleanupDialog {
                                visible: true,
                                worktree_path,
                                branch,
                                focused: CloseConfirmFocus::No,
                            });
                        }
                        _ => {} // "never"
                    }
                }
                AppEvent::WorktreesListed { worktrees } => {
                    self.ws_mut().worktrees = worktrees;
                }
            }
        }
        if had_events {
            self.dirty = true;
        }
        had_events
    }

    pub fn dismiss_done_on_focus(&mut self, pane_id: usize) {
        if self.pane_status(pane_id) != PaneStatus::Done {
            return;
        }

        let state = self.pane_states.entry(pane_id).or_default();
        state.dismissed = true;
        if state.status != PaneStatus::Done {
            state.status = PaneStatus::Done;
        }
    }

    pub fn pane_status(&self, pane_id: usize) -> PaneStatus {
        let hook_status = self
            .pane_states
            .get(&pane_id)
            .map(|s| s.status)
            .unwrap_or(PaneStatus::Idle);

        let claude_state = self.claude_monitor.state(pane_id);

        resolve_pane_status(hook_status, &claude_state)
    }

    pub fn pane_state_dismissed(&self, pane_id: usize) -> bool {
        self.pane_states
            .get(&pane_id)
            .map(|s| s.dismissed)
            .unwrap_or(false)
    }

    pub(super) fn find_pane(&self, pane_id: usize) -> Option<&crate::pane::Pane> {
        self.workspaces
            .iter()
            .flat_map(|ws| ws.panes.values())
            .find(|p| p.id == pane_id)
    }

    pub fn shutdown(&mut self) {
        for ws in &mut self.workspaces {
            ws.shutdown();
        }
    }

    pub(super) fn open_pane_list_overlay(&mut self) {
        if self.ws().sidebar_mode == SidebarMode::PaneList {
            self.ws_mut().sidebar_mode = SidebarMode::None;
            self.ws_mut().focus_target = FocusTarget::Pane;
            self.mark_layout_change();
            return;
        }
        let pane_ids = self.ws().layout.collect_pane_ids();
        let focused = self.ws().focused_pane_id;
        let selected = pane_ids.iter().position(|&id| id == focused).unwrap_or(0);
        self.pane_list_overlay = PaneListOverlay {
            visible: true,
            selected,
            pane_ids,
        };
        self.dirty = true;
    }

    pub(super) fn handle_pane_list_key(&mut self, key: KeyEvent) -> Result<bool> {
        let is_sidebar = self.ws().sidebar_mode == SidebarMode::PaneList;
        let len = self.pane_list_overlay.pane_ids.len();
        if len == 0 {
            self.pane_list_overlay.visible = false;
            if is_sidebar {
                self.ws_mut().sidebar_mode = SidebarMode::None;
                self.ws_mut().focus_target = FocusTarget::Pane;
                self.mark_layout_change();
            }
            return Ok(true);
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.pane_list_overlay.selected = (self.pane_list_overlay.selected + 1) % len;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.pane_list_overlay.selected = (self.pane_list_overlay.selected + len - 1) % len;
            }
            KeyCode::Char(c @ '0'..='9') => {
                let digit = (c as usize) - ('0' as usize);
                self.pane_list_overlay.selected = digit.min(len.saturating_sub(1));
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(&selected_id) = self
                    .pane_list_overlay
                    .pane_ids
                    .get(self.pane_list_overlay.selected)
                {
                    if self.ws().panes.contains_key(&selected_id) {
                        self.ws_mut().focused_pane_id = selected_id;
                        self.on_workspace_focus_context_changed();
                    }
                }
                self.pane_list_overlay.visible = false;
                if is_sidebar {
                    self.ws_mut().sidebar_mode = SidebarMode::None;
                    self.ws_mut().focus_target = FocusTarget::Pane;
                    self.mark_layout_change();
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.pane_list_overlay.visible = false;
                if is_sidebar {
                    self.ws_mut().sidebar_mode = SidebarMode::None;
                    self.ws_mut().focus_target = FocusTarget::Pane;
                    self.mark_layout_change();
                }
            }
            _ => {}
        }
        self.dirty = true;
        Ok(true)
    }

    pub(super) fn sanitize_shell_arg(s: &str) -> Option<String> {
        if s.bytes().any(|b| b < 0x20 || b == 0x7f) {
            return None;
        }
        Some(s.replace('\'', "'\\''"))
    }

    pub(super) fn open_in_editor(&mut self, path: &std::path::Path) {
        let editor = self.config.filetree.editor.trim().to_string();
        if editor.is_empty() {
            self.status_flash = Some((
                "no editor configured".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        let escaped_editor = match Self::sanitize_shell_arg(&editor) {
            Some(e) => e,
            None => {
                self.status_flash = Some((
                    "editor name contains invalid characters".to_string(),
                    std::time::Instant::now(),
                ));
                return;
            }
        };
        let path_str = path.to_string_lossy();
        let escaped_path = match Self::sanitize_shell_arg(&path_str) {
            Some(p) => p,
            None => {
                self.status_flash = Some((
                    "file path contains invalid characters".to_string(),
                    std::time::Instant::now(),
                ));
                return;
            }
        };
        let cmd = format!("'{}' '{}'\n", escaped_editor, escaped_path);

        let pane_id = self.ws().focused_pane_id;
        if let Some(pane) = self.ws_mut().panes.get_mut(&pane_id) {
            let _ = pane.write_input(cmd.as_bytes());
        }
        self.ws_mut().focus_target = FocusTarget::Pane;
        self.dirty = true;
    }

    pub(super) fn handle_filetree_action_popup_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => {
                self.filetree_action_popup.selected =
                    (self.filetree_action_popup.selected + 1) % FILETREE_ACTION_COUNT;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.filetree_action_popup.selected =
                    (self.filetree_action_popup.selected + FILETREE_ACTION_COUNT - 1)
                        % FILETREE_ACTION_COUNT;
            }
            KeyCode::Enter => {
                let path = self.filetree_action_popup.file_path.clone();
                if self.filetree_action_popup.selected == 0 {
                    self.clear_selection_if_preview();
                    let mut picker = self.image_picker.take();
                    self.ws_mut().preview.load(&path, picker.as_mut());
                    self.image_picker = picker;
                } else {
                    self.open_in_editor(&path);
                }
                self.filetree_action_popup.visible = false;
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.filetree_action_popup.visible = false;
            }
            _ => {}
        }
        self.dirty = true;
        Ok(true)
    }
}
