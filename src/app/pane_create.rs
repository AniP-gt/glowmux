use super::*;

impl App {
    /// Number of visible rows in the prompt textarea. Must match
    /// `PROMPT_VISIBLE` in `ui.rs` so cursor-tracking scroll logic agrees
    /// with the renderer.
    pub(super) const PROMPT_VISIBLE_ROWS: usize = 7;

    pub(super) fn handle_pane_create_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => {
                self.pane_create_dialog.visible = false;
                self.dirty = true;
            }
            KeyCode::Tab => {
                let n_agents = self.pane_create_dialog.agent_checks.len();
                let in_multi = self.pane_create_dialog.launch_mode == LaunchMode::Multi;
                self.pane_create_dialog.focused_field = if in_multi {
                    match &self.pane_create_dialog.focused_field {
                        PaneCreateField::LaunchModeToggle => {
                            if n_agents > 0 {
                                PaneCreateField::MultiCheck(0)
                            } else {
                                PaneCreateField::PromptField
                            }
                        }
                        PaneCreateField::MultiCheck(i) => {
                            if *i + 1 < n_agents {
                                PaneCreateField::MultiCheck(*i + 1)
                            } else {
                                PaneCreateField::PromptField
                            }
                        }
                        PaneCreateField::PromptField => PaneCreateField::OkButton,
                        PaneCreateField::OkButton => PaneCreateField::CancelButton,
                        PaneCreateField::CancelButton => PaneCreateField::LaunchModeToggle,
                        _ => PaneCreateField::LaunchModeToggle,
                    }
                } else {
                    match &self.pane_create_dialog.focused_field {
                        PaneCreateField::LaunchModeToggle => PaneCreateField::BranchName,
                        PaneCreateField::BranchName => PaneCreateField::BaseBranch,
                        PaneCreateField::BaseBranch => PaneCreateField::WorktreeToggle,
                        PaneCreateField::WorktreeToggle => PaneCreateField::AgentField,
                        PaneCreateField::AgentField => PaneCreateField::PromptField,
                        PaneCreateField::PromptField => PaneCreateField::AiGenerate,
                        PaneCreateField::AiGenerate => PaneCreateField::OkButton,
                        PaneCreateField::OkButton => PaneCreateField::CancelButton,
                        PaneCreateField::CancelButton => PaneCreateField::LaunchModeToggle,
                        _ => PaneCreateField::LaunchModeToggle,
                    }
                };
                self.dirty = true;
            }
            KeyCode::BackTab => {
                let n_agents = self.pane_create_dialog.agent_checks.len();
                let in_multi = self.pane_create_dialog.launch_mode == LaunchMode::Multi;
                self.pane_create_dialog.focused_field = if in_multi {
                    match &self.pane_create_dialog.focused_field {
                        PaneCreateField::LaunchModeToggle => PaneCreateField::CancelButton,
                        PaneCreateField::MultiCheck(i) => {
                            if *i == 0 {
                                PaneCreateField::LaunchModeToggle
                            } else {
                                PaneCreateField::MultiCheck(*i - 1)
                            }
                        }
                        PaneCreateField::PromptField => {
                            if n_agents > 0 {
                                PaneCreateField::MultiCheck(n_agents - 1)
                            } else {
                                PaneCreateField::LaunchModeToggle
                            }
                        }
                        PaneCreateField::OkButton => PaneCreateField::PromptField,
                        PaneCreateField::CancelButton => PaneCreateField::OkButton,
                        _ => PaneCreateField::LaunchModeToggle,
                    }
                } else {
                    match &self.pane_create_dialog.focused_field {
                        PaneCreateField::LaunchModeToggle => PaneCreateField::CancelButton,
                        PaneCreateField::BranchName => PaneCreateField::LaunchModeToggle,
                        PaneCreateField::BaseBranch => PaneCreateField::BranchName,
                        PaneCreateField::WorktreeToggle => PaneCreateField::BaseBranch,
                        PaneCreateField::AgentField => PaneCreateField::WorktreeToggle,
                        PaneCreateField::PromptField => PaneCreateField::AgentField,
                        PaneCreateField::AiGenerate => PaneCreateField::PromptField,
                        PaneCreateField::OkButton => PaneCreateField::AiGenerate,
                        PaneCreateField::CancelButton => PaneCreateField::OkButton,
                        _ => PaneCreateField::LaunchModeToggle,
                    }
                };
                self.dirty = true;
            }
            // Alt+Enter: submit from any field (including PromptField)
            KeyCode::Enter if key.modifiers == KeyModifiers::ALT => {
                self.do_pane_create_submit()?;
            }
            KeyCode::Enter => {
                let field = self.pane_create_dialog.focused_field.clone();
                match field {
                    // PromptField: plain Enter inserts a newline
                    PaneCreateField::PromptField => {
                        self.prompt_insert("\n");
                    }
                    PaneCreateField::CancelButton => {
                        self.pane_create_dialog.visible = false;
                        self.dirty = true;
                    }
                    PaneCreateField::WorktreeToggle => {
                        self.pane_create_dialog.worktree_enabled =
                            !self.pane_create_dialog.worktree_enabled;
                        self.dirty = true;
                    }
                    PaneCreateField::AiGenerate => {
                        if !self.pane_create_dialog.generating_name {
                            self.start_branch_name_generation();
                        }
                    }
                    PaneCreateField::LaunchModeToggle => {
                        self.toggle_launch_mode();
                    }
                    PaneCreateField::MultiCheck(i) => {
                        if let Some(checked) = self.pane_create_dialog.agent_checks.get_mut(i) {
                            *checked = !*checked;
                        }
                        self.dirty = true;
                    }
                    PaneCreateField::OkButton
                    | PaneCreateField::BranchName
                    | PaneCreateField::BaseBranch => {
                        self.do_pane_create_submit()?;
                    }
                    _ => {}
                }
            }
            KeyCode::Backspace => match self.pane_create_dialog.focused_field {
                PaneCreateField::BranchName => {
                    self.pane_create_dialog.branch_name.pop();
                    self.dirty = true;
                }
                PaneCreateField::BaseBranch => {
                    self.pane_create_dialog.base_branch.pop();
                    self.dirty = true;
                }
                PaneCreateField::AgentField => {
                    self.pane_create_dialog.agent.pop();
                    self.dirty = true;
                }
                PaneCreateField::PromptField => {
                    self.prompt_backspace();
                }
                _ => {}
            },
            KeyCode::Delete
                if self.pane_create_dialog.focused_field == PaneCreateField::PromptField =>
            {
                self.prompt_delete_forward();
            }
            KeyCode::Left
                if self.pane_create_dialog.focused_field == PaneCreateField::PromptField =>
            {
                self.prompt_move_left();
            }
            KeyCode::Right
                if self.pane_create_dialog.focused_field == PaneCreateField::PromptField =>
            {
                self.prompt_move_right();
            }
            KeyCode::Up
                if self.pane_create_dialog.focused_field == PaneCreateField::PromptField =>
            {
                // col_width matches render: inner_width - 2 (padding) - 10 ("Prompt: [" + "]")
                // We use a fixed conservative width here; render will correct scroll anyway.
                self.prompt_move_up(58);
            }
            KeyCode::Down
                if self.pane_create_dialog.focused_field == PaneCreateField::PromptField =>
            {
                self.prompt_move_down(58);
            }
            KeyCode::Home
                if self.pane_create_dialog.focused_field == PaneCreateField::PromptField =>
            {
                // Move to start of current logical line
                let pos = self.pane_create_dialog.prompt_cursor;
                let s = &self.pane_create_dialog.prompt;
                let line_start = s[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
                self.pane_create_dialog.prompt_cursor = line_start;
                self.update_prompt_scroll(Self::PROMPT_VISIBLE_ROWS);
                self.dirty = true;
            }
            KeyCode::End
                if self.pane_create_dialog.focused_field == PaneCreateField::PromptField =>
            {
                // Move to end of current logical line
                let pos = self.pane_create_dialog.prompt_cursor;
                let s = &self.pane_create_dialog.prompt;
                let line_end = s[pos..].find('\n').map(|i| pos + i).unwrap_or(s.len());
                self.pane_create_dialog.prompt_cursor = line_end;
                self.update_prompt_scroll(Self::PROMPT_VISIBLE_ROWS);
                self.dirty = true;
            }
            KeyCode::Char(c) => {
                match self.pane_create_dialog.focused_field {
                    PaneCreateField::BranchName => {
                        if c.is_ascii_alphanumeric() || c == '-' || c == '/' || c == '_' {
                            self.pane_create_dialog.branch_name.push(c);
                            self.dirty = true;
                        }
                    }
                    PaneCreateField::BaseBranch => {
                        if c.is_ascii_alphanumeric() || c == '-' || c == '/' || c == '_' || c == '.'
                        {
                            self.pane_create_dialog.base_branch.push(c);
                            self.dirty = true;
                        }
                    }
                    PaneCreateField::AgentField => {
                        if c.is_ascii_graphic() || c == ' ' {
                            self.pane_create_dialog.agent.push(c);
                            self.dirty = true;
                        }
                    }
                    PaneCreateField::PromptField => {
                        if !c.is_control() {
                            let mut buf = [0u8; 4];
                            let s = c.encode_utf8(&mut buf);
                            self.prompt_insert(s);
                        }
                    }
                    PaneCreateField::WorktreeToggle if c == ' ' => {
                        self.pane_create_dialog.worktree_enabled =
                            !self.pane_create_dialog.worktree_enabled;
                        self.dirty = true;
                    }
                    PaneCreateField::LaunchModeToggle if c == ' ' => {
                        self.toggle_launch_mode();
                    }
                    PaneCreateField::MultiCheck(i) if c == ' ' => {
                        if let Some(checked) = self.pane_create_dialog.agent_checks.get_mut(i) {
                            *checked = !*checked;
                        }
                        self.dirty = true;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(true)
    }

    pub(super) fn toggle_launch_mode(&mut self) {
        self.pane_create_dialog.launch_mode = match self.pane_create_dialog.launch_mode {
            LaunchMode::Single => {
                self.pane_create_dialog.focused_field =
                    if self.pane_create_dialog.agent_checks.is_empty() {
                        PaneCreateField::PromptField
                    } else {
                        PaneCreateField::MultiCheck(0)
                    };
                LaunchMode::Multi
            }
            LaunchMode::Multi => {
                self.pane_create_dialog.focused_field = PaneCreateField::BranchName;
                LaunchMode::Single
            }
        };
        self.pane_create_dialog.error_msg = None;
        self.dirty = true;
    }

    pub(super) fn do_pane_create_submit(&mut self) -> Result<()> {
        if self.pane_create_dialog.generating_name {
            return Ok(());
        }
        if self.pane_create_dialog.launch_mode == LaunchMode::Multi {
            self.create_multi_ai_panes()?;
            if self.pane_create_dialog.error_msg.is_none() {
                self.pane_create_dialog.visible = false;
            }
            self.dirty = true;
            return Ok(());
        }
        let branch = self.pane_create_dialog.branch_name.clone();
        let worktree = self.pane_create_dialog.worktree_enabled;
        let agent = self.pane_create_dialog.agent.clone();
        let base_branch = self.pane_create_dialog.base_branch.clone();
        let prompt = self.pane_create_dialog.prompt.clone();
        if !prompt.is_empty() {
            let sanitized = Self::sanitize_prompt(&prompt);
            if sanitized.len() > 8192 {
                self.pane_create_dialog.error_msg =
                    Some("Prompt too long (max 8192 bytes)".into());
                self.dirty = true;
                return Ok(());
            }
        }
        let effective_agent =
            Self::format_agent_command(&agent, &prompt, &crate::config::PromptMode::Arg);
        self.pane_create_dialog.visible = false;
        self.create_pane_from_dialog(branch, worktree, effective_agent, base_branch)?;
        self.dirty = true;
        Ok(())
    }

    pub(super) fn write_stdin_prompt(pane: &mut crate::pane::Pane, prompt: &str) {
        let sanitized = Self::sanitize_prompt(prompt);
        if sanitized.is_empty() {
            return;
        }
        let stdin_safe: String = sanitized
            .chars()
            .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
            .collect();
        let _ = pane.write_input(format!("{}\n", stdin_safe).as_bytes());
    }

    pub(super) fn format_agent_command(
        base_cmd: &str,
        prompt: &str,
        mode: &crate::config::PromptMode,
    ) -> String {
        use crate::config::PromptMode;
        if prompt.is_empty() {
            return base_cmd.to_string();
        }
        let sanitized = Self::sanitize_prompt(prompt);
        if sanitized.is_empty() {
            return base_cmd.to_string();
        }
        match mode {
            PromptMode::Arg => format!(
                "{} {}",
                base_cmd,
                Self::shell_quote_prompt(&Self::flatten_newlines(&sanitized))
            ),
            PromptMode::Flag(flag) => format!(
                "{} {} {}",
                base_cmd,
                flag,
                Self::shell_quote_prompt(&Self::flatten_newlines(&sanitized))
            ),
            PromptMode::Stdin | PromptMode::None => base_cmd.to_string(),
        }
    }

    pub(super) fn create_multi_ai_panes(&mut self) -> Result<()> {
        let mut selected_indices: Vec<usize> = self
            .pane_create_dialog
            .agent_checks
            .iter()
            .enumerate()
            .filter(|(_, &checked)| checked)
            .map(|(i, _)| i)
            .collect();

        if selected_indices.is_empty() {
            self.pane_create_dialog.error_msg = Some("Select at least one AI".into());
            return Ok(());
        }

        selected_indices.truncate(4);

        // Resolve agent indices safely; agent_checks length should match the
        // configured agents, but `.get()` keeps us correct in release builds
        // even if the two ever drift.
        let agents: Vec<crate::config::MultiAiAgent> = selected_indices
            .iter()
            .filter_map(|&i| self.config.multi_ai.agents.get(i).cloned())
            .collect();
        let n = agents.len();
        if n == 0 {
            self.pane_create_dialog.error_msg = Some("Select at least one AI".into());
            return Ok(());
        }

        let prompt = self.pane_create_dialog.prompt.clone();
        if !prompt.is_empty() && Self::sanitize_prompt(&prompt).len() > 8192 {
            self.pane_create_dialog.error_msg = Some("Prompt too long (max 8192 bytes)".into());
            return Ok(());
        }

        let focused_id = self.ws().focused_pane_id;

        // Single-agent Multi mode: keep the existing layout intact and just
        // launch the agent in the focused pane. Replacing the layout tree
        // with a single Leaf would orphan every other pane in the workspace.
        if n == 1 {
            let agent = &agents[0];
            let cmd = Self::format_agent_command(&agent.command, &prompt, &agent.prompt_mode);
            if let Some(pane) = self.ws_mut().panes.get_mut(&focused_id) {
                let _ = pane.write_input(format!("{}\n", cmd).as_bytes());
                if matches!(agent.prompt_mode, crate::config::PromptMode::Stdin) && !prompt.is_empty() {
                    Self::write_stdin_prompt(pane, &prompt);
                }
            }
            self.mark_layout_change();
            return Ok(());
        }

        let (cols, rows) = self.last_term_size;
        let pane_rows = rows.saturating_sub(5);
        let pane_cols = cols.saturating_sub(2);

        let pre_call_next_id = self.next_pane_id;
        let mut all_ids: Vec<usize> = vec![focused_id];
        let mut created_ids: Vec<usize> = vec![];

        for _ in 1..n {
            let new_id = self.next_pane_id;
            self.next_pane_id = self.next_pane_id.wrapping_add(1);
            match crate::pane::Pane::new(new_id, pane_rows, pane_cols, self.event_tx.clone()) {
                Ok(pane) => {
                    self.ws_mut().panes.insert(new_id, pane);
                    created_ids.push(new_id);
                    all_ids.push(new_id);
                }
                Err(e) => {
                    for &pid in &created_ids {
                        if let Some(mut p) = self.ws_mut().panes.remove(&pid) {
                            p.kill();
                        }
                    }
                    self.next_pane_id = pre_call_next_id;
                    return Err(e);
                }
            }
        }

        if let Some(new_layout) = Self::build_layout_node(LayoutMode::Grid, &all_ids) {
            self.ws_mut().layout = new_layout;
        }

        for (pane_id, agent) in all_ids.iter().zip(agents.iter()) {
            let cmd = Self::format_agent_command(&agent.command, &prompt, &agent.prompt_mode);
            let cmd_line = format!("{}\n", cmd);
            if let Some(pane) = self.ws_mut().panes.get_mut(pane_id) {
                let _ = pane.write_input(cmd_line.as_bytes());
                if matches!(agent.prompt_mode, crate::config::PromptMode::Stdin) && !prompt.is_empty() {
                    Self::write_stdin_prompt(pane, &prompt);
                }
            }
        }

        self.ws_mut().focused_pane_id = all_ids[0];
        self.mark_layout_change();
        Ok(())
    }

    /// Insert `text` at the prompt cursor, then advance cursor.
    pub(super) fn prompt_insert(&mut self, text: &str) {
        let pos = self.pane_create_dialog.prompt_cursor;
        self.pane_create_dialog.prompt.insert_str(pos, text);
        self.pane_create_dialog.prompt_cursor += text.len();
        self.update_prompt_scroll(Self::PROMPT_VISIBLE_ROWS);
        self.dirty = true;
    }

    /// Delete the character before the cursor (Backspace).
    pub(super) fn prompt_backspace(&mut self) {
        let pos = self.pane_create_dialog.prompt_cursor;
        if pos == 0 {
            return;
        }
        // Find the char boundary just before pos
        let s = &self.pane_create_dialog.prompt;
        let prev = s[..pos]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.pane_create_dialog.prompt.remove(prev);
        self.pane_create_dialog.prompt_cursor = prev;
        self.update_prompt_scroll(Self::PROMPT_VISIBLE_ROWS);
        self.dirty = true;
    }

    /// Delete the character at (after) the cursor (Delete key).
    pub(super) fn prompt_delete_forward(&mut self) {
        let pos = self.pane_create_dialog.prompt_cursor;
        let len = self.pane_create_dialog.prompt.len();
        if pos >= len {
            return;
        }
        self.pane_create_dialog.prompt.remove(pos);
        self.update_prompt_scroll(Self::PROMPT_VISIBLE_ROWS);
        self.dirty = true;
    }

    /// Move cursor one char to the left.
    pub(super) fn prompt_move_left(&mut self) {
        let pos = self.pane_create_dialog.prompt_cursor;
        if pos == 0 {
            return;
        }
        let s = &self.pane_create_dialog.prompt;
        let prev = s[..pos]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.pane_create_dialog.prompt_cursor = prev;
        self.update_prompt_scroll(Self::PROMPT_VISIBLE_ROWS);
        self.dirty = true;
    }

    /// Move cursor one char to the right.
    pub(super) fn prompt_move_right(&mut self) {
        let pos = self.pane_create_dialog.prompt_cursor;
        let s = &self.pane_create_dialog.prompt;
        if pos >= s.len() {
            return;
        }
        let next = s[pos..]
            .char_indices()
            .nth(1)
            .map(|(i, _)| pos + i)
            .unwrap_or(s.len());
        self.pane_create_dialog.prompt_cursor = next;
        self.update_prompt_scroll(Self::PROMPT_VISIBLE_ROWS);
        self.dirty = true;
    }

    /// Move cursor up one visual row (given visible column width).
    pub(super) fn prompt_move_up(&mut self, col_width: usize) {
        if col_width == 0 {
            return;
        }
        let pos = self.pane_create_dialog.prompt_cursor;
        let s = &self.pane_create_dialog.prompt;
        // Compute (logical_line, col) of cursor
        let (line_idx, col) = Self::prompt_line_col(s, pos, col_width);
        if line_idx == 0 {
            self.pane_create_dialog.prompt_cursor = 0;
            self.update_prompt_scroll(Self::PROMPT_VISIBLE_ROWS);
            self.dirty = true;
            return;
        }
        let lines = Self::prompt_wrap_lines(s, col_width);
        let target_line = line_idx - 1;
        let target_col = col.min(lines[target_line].1);
        self.pane_create_dialog.prompt_cursor =
            Self::prompt_offset_of(s, &lines, target_line, target_col);
        self.update_prompt_scroll(Self::PROMPT_VISIBLE_ROWS);
        self.dirty = true;
    }

    /// Move cursor down one visual row.
    pub(super) fn prompt_move_down(&mut self, col_width: usize) {
        if col_width == 0 {
            return;
        }
        let pos = self.pane_create_dialog.prompt_cursor;
        let s = &self.pane_create_dialog.prompt;
        let (line_idx, col) = Self::prompt_line_col(s, pos, col_width);
        let lines = Self::prompt_wrap_lines(s, col_width);
        if line_idx + 1 >= lines.len() {
            self.pane_create_dialog.prompt_cursor = s.len();
            self.update_prompt_scroll(Self::PROMPT_VISIBLE_ROWS);
            self.dirty = true;
            return;
        }
        let target_line = line_idx + 1;
        let target_col = col.min(lines[target_line].1);
        self.pane_create_dialog.prompt_cursor =
            Self::prompt_offset_of(s, &lines, target_line, target_col);
        self.update_prompt_scroll(Self::PROMPT_VISIBLE_ROWS);
        self.dirty = true;
    }

    /// Recompute and persist `prompt_scroll` so the cursor stays within the
    /// visible window. Without this, the renderer would derive scroll from
    /// the cursor each frame but the value would never be stored back, so
    /// the next frame would snap to the top.
    pub(super) fn update_prompt_scroll(&mut self, visible_rows: usize) {
        if visible_rows == 0 {
            return;
        }
        // Matches `col_width` used by the dialog renderer / move helpers.
        let col_width: usize = 58;
        let cursor = self.pane_create_dialog.prompt_cursor;
        let prompt = self.pane_create_dialog.prompt.clone();
        let (cursor_row, _) = Self::prompt_line_col(&prompt, cursor, col_width);
        let scroll = self.pane_create_dialog.prompt_scroll;
        let new_scroll = if cursor_row < scroll {
            cursor_row
        } else if cursor_row >= scroll + visible_rows {
            cursor_row + 1 - visible_rows
        } else {
            scroll
        };
        self.pane_create_dialog.prompt_scroll = new_scroll;
    }

    /// Wrap `s` into visual rows of `width` chars, respecting '\n'.
    /// Returns a vec of (byte_start, char_len) per row.
    pub(super) fn prompt_wrap_lines(s: &str, width: usize) -> Vec<(usize, usize)> {
        let mut rows: Vec<(usize, usize)> = Vec::new();
        let mut byte_pos = 0usize;
        for logical_line in s.split('\n') {
            let chars: Vec<(usize, char)> = logical_line.char_indices().collect();
            if chars.is_empty() {
                rows.push((byte_pos, 0));
                byte_pos += 1; // '\n'
                continue;
            }
            let mut start_char = 0usize;
            loop {
                let end_char = (start_char + width).min(chars.len());
                let row_byte_start = byte_pos
                    + if start_char < chars.len() {
                        chars[start_char].0
                    } else {
                        logical_line.len()
                    };
                let char_len = end_char - start_char;
                rows.push((row_byte_start, char_len));
                start_char = end_char;
                if start_char >= chars.len() {
                    break;
                }
            }
            // advance past the logical line and its '\n'
            byte_pos += logical_line.len();
            if byte_pos < s.len() {
                byte_pos += 1; // '\n'
            }
        }
        if rows.is_empty() {
            rows.push((0, 0));
        }
        rows
    }

    /// Return (row_index, col_in_row) for the given byte offset.
    pub(super) fn prompt_line_col(s: &str, byte_pos: usize, width: usize) -> (usize, usize) {
        let rows = Self::prompt_wrap_lines(s, width);
        for (i, &(start, char_len)) in rows.iter().enumerate() {
            let end_byte = if i + 1 < rows.len() {
                rows[i + 1].0
            } else {
                s.len() + 1
            };
            if byte_pos >= start && byte_pos < end_byte.min(s.len() + 1) {
                let col = s[start..byte_pos].chars().count().min(char_len);
                return (i, col);
            }
        }
        let last = rows.len().saturating_sub(1);
        (last, rows.last().map(|r| r.1).unwrap_or(0))
    }

    /// Compute byte offset for a given (row, col) pair.
    pub(super) fn prompt_offset_of(s: &str, rows: &[(usize, usize)], row: usize, col: usize) -> usize {
        if row >= rows.len() {
            return s.len();
        }
        let (start, char_len) = rows[row];
        let clamped_col = col.min(char_len);
        let byte_offset: usize = s[start..]
            .char_indices()
            .nth(clamped_col)
            .map(|(i, _)| i)
            .unwrap_or(s[start..].len());
        start + byte_offset
    }

    pub(super) fn sanitize_prompt(s: &str) -> String {
        s.chars()
            .filter(|&c| c == '\n' || (!c.is_control() && c != '\x7f'))
            .collect()
    }

    pub(super) fn flatten_newlines(s: &str) -> String {
        s.chars()
            .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
            .collect()
    }

    pub(super) fn shell_quote_prompt(s: &str) -> String {
        format!("'{}'", s.replace('\'', "'\\''"))
    }

    pub(super) fn handle_close_confirm_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.close_confirm_dialog.visible = false;
                self.dirty = true;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.close_confirm_dialog.focused = CloseConfirmFocus::Yes;
                self.dirty = true;
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.close_confirm_dialog.focused = CloseConfirmFocus::No;
                self.dirty = true;
            }
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                let should_close = self.close_confirm_dialog.focused == CloseConfirmFocus::Yes
                    || matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y'));
                if should_close {
                    let pane_id = self.close_confirm_dialog.pane_id;
                    let worktree_path = self.close_confirm_dialog.worktree_path.clone();
                    self.close_confirm_dialog.visible = false;
                    if let Some(wt_path) = worktree_path {
                        match self.config.worktree.close_worktree.as_str() {
                            "auto" => {
                                let repo_root = self.ws().cwd.clone();
                                if let Some(handle) = &self.tokio_handle {
                                    let path = wt_path.clone();
                                    let root = repo_root.clone();
                                    handle.spawn(async move {
                                        let result = tokio::task::spawn_blocking(move || {
                                            crate::worktree::WorktreeManager::new()
                                                .remove(&path, &root)
                                        })
                                        .await;
                                        if let Ok(Err(e)) = result {
                                            eprintln!("glowmux: worktree remove failed: {}", e);
                                        }
                                    });
                                }
                            }
                            "ask" => {
                                self.worktree_cleanup_dialog = Some(WorktreeCleanupDialog {
                                    visible: true,
                                    worktree_path: wt_path.clone(),
                                    branch: self
                                        .ws()
                                        .panes
                                        .values()
                                        .find(|p| p.worktree_path.as_ref() == Some(&wt_path))
                                        .and_then(|p| p.branch_name.clone())
                                        .unwrap_or_default(),
                                    focused: CloseConfirmFocus::No,
                                });
                            }
                            _ => {} // "never"
                        }
                    }
                    let multi_pane = self.ws().layout.pane_count() > 1;
                    let multi_tab = self.workspaces.len() > 1;
                    if multi_pane {
                        self.ws_mut().focused_pane_id = pane_id;
                        self.close_focused_pane();
                    } else if multi_tab {
                        self.close_tab(self.active_tab);
                    }
                    self.dirty = true;
                } else {
                    self.close_confirm_dialog.visible = false;
                    self.dirty = true;
                }
            }
            _ => {}
        }
        Ok(true)
    }

    pub(super) fn handle_worktree_cleanup_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                if let Some(ref mut d) = self.worktree_cleanup_dialog {
                    d.visible = false;
                }
                self.dirty = true;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if let Some(ref mut d) = self.worktree_cleanup_dialog {
                    d.focused = CloseConfirmFocus::Yes;
                }
                self.dirty = true;
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if let Some(ref mut d) = self.worktree_cleanup_dialog {
                    d.focused = CloseConfirmFocus::No;
                }
                self.dirty = true;
            }
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                let should_delete = self
                    .worktree_cleanup_dialog
                    .as_ref()
                    .map(|d| {
                        d.focused == CloseConfirmFocus::Yes
                            || matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y'))
                    })
                    .unwrap_or(false);
                if should_delete {
                    let path = self
                        .worktree_cleanup_dialog
                        .as_ref()
                        .map(|d| d.worktree_path.clone());
                    if let Some(p) = path {
                        let repo_root = self.ws().cwd.clone();
                        if let Some(handle) = &self.tokio_handle {
                            let root = repo_root.clone();
                            handle.spawn(async move {
                                let result = tokio::task::spawn_blocking(move || {
                                    crate::worktree::WorktreeManager::new().remove(&p, &root)
                                })
                                .await;
                                if let Ok(Err(e)) = result {
                                    eprintln!("glowmux: worktree remove failed: {}", e);
                                }
                            });
                        }
                    }
                }
                if let Some(ref mut d) = self.worktree_cleanup_dialog {
                    d.visible = false;
                }
                self.dirty = true;
            }
            _ => {}
        }
        Ok(true)
    }

    pub(super) fn start_branch_name_generation(&mut self) {
        // Respect the feature gate: worktree_ai_name OR ai.worktree_name.enabled.
        // Either flag turns the AI Generate button into a real call.
        let feature_on =
            self.config.features.worktree_ai_name || self.config.ai.worktree_name.enabled;
        if !feature_on {
            self.pane_create_dialog.error_msg =
                Some("AI worktree name disabled (features.worktree_ai_name)".to_string());
            self.dirty = true;
            return;
        }
        if let Some(handle) = &self.tokio_handle {
            self.pane_create_dialog.generating_name = true;
            self.pane_create_dialog.error_msg = None;
            let tx = self.event_tx.clone();
            let config = self.config.ai.clone();
            let context = self.pane_create_dialog.branch_name.clone();
            handle.spawn(async move {
                let result = crate::worktree::generate_branch_name(&context, &config).await;
                let branch = result.unwrap_or_default();
                let _ = tx.send(AppEvent::BranchNameGenerated { branch });
            });
        }
    }

    pub(super) fn create_pane_from_dialog(
        &mut self,
        branch_name: String,
        worktree_enabled: bool,
        agent: String,
        base_branch: String,
    ) -> Result<()> {
        let (cols, rows) = self.last_term_size;
        let pane_id = self.next_pane_id;
        self.next_pane_id = self.next_pane_id.wrapping_add(1);

        let cwd = self.ws().cwd.clone();
        let pane_rows = rows.saturating_sub(5);
        let pane_cols = cols.saturating_sub(2);
        let mut pane =
            crate::pane::Pane::new(pane_id, pane_rows, pane_cols, self.event_tx.clone())?;

        if !branch_name.is_empty() {
            pane.branch_name = Some(branch_name.clone());
        }

        self.ws_mut().panes.insert(pane_id, pane);

        let focused_id = self.ws().focused_pane_id;
        self.ws_mut()
            .layout
            .split_pane(focused_id, pane_id, SplitDirection::Vertical);
        self.ws_mut().focused_pane_id = pane_id;
        self.on_workspace_focus_context_changed();
        self.mark_layout_change();

        let has_branch = !branch_name.is_empty();
        if worktree_enabled && has_branch {
            if let Some(handle) = &self.tokio_handle {
                let tx = self.event_tx.clone();
                let repo_root = cwd;
                let branch = branch_name;
                let opts = crate::worktree::WorktreeCreateOptions {
                    prefer_gwq: self.config.worktree.prefer_gwq,
                    worktree_dir: self.config.worktree.worktree_dir.clone(),
                    base_branch,
                };
                handle.spawn(async move {
                    let branch_clone = branch.clone();
                    let opts_clone = opts.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        let mgr = crate::worktree::WorktreeManager::new();
                        mgr.create_with_options(&repo_root, &branch_clone, &opts_clone)
                    })
                    .await;
                    match result {
                        Ok(Ok(path)) => {
                            let _ = tx.send(AppEvent::WorktreeCreated {
                                pane_id,
                                cwd: path,
                                branch_name: branch,
                            });
                        }
                        Ok(Err(e)) => {
                            let _ = tx.send(AppEvent::WorktreeCreateFailed {
                                pane_id,
                                branch_name: branch,
                                error: e.to_string(),
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::WorktreeCreateFailed {
                                pane_id,
                                branch_name: branch,
                                error: e.to_string(),
                            });
                        }
                    }
                });
            }
        } else if !worktree_enabled && has_branch {
            // Non-worktree branch: run `git checkout -b <branch>` in the new pane's shell.
            // Shell-quote the branch name to handle special characters safely.
            let quoted = format!("'{}'", branch_name.replace('\'', "'\\''"));
            let cmd = format!("git checkout -b {}\n", quoted);
            if let Some(pane) = self.ws_mut().panes.get_mut(&pane_id) {
                let _ = pane.write_input(cmd.as_bytes());
            }
        }

        // If an agent command is set, launch it in the new pane after any branch/worktree setup.
        // For worktree panes the agent will be launched after the WorktreeCreated cd completes,
        // so we store it on the pane and send it in the WorktreeCreated handler.
        // For non-worktree panes we send it immediately (shell buffers the input).
        if !agent.is_empty() {
            if worktree_enabled && has_branch {
                // Store for deferred launch after WorktreeCreated cd
                if let Some(pane) = self.ws_mut().panes.get_mut(&pane_id) {
                    pane.pending_agent = Some(agent);
                }
            } else {
                let cmd = format!("{}\n", agent);
                if let Some(pane) = self.ws_mut().panes.get_mut(&pane_id) {
                    let _ = pane.write_input(cmd.as_bytes());
                }
            }
        }

        Ok(())
    }

    pub fn save_session(&self) {
        if !self.config.session.enabled {
            return;
        }
        let Some(path) = crate::session::SessionData::session_path() else {
            return;
        };

        let workspaces: Vec<_> = self
            .workspaces
            .iter()
            .map(|ws| {
                let panes: Vec<_> = ws
                    .panes
                    .values()
                    .map(|p| crate::session::PaneSnapshot {
                        id: p.id,
                        cwd: p.pane_cwd(),
                        title: self
                            .pane_display_title(p.id)
                            .unwrap_or_default()
                            .to_string(),
                        worktree_path: p.worktree_path.clone(),
                        branch: p.branch_name.clone(),
                    })
                    .collect();
                crate::session::WorkspaceSnapshot {
                    name: ws.name.clone(),
                    cwd: ws.cwd.clone(),
                    panes,
                    layout_mode: format!("{:?}", ws.layout_mode),
                }
            })
            .collect();

        let data = crate::session::SessionData {
            version: 1,
            workspaces,
            active_tab: self.active_tab,
        };
        data.save(&path);
    }

    pub(super) fn apply_startup_panes(&mut self, rows: u16, cols: u16) -> Result<()> {
        let pane_configs = self.config.startup.panes.clone();
        if pane_configs.is_empty() {
            return Ok(());
        }

        let initial_id = self.ws().focused_pane_id;
        let mut all_ids = vec![initial_id];
        let mut commands: Vec<(usize, String)> = Vec::new();

        if let Some(first) = pane_configs.first() {
            if !first.command.is_empty() {
                commands.push((initial_id, first.command.clone()));
            }
        }

        for startup_pane in pane_configs.iter().skip(1) {
            let new_id = self.next_pane_id;
            self.next_pane_id = self.next_pane_id.wrapping_add(1);

            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let pane = Pane::new_with_cwd(new_id, rows, cols, self.event_tx.clone(), Some(cwd))?;
            self.ws_mut().panes.insert(new_id, pane);
            all_ids.push(new_id);

            if !startup_pane.command.is_empty() {
                commands.push((new_id, startup_pane.command.clone()));
            }
        }

        let directions: Vec<SplitDirection> = pane_configs
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, sp)| match sp.split.to_lowercase().as_str() {
                "vertical" | "v" => SplitDirection::Vertical,
                "horizontal" | "h" => SplitDirection::Horizontal,
                _ => {
                    if i % 2 == 1 {
                        SplitDirection::Vertical
                    } else {
                        SplitDirection::Horizontal
                    }
                }
            })
            .collect();

        let uniform_dir = if directions.is_empty() {
            None
        } else if directions.iter().all(|&d| d == directions[0]) {
            Some(directions[0])
        } else {
            None
        };

        if let Some(dir) = uniform_dir {
            // All panes share one direction: build a balanced binary tree so every
            // pane gets an equal share. Sequential splitting produces a skewed tree
            // where each new split halves only the last pane.
            if let Some(new_layout) = Self::build_stack(&all_ids, dir) {
                self.ws_mut().layout = new_layout;
            }
        } else {
            // Mixed directions: build tree sequentially, splitting the previously
            // added pane each time (replicates the original incremental behavior).
            for (i, _) in pane_configs.iter().enumerate().skip(1) {
                let new_id = all_ids[i];
                let focused = all_ids[i - 1];
                let direction = directions[i - 1];
                self.ws_mut().layout.split_pane(focused, new_id, direction);
            }
        }

        for (pane_id, cmd) in commands {
            let cmd = format!("{}\r", cmd);
            if let Some(p) = self.ws_mut().panes.get_mut(&pane_id) {
                let _ = p.write_input(cmd.as_bytes());
            }
        }

        if let Some(&first_id) = self.ws().layout.collect_pane_ids().first() {
            self.ws_mut().focused_pane_id = first_id;
        }

        Ok(())
    }
}
