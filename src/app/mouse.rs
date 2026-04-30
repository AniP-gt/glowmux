use super::*;

impl App {
    pub fn handle_mouse_event(&mut self, mouse: MouseEvent) {
        // Cancel any in-progress rename on mouse click so
        // the buffer can't silently migrate to another tab.
        if matches!(mouse.kind, MouseEventKind::Down(_)) && self.rename_input.is_some() {
            let needs_relayout = !self.status_bar_visible;
            self.rename_input = None;
            self.dirty = true;
            if needs_relayout {
                self.mark_layout_change();
            }
        }
        if matches!(mouse.kind, MouseEventKind::Down(_)) && self.pane_rename_input.is_some() {
            let needs_relayout = !self.status_bar_visible;
            self.pane_rename_input = None;
            self.dirty = true;
            if needs_relayout {
                self.mark_layout_change();
            }
        }

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let col = mouse.column;
                let row = mouse.row;

                // Clear previous selection on any click
                self.selection = None;

                // Check tab bar clicks
                for &(tab_idx, rect) in &self.last_tab_rects {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        let now = Instant::now();
                        let is_double = matches!(
                            self.last_tab_click,
                            Some((prev_idx, prev_t))
                                if prev_idx == tab_idx
                                    && now.duration_since(prev_t).as_millis() < 500
                        );
                        if tab_idx != self.active_tab {
                            self.reset_pane_list_sidebar_if_active();
                        }
                        self.active_tab = tab_idx;
                        self.on_workspace_focus_context_changed();
                        if is_double {
                            self.rename_input = Some(String::new());
                            self.last_tab_click = None;
                        } else {
                            self.last_tab_click = Some((tab_idx, now));
                        }
                        self.dirty = true;
                        return;
                    }
                }
                // Click missed the tab bar — reset double-click tracker.
                self.last_tab_click = None;

                // Check [+] new tab button
                if let Some(rect) = self.last_new_tab_rect {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        let _ = self.new_tab();
                        return;
                    }
                }

                // Check border drag (file tree / preview)
                if self.is_on_file_tree_border(col) {
                    self.dragging = Some(DragTarget::FileTreeBorder);
                    return;
                }
                if self.is_on_preview_border(col) {
                    self.dragging = Some(DragTarget::PreviewBorder);
                    return;
                }

                // Check pane split border drag
                if let Some(pane_area) = self.ws().last_pane_rects.first().map(|_| {
                    // Compute the total pane area from all pane rects
                    let rects = &self.ws().last_pane_rects;
                    let min_x = rects.iter().map(|(_, r)| r.x).min().unwrap_or(0);
                    let min_y = rects.iter().map(|(_, r)| r.y).min().unwrap_or(0);
                    let max_x = rects.iter().map(|(_, r)| r.x + r.width).max().unwrap_or(0);
                    let max_y = rects.iter().map(|(_, r)| r.y + r.height).max().unwrap_or(0);
                    Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
                }) {
                    let boundaries = self.ws().layout.split_boundaries(pane_area);
                    for (boundary, direction, path) in boundaries {
                        let on_border = match direction {
                            SplitDirection::Vertical => {
                                col >= boundary.saturating_sub(1)
                                    && col <= boundary
                                    && row >= pane_area.y
                                    && row < pane_area.y + pane_area.height
                            }
                            SplitDirection::Horizontal => {
                                row >= boundary.saturating_sub(1)
                                    && row <= boundary
                                    && col >= pane_area.x
                                    && col < pane_area.x + pane_area.width
                            }
                        };
                        if on_border {
                            self.dragging = Some(DragTarget::PaneSplit(path, direction, pane_area));
                            return;
                        }
                    }
                }

                // Check sidebar click (file tree or pane list)
                if let Some(rect) = self.ws().last_file_tree_rect {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        match self.ws().sidebar_mode {
                            SidebarMode::FileTree => {
                                self.ws_mut().focus_target = FocusTarget::FileTree;
                                let inner_y = row.saturating_sub(rect.y + 1);
                                let scroll = self.ws().file_tree.scroll_offset;
                                let entry_idx = scroll + inner_y as usize;
                                let entry_count = self.ws().file_tree.visible_entries().len();
                                if entry_idx < entry_count {
                                    self.ws_mut().file_tree.selected_index = entry_idx;
                                    let path = self.ws_mut().file_tree.toggle_or_select();
                                    if let Some(path) = path {
                                        self.clear_selection_if_preview();
                                        let mut picker = self.image_picker.take();
                                        self.ws_mut().preview.load(&path, picker.as_mut());
                                        self.image_picker = picker;
                                    }
                                }
                                return;
                            }
                            SidebarMode::PaneList => {
                                self.ws_mut().focus_target = FocusTarget::PaneList;
                                return;
                            }
                            SidebarMode::None => {}
                        }
                    }
                }

                // Check preview click
                if let Some(rect) = self.ws().last_preview_rect {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        self.ws_mut().focus_target = FocusTarget::Preview;
                        return;
                    }
                }

                // Check pane clicks
                let pane_rects = self.ws().last_pane_rects.clone();
                for (pane_id, rect) in pane_rects {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        self.ws_mut().focused_pane_id = pane_id;
                        self.ws_mut().focus_target = FocusTarget::Pane;
                        self.dismiss_done_on_focus(pane_id);
                        self.on_workspace_focus_context_changed();

                        // Check if clicking on scrollbar (rightmost column inside border)
                        let scrollbar_col = rect.x + rect.width - 2; // -1 border, -1 scrollbar
                        if col >= scrollbar_col {
                            let inner = Rect::new(
                                rect.x + 1,
                                rect.y + 1,
                                rect.width.saturating_sub(2),
                                rect.height.saturating_sub(2),
                            );
                            self.scroll_pane_to_click(pane_id, row, &inner);
                            self.dragging = Some(DragTarget::Scrollbar(pane_id, inner));
                        }
                        return;
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let col = mouse.column;
                let row = mouse.row;

                // Border drag takes priority
                if let Some(ref target) = self.dragging.clone() {
                    match target {
                        DragTarget::FileTreeBorder => {
                            self.file_tree_width = col.clamp(10, 60);
                        }
                        DragTarget::PreviewBorder => {
                            if let Some(rect) = self.ws().last_preview_rect {
                                if self.layout_swapped {
                                    let new_width = col.saturating_sub(rect.x).clamp(15, 80);
                                    self.preview_width = new_width;
                                } else {
                                    let total_right = rect.x + rect.width;
                                    let new_width = total_right.saturating_sub(col).clamp(15, 80);
                                    self.preview_width = new_width;
                                }
                            }
                        }
                        DragTarget::PaneSplit(path, direction, area) => {
                            let new_ratio = match direction {
                                SplitDirection::Vertical => {
                                    (col.saturating_sub(area.x) as f32) / area.width.max(1) as f32
                                }
                                SplitDirection::Horizontal => {
                                    (row.saturating_sub(area.y) as f32) / area.height.max(1) as f32
                                }
                            };
                            self.ws_mut().layout.update_ratio(path, new_ratio);
                        }
                        DragTarget::Scrollbar(pane_id, inner) => {
                            self.scroll_pane_to_click(*pane_id, row, inner);
                        }
                    }
                    return;
                }

                // Text selection: extend if active, or start new
                if let Some(ref mut sel) = self.selection {
                    let inner = sel.content_rect;
                    match sel.target {
                        SelectionTarget::Pane(_) => {
                            // Pane: screen-relative coords inside inner.
                            sel.end_col = col
                                .saturating_sub(inner.x)
                                .min(inner.width.saturating_sub(1))
                                as u32;
                            sel.end_row = row
                                .saturating_sub(inner.y)
                                .min(inner.height.saturating_sub(1))
                                as u32;
                        }
                        SelectionTarget::Preview => {
                            // Preview: translate screen coords to
                            // source (absolute line + char offset)
                            // using the current scroll state.
                            let scroll_v = self.ws().preview.scroll_offset;
                            let h_scroll = self.ws().preview.h_scroll_offset;

                            let mut screen_col = col.saturating_sub(inner.x);
                            let mut screen_row = row.saturating_sub(inner.y);

                            // Auto-scroll when drag reaches an edge.
                            // Move the underlying scroll by one step
                            // so the cursor can "pull" more content
                            // into view. Clamp screen position so the
                            // computed source coord tracks the new edge.
                            if col < inner.x {
                                self.ws_mut().preview.scroll_left(2);
                                screen_col = 0;
                            } else if col >= inner.x + inner.width {
                                self.ws_mut().preview.scroll_right(2);
                                screen_col = inner.width.saturating_sub(1);
                            }
                            if row < inner.y {
                                self.ws_mut().preview.scroll_up(1);
                                screen_row = 0;
                            } else if row >= inner.y + inner.height {
                                self.ws_mut().preview.scroll_down(1);
                                screen_row = inner.height.saturating_sub(1);
                            }

                            // Re-read scroll state in case we changed it above.
                            let scroll_v = self.ws().preview.scroll_offset.max(scroll_v);
                            let h_scroll = self.ws().preview.h_scroll_offset.max(h_scroll);
                            // Clamp end_row to a valid absolute line index.
                            let lines_len = preview_line_count(&self.ws().preview);
                            let abs_row =
                                (scroll_v + screen_row as usize).min(lines_len.saturating_sub(1));
                            let abs_col = screen_col as usize + h_scroll;
                            // Update the selection endpoint (source coords).
                            if let Some(sel) = self.selection.as_mut() {
                                sel.end_row = abs_row as u32;
                                sel.end_col = abs_col as u32;
                            }
                        }
                    }
                } else {
                    // Start new selection — try pane areas first, then preview
                    let pane_rects = self.ws().last_pane_rects.clone();
                    let mut started = false;
                    for (pane_id, rect) in pane_rects {
                        if col >= rect.x
                            && col < rect.x + rect.width
                            && row >= rect.y
                            && row < rect.y + rect.height
                        {
                            let inner = Rect::new(
                                rect.x + 1,
                                rect.y + 1,
                                rect.width.saturating_sub(2),
                                rect.height.saturating_sub(2),
                            );
                            let cell_col = col.saturating_sub(inner.x) as u32;
                            let cell_row = row.saturating_sub(inner.y) as u32;
                            self.selection = Some(TextSelection {
                                target: SelectionTarget::Pane(pane_id),
                                start_row: cell_row,
                                start_col: cell_col,
                                end_row: cell_row,
                                end_col: cell_col,
                                content_rect: inner,
                            });
                            started = true;
                            break;
                        }
                    }
                    // Preview drag selection. Content area is the inside
                    // of the preview border minus the 5-column line-number
                    // gutter (format "{:>4}│"). Selection stores source
                    // coords (abs line index, char offset) so it can
                    // survive scrolling.
                    if !started {
                        if let Some(rect) = self.ws().last_preview_rect {
                            if col >= rect.x
                                && col < rect.x + rect.width
                                && row >= rect.y
                                && row < rect.y + rect.height
                            {
                                let gutter_width = if self.ws().preview.diff_mode {
                                    0
                                } else {
                                    5
                                };
                                let inner = Rect::new(
                                    rect.x + 1 + gutter_width,
                                    rect.y + 1,
                                    rect.width.saturating_sub(2 + gutter_width),
                                    rect.height.saturating_sub(2),
                                );
                                // Ignore drags that start inside the gutter
                                if col >= inner.x && row >= inner.y {
                                    let screen_col = col.saturating_sub(inner.x);
                                    let screen_row = row.saturating_sub(inner.y);
                                    let scroll_v = self.ws().preview.scroll_offset;
                                    let h_scroll = self.ws().preview.h_scroll_offset;
                                    let lines_len = preview_line_count(&self.ws().preview);
                                    let abs_row = (scroll_v + screen_row as usize)
                                        .min(lines_len.saturating_sub(1));
                                    let abs_col = screen_col as usize + h_scroll;
                                    self.selection = Some(TextSelection {
                                        target: SelectionTarget::Preview,
                                        start_row: abs_row as u32,
                                        start_col: abs_col as u32,
                                        end_row: abs_row as u32,
                                        end_col: abs_col as u32,
                                        content_rect: inner,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.dragging = None;

                // Copy selected text to clipboard
                if let Some(sel) = self.selection.clone() {
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
                    }
                    // Keep selection visible until next click
                }
            }
            MouseEventKind::ScrollUp => {
                let col = mouse.column;
                let row = mouse.row;

                if let Some(rect) = self.ws().last_file_tree_rect {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        match self.ws().sidebar_mode {
                            SidebarMode::FileTree => {
                                self.ws_mut().file_tree.scroll_up(3);
                                return;
                            }
                            SidebarMode::PaneList => {
                                return;
                            }
                            SidebarMode::None => {}
                        }
                    }
                }
                if let Some(rect) = self.ws().last_preview_rect {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        self.ws_mut().preview.scroll_up(3);
                        return;
                    }
                }
                let pane_rects = self.ws().last_pane_rects.clone();
                for (pane_id, rect) in pane_rects {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        if let Some(pane) = self.ws().panes.get(&pane_id) {
                            pane.scroll_up(3);
                        }
                        return;
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                let col = mouse.column;
                let row = mouse.row;

                if let Some(rect) = self.ws().last_file_tree_rect {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        match self.ws().sidebar_mode {
                            SidebarMode::FileTree => {
                                self.ws_mut().file_tree.scroll_down(3);
                                return;
                            }
                            SidebarMode::PaneList => {
                                return;
                            }
                            SidebarMode::None => {}
                        }
                    }
                }
                if let Some(rect) = self.ws().last_preview_rect {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        self.ws_mut().preview.scroll_down(3);
                        return;
                    }
                }
                let pane_rects = self.ws().last_pane_rects.clone();
                for (pane_id, rect) in pane_rects {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        if let Some(pane) = self.ws().panes.get(&pane_id) {
                            pane.scroll_down(3);
                        }
                        return;
                    }
                }
            }
            MouseEventKind::ScrollLeft => {
                let col = mouse.column;
                let row = mouse.row;
                if let Some(rect) = self.ws().last_preview_rect {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        self.ws_mut().preview.scroll_left(4);
                    }
                }
            }
            MouseEventKind::ScrollRight => {
                let col = mouse.column;
                let row = mouse.row;
                if let Some(rect) = self.ws().last_preview_rect {
                    if col >= rect.x
                        && col < rect.x + rect.width
                        && row >= rect.y
                        && row < rect.y + rect.height
                    {
                        self.ws_mut().preview.scroll_right(4);
                    }
                }
            }
            MouseEventKind::Moved => {
                let col = mouse.column;
                let old_hover = self.hover_border.clone();
                if self.is_on_file_tree_border(col) {
                    self.hover_border = Some(DragTarget::FileTreeBorder);
                } else if self.is_on_preview_border(col) {
                    self.hover_border = Some(DragTarget::PreviewBorder);
                } else {
                    self.hover_border = None;
                }
                if self.hover_border != old_hover {
                    self.dirty = true;
                }
            }
            _ => {}
        }
    }

    /// Forward pasted text to PTY, wrapping in bracketed paste only if
    /// the PTY application has enabled the mode (e.g. Claude Code, modern
    /// readline). Sending bracketed paste to a shell that hasn't opted in
    /// causes the escape sequences to appear as literal text (issue #2).
    pub fn forward_paste_to_pty(&mut self, text: &str) -> Result<()> {
        let focused_id = self.ws().focused_pane_id;
        if let Some(pane) = self.ws_mut().panes.get_mut(&focused_id) {
            pane.scroll_reset();
            if pane.is_bracketed_paste_enabled() {
                let mut data = Vec::with_capacity(text.len() + 12);
                data.extend_from_slice(b"\x1b[200~");
                data.extend_from_slice(text.as_bytes());
                data.extend_from_slice(b"\x1b[201~");
                pane.write_input(&data)?;
            } else {
                pane.write_input(text.as_bytes())?;
            }
        }
        Ok(())
    }

    /// Returns true if the paste was consumed by a dialog (not forwarded to PTY).
    pub fn handle_paste_in_dialog(&mut self, text: &str) -> bool {
        if self.pane_create_dialog.visible {
            match self.pane_create_dialog.focused_field {
                PaneCreateField::PromptField => {
                    // Allow newlines in pasted text; strip other control chars.
                    let filtered: String = text
                        .chars()
                        .filter(|&c| c == '\n' || !c.is_control())
                        .collect();
                    self.prompt_insert(&filtered);
                }
                PaneCreateField::BranchName => {
                    for c in text.chars() {
                        if c.is_ascii_alphanumeric() || c == '-' || c == '/' || c == '_' {
                            self.pane_create_dialog.branch_name.push(c);
                        }
                    }
                    self.dirty = true;
                }
                PaneCreateField::BaseBranch => {
                    for c in text.chars() {
                        if c.is_ascii_alphanumeric()
                            || c == '-'
                            || c == '/'
                            || c == '_'
                            || c == '.'
                        {
                            self.pane_create_dialog.base_branch.push(c);
                        }
                    }
                    self.dirty = true;
                }
                PaneCreateField::AgentField => {
                    for c in text.chars() {
                        if c.is_ascii_graphic() || c == ' ' {
                            self.pane_create_dialog.agent.push(c);
                        }
                    }
                    self.dirty = true;
                }
                _ => {}
            }
            return true;
        }
        false
    }

    #[allow(dead_code)]
    pub fn forward_key_to_pty(&mut self, key: KeyEvent) -> Result<()> {
        let focused_id = self.ws().focused_pane_id;
        if let Some(pane) = self.ws_mut().panes.get_mut(&focused_id) {
            pane.scroll_reset();
            if let Some(bytes) = key_event_to_bytes(&key) {
                pane.write_input(&bytes)?;
            }
        }
        Ok(())
    }
}
