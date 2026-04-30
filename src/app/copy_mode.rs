use super::*;

impl App {
    pub(super) fn enter_copy_mode(&mut self) {
        let pane_id = self.ws().focused_pane_id;
        let rect = self
            .ws()
            .last_pane_rects
            .iter()
            .find(|&&(id, _)| id == pane_id)
            .map(|&(_, r)| r);

        enum SizeSource {
            Rect,
            Parser,
            Default,
        }

        let (screen_rows, screen_cols, source) = if let Some(rect) = rect {
            (
                rect.height.saturating_sub(2),
                rect.width.saturating_sub(2),
                SizeSource::Rect,
            )
        } else if let Some(pane) = self.ws().panes.get(&pane_id) {
            let parser = pane.parser.lock().unwrap_or_else(|e| e.into_inner());
            let rows = parser.screen().size().0;
            let cols = parser.screen().size().1;
            (rows, cols, SizeSource::Parser)
        } else {
            (24u16, 80u16, SizeSource::Default)
        };

        match source {
            SizeSource::Parser => {
                self.status_flash = Some((
                    "copy mode: layout not determined, using parser size".to_string(),
                    std::time::Instant::now(),
                ));
            }
            SizeSource::Default => {
                self.status_flash = Some((
                    "copy mode: using default size (24x80)".to_string(),
                    std::time::Instant::now(),
                ));
            }
            SizeSource::Rect => {}
        }

        self.copy_mode = Some(CopyModeState {
            pane_id,
            cursor_row: screen_rows.saturating_sub(1),
            cursor_col: 0,
            selection_start: None,
            line_wise: false,
            screen_rows,
            screen_cols,
            first_g: false,
            scrollback_offset: 0,
        });
        self.dirty = true;
    }

    pub(super) fn handle_copy_mode_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.copy_mode.is_none() {
            return Ok(false);
        }

        let pane_id = self.copy_mode.as_ref().unwrap().pane_id;
        let max_scrollback = self
            .ws()
            .panes
            .get(&pane_id)
            .map(|p| {
                p.total_scrollback
                    .load(std::sync::atomic::Ordering::Relaxed)
            })
            .unwrap_or(0);
        let action = Self::move_copy_cursor(self.copy_mode.as_mut().unwrap(), key, max_scrollback);

        match action {
            CopyModeAction::Quit => {
                self.copy_mode = None;
            }
            CopyModeAction::Yank => {
                self.yank_selection();
                self.copy_mode = None;
            }
            CopyModeAction::Continue => {}
        }
        self.dirty = true;
        Ok(true)
    }

    pub(super) fn move_copy_cursor(
        cm: &mut CopyModeState,
        key: KeyEvent,
        max_scrollback: usize,
    ) -> CopyModeAction {
        if cm.screen_rows == 0 {
            return CopyModeAction::Continue;
        }

        let is_g = matches!(key.code, KeyCode::Char('g')) && key.modifiers == KeyModifiers::NONE;
        if !is_g {
            cm.first_g = false;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return CopyModeAction::Quit,
            KeyCode::Char('h') | KeyCode::Left => {
                cm.cursor_col = cm.cursor_col.saturating_sub(1);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                cm.cursor_col = (cm.cursor_col + 1).min(cm.screen_cols.saturating_sub(1));
            }
            KeyCode::Char('j') | KeyCode::Down if key.modifiers == KeyModifiers::NONE => {
                if cm.cursor_row >= cm.screen_rows.saturating_sub(1) && cm.scrollback_offset > 0 {
                    cm.scrollback_offset -= 1;
                } else {
                    cm.cursor_row = (cm.cursor_row + 1).min(cm.screen_rows.saturating_sub(1));
                }
            }
            KeyCode::Char('k') | KeyCode::Up if key.modifiers == KeyModifiers::NONE => {
                if cm.cursor_row == 0 && cm.scrollback_offset < max_scrollback {
                    cm.scrollback_offset += 1;
                } else {
                    cm.cursor_row = cm.cursor_row.saturating_sub(1);
                }
            }
            KeyCode::Char('0') => cm.cursor_col = 0,
            KeyCode::Char('$') => cm.cursor_col = cm.screen_cols.saturating_sub(1),
            KeyCode::Char('g') if key.modifiers == KeyModifiers::NONE => {
                if cm.first_g {
                    cm.cursor_row = 0;
                    cm.first_g = false;
                } else {
                    cm.first_g = true;
                }
            }
            KeyCode::Char('G') => {
                cm.cursor_row = cm.screen_rows.saturating_sub(1);
            }
            KeyCode::Char('v') if key.modifiers == KeyModifiers::NONE => {
                if cm.selection_start.is_some() && !cm.line_wise {
                    cm.selection_start = None;
                } else {
                    cm.selection_start = Some((cm.cursor_row, cm.cursor_col));
                    cm.line_wise = false;
                }
            }
            KeyCode::Char('V') => {
                if cm.selection_start.is_some() && cm.line_wise {
                    cm.selection_start = None;
                    cm.line_wise = false;
                } else {
                    cm.selection_start = Some((cm.cursor_row, 0));
                    cm.line_wise = true;
                }
            }
            KeyCode::Char('y') | KeyCode::Enter => return CopyModeAction::Yank,
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = cm.screen_rows / 2;
                if cm.cursor_row < half && cm.scrollback_offset < max_scrollback {
                    let remaining = half - cm.cursor_row;
                    cm.scrollback_offset =
                        (cm.scrollback_offset + remaining as usize).min(max_scrollback);
                    cm.cursor_row = 0;
                } else {
                    cm.cursor_row = cm.cursor_row.saturating_sub(half);
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = cm.screen_rows / 2;
                let bottom = cm.screen_rows.saturating_sub(1);
                if cm.cursor_row + half > bottom && cm.scrollback_offset > 0 {
                    let overflow = (cm.cursor_row + half) - bottom;
                    cm.scrollback_offset = cm.scrollback_offset.saturating_sub(overflow as usize);
                    cm.cursor_row = bottom;
                } else {
                    cm.cursor_row = (cm.cursor_row + half).min(bottom);
                }
            }
            _ => {}
        }
        CopyModeAction::Continue
    }

    pub(super) fn yank_selection(&mut self) {
        let cm = match self.copy_mode.as_ref() {
            Some(cm) => cm.clone(),
            None => return,
        };

        let pane_id = cm.pane_id;
        let parser_arc = match self.ws().panes.get(&pane_id) {
            Some(p) => std::sync::Arc::clone(&p.parser),
            None => return,
        };
        let text = {
            let mut parser = match parser_arc.lock() {
                Ok(guard) => guard,
                Err(_poisoned) => {
                    self.status_flash = Some((
                        "warning: terminal state may be corrupted".to_string(),
                        std::time::Instant::now(),
                    ));
                    return;
                }
            };

            let original_scrollback = parser.screen().scrollback();
            parser.screen_mut().set_scrollback(cm.scrollback_offset);
            let screen = parser.screen();

            let (start_row, start_col, end_row, end_col) =
                if let Some((sr, sc)) = cm.selection_start {
                    let min_r = sr.min(cm.cursor_row);
                    let max_r = sr.max(cm.cursor_row);
                    if cm.line_wise {
                        (min_r, 0u16, max_r, cm.screen_cols)
                    } else {
                        let (sc_norm, ec_norm) = if sr <= cm.cursor_row {
                            (sc, cm.cursor_col)
                        } else {
                            (cm.cursor_col, sc)
                        };
                        let end_col = ec_norm.saturating_add(1).min(cm.screen_cols);
                        (min_r, sc_norm, max_r, end_col)
                    }
                } else {
                    (cm.cursor_row, 0, cm.cursor_row, cm.screen_cols)
                };

            let mut lines = Vec::new();
            for row in start_row..=end_row {
                let col_start = if !cm.line_wise && row == start_row {
                    start_col
                } else {
                    0
                };
                let col_end = if !cm.line_wise && row == end_row {
                    end_col
                } else {
                    cm.screen_cols
                };
                let mut line = String::new();
                for col in col_start..col_end {
                    if let Some(cell) = screen.cell(row, col) {
                        let contents = cell.contents();
                        if contents.is_empty() {
                            line.push(' ');
                        } else {
                            line.push_str(contents);
                        }
                    }
                }
                lines.push(line.trim_end().to_string());
            }

            parser.screen_mut().set_scrollback(original_scrollback);

            lines.join("\n")
        };

        if !text.is_empty() {
            self.copy_to_clipboard(&text);
            let line_count = text.lines().count();
            self.status_flash = Some((
                format!("Copied ({} lines)", line_count),
                std::time::Instant::now(),
            ));
        }
    }
}
