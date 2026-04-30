use super::*;

impl App {
    /// Recompute pane rectangles and apply sizes to every PTY in the
    /// active workspace. Returns `true` if any pane was actually
    /// resized (so callers can decide whether to enter the post-resize
    /// cooldown). Safe to call without a Frame — uses the cached
    /// `last_term_size`.
    pub fn relayout_panes(&mut self) -> bool {
        let (cols, rows) = self.last_term_size;
        if cols < 20 || rows < 5 {
            return false;
        }

        // Mirror the area math in ui::render / render_main_area,
        // including the fallback where tree / preview are hidden when
        // the terminal is too narrow. Keeping these in sync prevents
        // PTY size drift from the actually-painted pane size.
        const MIN_PANE_AREA_WIDTH: u16 = 20;
        let tab_h = 1u16;
        let status_h: u16 = if self.status_bar_visible
            || self.rename_input.is_some()
            || self.pane_rename_input.is_some()
        {
            1
        } else {
            0
        };
        let main_h = rows.saturating_sub(tab_h + status_h);

        let mut has_tree = self.ws().sidebar_mode != SidebarMode::None;
        let mut has_preview = self.ws().preview.is_active();
        let tree_w_nom = self.file_tree_width;
        let preview_w_nom = self.preview_width;

        let needed = MIN_PANE_AREA_WIDTH
            + if has_tree { tree_w_nom } else { 0 }
            + if has_preview { preview_w_nom } else { 0 };
        if cols < needed && has_preview {
            has_preview = false;
        }
        let needed = MIN_PANE_AREA_WIDTH + if has_tree { tree_w_nom } else { 0 };
        if cols < needed && has_tree {
            has_tree = false;
        }

        let tree_w = if has_tree { tree_w_nom } else { 0 };
        let preview_w = if has_preview { preview_w_nom } else { 0 };
        let pane_w = cols.saturating_sub(tree_w).saturating_sub(preview_w);

        // x/y exact values don't matter for calculate_rects' sub-areas,
        // only width/height propagate into the recursive split sizes.
        let pane_area = Rect::new(0, tab_h, pane_w, main_h);
        let rects = self.ws().layout.calculate_rects(pane_area);

        let mut any_changed = false;
        for (pane_id, rect) in &rects {
            if let Some(pane) = self.ws_mut().panes.get_mut(pane_id) {
                let inner_rows = rect.height.saturating_sub(2);
                let inner_cols = rect.width.saturating_sub(2);
                if pane.resize(inner_rows, inner_cols).unwrap_or(false) {
                    any_changed = true;
                }
            }
        }

        self.ws_mut().last_pane_rects = rects;
        any_changed
    }

    /// Mark a layout change: apply resizes immediately and, if sizes
    /// actually changed, delay the next paint for a few frames so the
    /// PTY child can respond to SIGWINCH with a fresh redraw before
    /// we render. When no size changes happen (e.g. a sidebar toggle
    /// that fits in the same remaining width) we skip the cooldown so
    /// the UI stays responsive. Also drops any live selection, whose
    /// stored `content_rect` / `pane_id` could reference a layout that
    /// no longer exists.
    pub fn mark_layout_change(&mut self) {
        let changed = self.relayout_panes();
        if changed {
            // Take max so a freshly-triggered layout change on top of
            // an existing cooldown doesn't prematurely cut the wait.
            self.resize_cooldown = self.resize_cooldown.max(5);
        }
        // Any in-flight selection is bound to the old geometry.
        self.selection = None;
        self.dirty = true;
    }

    /// Called from main.rs on crossterm Resize events so we can update
    /// the cached terminal size and propagate the resize into panes.
    pub fn on_terminal_resize(&mut self, cols: u16, rows: u16) {
        self.last_term_size = (cols, rows);
        self.mark_layout_change();
    }

    pub fn apply_layout_mode(&mut self, mode: LayoutMode) {
        let pane_ids = self.ws().layout.collect_pane_ids();
        if pane_ids.is_empty() {
            return;
        }

        let new_layout = Self::build_layout_node(mode, &pane_ids);
        if let Some(layout) = new_layout {
            self.ws_mut().layout = layout;
            self.ws_mut().layout_mode = mode;
            self.mark_layout_change();
        }
    }

    pub(super) fn build_layout_node(mode: LayoutMode, pane_ids: &[usize]) -> Option<LayoutNode> {
        let count = pane_ids.len();
        if count == 0 {
            return None;
        }
        if count == 1 {
            return Some(LayoutNode::Leaf {
                pane_id: pane_ids[0],
            });
        }

        match mode {
            LayoutMode::Stack | LayoutMode::Auto => {
                Self::build_stack(pane_ids, SplitDirection::Horizontal)
            }
            LayoutMode::TwoSplit => {
                let left = LayoutNode::Leaf {
                    pane_id: pane_ids[0],
                };
                let right = if count == 2 {
                    LayoutNode::Leaf {
                        pane_id: pane_ids[1],
                    }
                } else {
                    Self::build_stack(&pane_ids[1..], SplitDirection::Horizontal).unwrap_or(
                        LayoutNode::Leaf {
                            pane_id: pane_ids[1],
                        },
                    )
                };
                Some(LayoutNode::Split {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                    first: Box::new(left),
                    second: Box::new(right),
                })
            }
            LayoutMode::Grid => {
                if count >= 4 {
                    let top = LayoutNode::Split {
                        direction: SplitDirection::Vertical,
                        ratio: 0.5,
                        first: Box::new(LayoutNode::Leaf {
                            pane_id: pane_ids[0],
                        }),
                        second: Box::new(LayoutNode::Leaf {
                            pane_id: pane_ids[1],
                        }),
                    };
                    let bottom = LayoutNode::Split {
                        direction: SplitDirection::Vertical,
                        ratio: 0.5,
                        first: Box::new(LayoutNode::Leaf {
                            pane_id: pane_ids[2],
                        }),
                        second: Box::new(LayoutNode::Leaf {
                            pane_id: pane_ids[3],
                        }),
                    };
                    Some(LayoutNode::Split {
                        direction: SplitDirection::Horizontal,
                        ratio: 0.5,
                        first: Box::new(top),
                        second: Box::new(bottom),
                    })
                } else if count == 3 {
                    let top = LayoutNode::Split {
                        direction: SplitDirection::Vertical,
                        ratio: 0.5,
                        first: Box::new(LayoutNode::Leaf {
                            pane_id: pane_ids[0],
                        }),
                        second: Box::new(LayoutNode::Leaf {
                            pane_id: pane_ids[1],
                        }),
                    };
                    Some(LayoutNode::Split {
                        direction: SplitDirection::Horizontal,
                        ratio: 0.5,
                        first: Box::new(top),
                        second: Box::new(LayoutNode::Leaf {
                            pane_id: pane_ids[2],
                        }),
                    })
                } else {
                    Self::build_layout_node(LayoutMode::TwoSplit, pane_ids)
                }
            }
            LayoutMode::MainSub => {
                if count >= 3 {
                    let main = LayoutNode::Leaf {
                        pane_id: pane_ids[0],
                    };
                    let sub = Self::build_stack(&pane_ids[1..], SplitDirection::Horizontal)
                        .unwrap_or(LayoutNode::Leaf {
                            pane_id: pane_ids[1],
                        });
                    Some(LayoutNode::Split {
                        direction: SplitDirection::Vertical,
                        ratio: 0.6,
                        first: Box::new(main),
                        second: Box::new(sub),
                    })
                } else {
                    Self::build_layout_node(LayoutMode::TwoSplit, pane_ids)
                }
            }
            LayoutMode::BigOnePlusThree => {
                if count >= 4 {
                    let big = LayoutNode::Leaf {
                        pane_id: pane_ids[0],
                    };
                    let small = Self::build_stack(&pane_ids[1..4], SplitDirection::Horizontal)
                        .unwrap_or(LayoutNode::Leaf {
                            pane_id: pane_ids[1],
                        });
                    Some(LayoutNode::Split {
                        direction: SplitDirection::Vertical,
                        ratio: 0.65,
                        first: Box::new(big),
                        second: Box::new(small),
                    })
                } else if count == 3 {
                    Self::build_layout_node(LayoutMode::MainSub, pane_ids)
                } else {
                    Self::build_layout_node(LayoutMode::TwoSplit, pane_ids)
                }
            }
        }
    }

    pub(super) fn build_stack(pane_ids: &[usize], direction: SplitDirection) -> Option<LayoutNode> {
        match pane_ids.len() {
            0 => None,
            1 => Some(LayoutNode::Leaf {
                pane_id: pane_ids[0],
            }),
            2 => Some(LayoutNode::Split {
                direction,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf {
                    pane_id: pane_ids[0],
                }),
                second: Box::new(LayoutNode::Leaf {
                    pane_id: pane_ids[1],
                }),
            }),
            _ => {
                let mid = pane_ids.len() / 2;
                let first = Self::build_stack(&pane_ids[..mid], direction)?;
                let second = Self::build_stack(&pane_ids[mid..], direction)?;
                Some(LayoutNode::Split {
                    direction,
                    ratio: mid as f32 / pane_ids.len() as f32,
                    first: Box::new(first),
                    second: Box::new(second),
                })
            }
        }
    }

    pub(super) fn cycle_layout_mode(&mut self) {
        let count = self.ws().layout.pane_count();
        if count <= 1 {
            return;
        }

        let current = self.ws().layout_mode;
        let modes: &[LayoutMode] = if count == 2 {
            &[LayoutMode::Stack, LayoutMode::TwoSplit]
        } else if count == 3 {
            &[LayoutMode::Stack, LayoutMode::TwoSplit, LayoutMode::MainSub]
        } else {
            &[
                LayoutMode::Stack,
                LayoutMode::TwoSplit,
                LayoutMode::Grid,
                LayoutMode::MainSub,
                LayoutMode::BigOnePlusThree,
            ]
        };

        let next = if current == LayoutMode::Auto {
            modes[0]
        } else {
            modes
                .iter()
                .position(|&m| m == current)
                .map(|idx| modes[(idx + 1) % modes.len()])
                .unwrap_or(modes[0])
        };

        self.apply_layout_mode(next);
    }

    pub(super) fn handle_layout_picker_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.key_matches(key, &self.config.keybindings.quit) {
            self.should_quit = true;
            return Ok(true);
        }

        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Char('1')) => {
                self.apply_layout_mode(LayoutMode::Stack);
                self.layout_picker.visible = false;
            }
            (KeyModifiers::NONE, KeyCode::Char('2')) => {
                self.apply_layout_mode(LayoutMode::TwoSplit);
                self.layout_picker.visible = false;
            }
            (KeyModifiers::NONE, KeyCode::Char('3')) => {
                self.apply_layout_mode(LayoutMode::Grid);
                self.layout_picker.visible = false;
            }
            (KeyModifiers::NONE, KeyCode::Char('4')) => {
                self.apply_layout_mode(LayoutMode::MainSub);
                self.layout_picker.visible = false;
            }
            (KeyModifiers::NONE, KeyCode::Char('5')) => {
                self.apply_layout_mode(LayoutMode::BigOnePlusThree);
                self.layout_picker.visible = false;
            }
            (KeyModifiers::NONE, KeyCode::Char('6')) => {
                self.apply_layout_mode(LayoutMode::Auto);
                self.layout_picker.visible = false;
            }
            (KeyModifiers::NONE, KeyCode::Esc) | (KeyModifiers::NONE, KeyCode::Char('q')) => {
                self.layout_picker.visible = false;
            }
            (KeyModifiers::NONE, KeyCode::Char('j')) | (KeyModifiers::NONE, KeyCode::Down) => {
                self.layout_picker.selected = (self.layout_picker.selected + 1) % 6;
            }
            (KeyModifiers::NONE, KeyCode::Char('k')) | (KeyModifiers::NONE, KeyCode::Up) => {
                self.layout_picker.selected = self.layout_picker.selected.saturating_sub(1);
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                let mode = match self.layout_picker.selected {
                    0 => LayoutMode::Stack,
                    1 => LayoutMode::TwoSplit,
                    2 => LayoutMode::Grid,
                    3 => LayoutMode::MainSub,
                    4 => LayoutMode::BigOnePlusThree,
                    _ => LayoutMode::Auto,
                };
                self.apply_layout_mode(mode);
                self.layout_picker.visible = false;
            }
            (KeyModifiers::NONE, KeyCode::Char(' ')) => {
                self.layout_picker.selected = (self.layout_picker.selected + 1) % 6;
            }
            _ => {}
        }
        self.dirty = true;
        Ok(true)
    }

    pub(super) fn handle_feature_toggle_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.key_matches(key, &self.config.keybindings.quit) {
            self.should_quit = true;
            return Ok(true);
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.feature_toggle.selected = (self.feature_toggle.selected + 1) % FEATURES.len();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.feature_toggle.selected = if self.feature_toggle.selected == 0 {
                    FEATURES.len() - 1
                } else {
                    self.feature_toggle.selected - 1
                };
            }
            KeyCode::Char(' ') => {
                let (key_name, _) = FEATURES[self.feature_toggle.selected];
                let current = self.feature_toggle.pending.get_by_key(key_name);
                self.feature_toggle.pending.set_by_key(key_name, !current);
            }
            // Only close dialog on plain 'q' (no modifiers) or Enter
            KeyCode::Char('q') if key.modifiers == KeyModifiers::NONE => {
                self.config.features = self.feature_toggle.pending.clone();
                self.ai_title_enabled = self.config.features.ai_title;
                self.feature_toggle.visible = false;
                if let Err(e) = self.config.save() {
                    eprintln!("glowmux: config save error: {}", e);
                }
            }
            KeyCode::Enter => {
                self.config.features = self.feature_toggle.pending.clone();
                self.ai_title_enabled = self.config.features.ai_title;
                self.feature_toggle.visible = false;
                if let Err(e) = self.config.save() {
                    eprintln!("glowmux: config save error: {}", e);
                }
            }
            KeyCode::Esc => {
                self.feature_toggle.visible = false;
            }
            _ => {}
        }
        self.dirty = true;
        Ok(true)
    }

    pub(super) fn handle_settings_panel_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.settings_panel.editing {
            match key.code {
                KeyCode::Enter => {
                    let Some(&(key_name, _)) = SETTINGS_ITEMS.get(self.settings_panel.selected)
                    else {
                        self.settings_panel.editing = false;
                        self.settings_panel.edit_buffer.clear();
                        self.dirty = true;
                        return Ok(true);
                    };
                    let buf = self.settings_panel.edit_buffer.clone();
                    self.set_setting_value(key_name, &buf);
                    if let Err(e) = self.config.save() {
                        eprintln!("glowmux: config save error: {}", e);
                    }
                    self.settings_panel.editing = false;
                    self.settings_panel.edit_buffer.clear();
                }
                KeyCode::Esc => {
                    self.settings_panel.editing = false;
                    self.settings_panel.edit_buffer.clear();
                }
                KeyCode::Backspace => {
                    self.settings_panel.edit_buffer.pop();
                }
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    if self.settings_panel.edit_buffer.len() < 10 {
                        self.settings_panel.edit_buffer.push(c);
                    }
                }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.settings_panel.selected =
                        (self.settings_panel.selected + 1) % SETTINGS_ITEMS.len();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.settings_panel.selected = if self.settings_panel.selected == 0 {
                        SETTINGS_ITEMS.len() - 1
                    } else {
                        self.settings_panel.selected - 1
                    };
                }
                KeyCode::Enter => {
                    if let Some(&(key_name, _)) = SETTINGS_ITEMS.get(self.settings_panel.selected) {
                        self.settings_panel.edit_buffer = self.get_setting_value(key_name);
                        self.settings_panel.editing = true;
                    }
                }
                KeyCode::Char('q') | KeyCode::Esc => {
                    self.settings_panel.visible = false;
                }
                _ => {}
            }
        }
        self.dirty = true;
        Ok(true)
    }

    pub fn get_setting_value(&self, key: &str) -> String {
        match key {
            "terminal.scrollback" => self.config.terminal.scrollback.to_string(),
            "layout.breakpoint_stack" => self.config.layout.breakpoint_stack.to_string(),
            "layout.breakpoint_split2" => self.config.layout.breakpoint_split2.to_string(),
            "ai_title_engine.update_interval_sec" => {
                self.config.ai_title_engine.update_interval_sec.to_string()
            }
            "ai_title_engine.max_chars" => self.config.ai_title_engine.max_chars.to_string(),
            _ => String::new(),
        }
    }

    pub(super) fn set_setting_value(&mut self, key: &str, value: &str) {
        match key {
            "terminal.scrollback" => {
                if let Ok(v) = value.parse::<usize>() {
                    self.config.terminal.scrollback = v.clamp(100, 1_000_000);
                }
            }
            "layout.breakpoint_stack" => {
                if let Ok(v) = value.parse::<u16>() {
                    self.config.layout.breakpoint_stack = v.max(40);
                }
            }
            "layout.breakpoint_split2" => {
                if let Ok(v) = value.parse::<u16>() {
                    self.config.layout.breakpoint_split2 = v.max(40);
                }
            }
            "ai_title_engine.update_interval_sec" => {
                if let Ok(v) = value.parse::<u64>() {
                    self.config.ai_title_engine.update_interval_sec = v.max(5);
                }
            }
            "ai_title_engine.max_chars" => {
                if let Ok(v) = value.parse::<usize>() {
                    self.config.ai_title_engine.max_chars = v.clamp(5, 200);
                }
            }
            _ => {}
        }
    }

    pub(super) fn is_on_file_tree_border(&self, col: u16) -> bool {
        if let Some(rect) = self.ws().last_file_tree_rect {
            let border_col = rect.x + rect.width;
            col >= border_col.saturating_sub(1) && col <= border_col
        } else {
            false
        }
    }

    pub(super) fn is_on_preview_border(&self, col: u16) -> bool {
        if let Some(rect) = self.ws().last_preview_rect {
            // When swapped: [tree][preview][panes] → drag the RIGHT edge of preview
            // When normal:  [tree][panes][preview] → drag the LEFT edge of preview
            let border_col = if self.layout_swapped {
                rect.x + rect.width
            } else {
                rect.x
            };
            col >= border_col.saturating_sub(1) && col <= border_col
        } else {
            false
        }
    }
}
