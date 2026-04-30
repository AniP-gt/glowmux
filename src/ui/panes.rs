use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{CopyModeState, PaneStatus};

use super::{
    make_progress_bar, format_tokens, truncate_to_width,
    BG, BORDER, FOCUS_BORDER, TEXT_DIM, ACCENT_GREEN, ACCENT_BLUE, ACCENT_CLAUDE,
    SCROLL_BG,
};

pub(super) fn str_to_border_type(s: &str) -> BorderType {
    match s {
        "plain" => BorderType::Plain,
        "double" => BorderType::Double,
        "thick" => BorderType::Thick,
        "none" => BorderType::Plain, // ratatui has no "none" variant; borders hidden via Borders::NONE
        _ => BorderType::Rounded,
    }
}

pub(super) fn parse_color_str(s: &str) -> Option<Color> {
    let s = s.trim();
    if s.is_empty() || s == "reset" {
        return None;
    }
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        }
    }
    match s {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        "gray" | "grey" => Some(Color::Gray),
        _ => None,
    }
}

/// Colors derived from the [status] config section.
pub(super) struct StatusColors {
    pub(super) running: Color,
    pub(super) done: Color,
    pub(super) waiting: Color,
    pub(super) bg_done: Color,
    pub(super) bg_waiting: Color,
}

impl StatusColors {
    pub(super) fn from_config(cfg: &crate::config::StatusConfig) -> Self {
        let running = parse_color_str(&cfg.color_running).unwrap_or(Color::Cyan);
        let done = parse_color_str(&cfg.color_done).unwrap_or(Color::Green);
        let waiting = parse_color_str(&cfg.color_waiting).unwrap_or(Color::Yellow);
        // override_bg_* takes precedence; "reset"/empty falls back to the non-override value,
        // then to BG so that "reset" semantics map to the app background rather than a magic color.
        let bg_done_str = if cfg.override_bg_done.is_empty() {
            &cfg.bg_done
        } else {
            &cfg.override_bg_done
        };
        let bg_waiting_str = if cfg.override_bg_waiting.is_empty() {
            &cfg.bg_waiting
        } else {
            &cfg.override_bg_waiting
        };
        let bg_done = parse_color_str(bg_done_str).unwrap_or(BG);
        let bg_waiting = parse_color_str(bg_waiting_str).unwrap_or(BG);
        Self {
            running,
            done,
            waiting,
            bg_done,
            bg_waiting,
        }
    }
}

pub(super) fn vt100_color_to_ratatui(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(idx) => Color::Indexed(idx),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

pub(super) fn render_panes(app: &mut crate::app::App, frame: &mut Frame, area: Rect) {
    let rects = app.ws().layout.calculate_rects(area);
    app.ws_mut().last_pane_rects = rects.clone();

    for &(pane_id, rect) in &rects {
        if let Some(pane) = app.ws_mut().panes.get_mut(&pane_id) {
            let inner_rows = rect.height.saturating_sub(2);
            let inner_cols = rect.width.saturating_sub(2);
            let _: anyhow::Result<_> = pane.resize(inner_rows, inner_cols); // now returns Result<bool>
        }
    }

    // Update Claude monitor for each pane using the pane's own cwd
    // (may differ from workspace cwd if user cd'd inside the pane)
    let pane_cwds: Vec<(usize, std::path::PathBuf)> = rects
        .iter()
        .filter_map(|&(pane_id, _)| {
            app.ws()
                .panes
                .get(&pane_id)
                .map(|p: &crate::pane::Pane| (pane_id, p.pane_cwd()))
        })
        .collect();
    let mut cwd_counts: std::collections::HashMap<std::path::PathBuf, usize> = std::collections::HashMap::new();
    for (_, cwd) in &pane_cwds {
        *cwd_counts.entry(cwd.clone()).or_insert(0_usize) += 1;
    }
    for (pane_id, cwd) in pane_cwds {
        let allow_cwd_fallback = cwd_counts.get(&cwd).copied().unwrap_or(0) <= 1;
        app.claude_monitor.update(pane_id, &cwd, allow_cwd_fallback);
    }

    let focused_id = app.ws().focused_pane_id;
    let focus_target = app.ws().focus_target;
    let selection = app.selection.clone();
    let show_status_dot = app.config.features.status_dot;
    // When status_bg_color is explicitly enabled, force it regardless of respect_terminal_bg.
    let show_status_bg = app.config.features.status_bg_color;
    let show_pane_numbers = app.config.pane.show_pane_numbers;
    let border_type = str_to_border_type(&app.config.pane.border_style);
    let status_colors = StatusColors::from_config(&app.config.status);
    let copy_mode = app.copy_mode.clone();
    for (pane_id, rect) in rects {
        if let Some(pane) = app.ws().panes.get(&pane_id) {
            let is_focused = pane_id == focused_id && focus_target == crate::app::FocusTarget::Pane;
            let pane_sel = selection.as_ref().filter(
                |s| matches!(s.target, crate::app::SelectionTarget::Pane(id) if id == pane_id),
            );
            let claude_state = app.claude_monitor.state(pane_id);
            let pane_status = app.pane_status(pane_id);
            let dismissed = app.pane_state_dismissed(pane_id);
            let ai_title = app.pane_display_title(pane_id).map(|s: &str| s.to_string());
            let rename_buf = app.pane_rename_input.as_ref().and_then(|(id, buf): &(usize, String)| {
                if *id == pane_id {
                    Some(buf.clone())
                } else {
                    None
                }
            });
            let pane_copy_mode = copy_mode.as_ref().filter(|c| c.pane_id == pane_id);
            render_single_pane(
                pane,
                is_focused,
                pane_sel,
                &claude_state,
                pane_status,
                dismissed,
                show_status_dot,
                show_status_bg,
                show_pane_numbers,
                border_type,
                &status_colors,
                ai_title.as_deref(),
                rename_buf.as_deref(),
                pane_copy_mode,
                frame,
                rect,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_single_pane(
    pane: &crate::pane::Pane,
    is_focused: bool,
    selection: Option<&crate::app::TextSelection>,
    claude_state: &crate::claude_monitor::ClaudeState,
    pane_status: PaneStatus,
    dismissed: bool,
    show_status_dot: bool,
    show_status_bg: bool,
    show_pane_numbers: bool,
    border_type: BorderType,
    status_colors: &StatusColors,
    ai_title: Option<&str>,
    rename_cursor: Option<&str>,
    copy_mode: Option<&CopyModeState>,
    frame: &mut Frame,
    area: Rect,
) {
    let is_claude = pane.is_claude_running();
    let in_copy_mode = copy_mode.is_some();
    let border_color = if in_copy_mode {
        Color::Yellow
    } else if is_focused && is_claude {
        ACCENT_CLAUDE
    } else if is_focused {
        FOCUS_BORDER
    } else {
        BORDER
    };

    let is_scrolled = pane.is_scrolled_back();
    let label = if let Some(buf) = rename_cursor {
        format!("{}\u{2588}", buf)
    } else {
        let base_label = if let Some(title) = ai_title {
            title.to_string()
        } else if is_claude {
            "claude".to_string()
        } else {
            "shell".to_string()
        };
        // Spec v5: append " ⌇ {branch}" to the pane title when a branch is bound.
        // Truncate the base label first so the branch always fits within a sensible
        // width budget — otherwise long AI titles can push the branch off-screen.
        match pane.branch_name.as_deref() {
            Some(branch) if !branch.is_empty() => {
                let truncated_base = truncate_to_width(&base_label, 24);
                format!("{} \u{2307} {}", truncated_base, branch)
            }
            _ => base_label,
        }
    };

    let claude_suffix = if is_claude {
        let mut parts = Vec::new();
        if claude_state.subagent_count > 0 {
            if !claude_state.subagent_types.is_empty() {
                parts.push(format!(
                    "\u{1f916} {}",
                    claude_state.subagent_types.join(", ")
                ));
            } else {
                parts.push(format!("\u{1f916}\u{00d7}{}", claude_state.subagent_count));
            }
        }
        if let Some(ref tool) = claude_state.current_tool {
            parts.push(format!("\u{1f527} {}", tool));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!(" {} ", parts.join(" "))
        }
    } else {
        String::new()
    };

    // Indicator: always visible (not just when focused) so activity is trackable across panes.
    // Colored ● using config colors instead of emoji (emoji rendering is terminal-dependent).
    let (dot_text, dot_color): (&str, Option<Color>) = if show_status_dot && !dismissed {
        match pane_status {
            PaneStatus::Idle => ("", None),
            PaneStatus::Running => ("\u{25cf} ", Some(status_colors.running)),
            PaneStatus::Done => ("\u{25cf} ", Some(status_colors.done)),
            PaneStatus::Waiting => ("\u{25cf} ", Some(status_colors.waiting)),
        }
    } else {
        ("", None)
    };

    let id_part = if show_pane_numbers {
        format!(" [{}]", pane.id)
    } else {
        String::new()
    };
    let copy_label = if in_copy_mode { "[COPY] " } else { "" };

    let label_style = if in_copy_mode {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else if is_focused && is_claude {
        Style::default()
            .fg(ACCENT_CLAUDE)
            .add_modifier(Modifier::BOLD)
    } else if is_focused {
        Style::default()
            .fg(FOCUS_BORDER)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_DIM)
    };

    // Build title as a Line so the indicator dot can have its own color.
    // Focused panes show a focus marker (▶) separate from the status dot.
    // Unfocused panes show the status dot in place of the first character.
    let pane_title: Line = if is_focused || in_copy_mode {
        let focus_marker = Span::styled(format!(" \u{25b6} {}", copy_label), label_style);
        let rest = Span::styled(
            format!("{}{}{} ", label, id_part, claude_suffix),
            label_style,
        );
        if let Some(c) = dot_color {
            Line::from(vec![
                focus_marker,
                Span::styled(dot_text, Style::default().fg(c)),
                rest,
            ])
        } else {
            Line::from(vec![focus_marker, rest])
        }
    } else if let Some(c) = dot_color {
        Line::from(vec![
            Span::styled(" ", label_style),
            Span::styled(dot_text, Style::default().fg(c)),
            Span::styled(
                format!("{}{}{} ", label, id_part, claude_suffix),
                label_style,
            ),
        ])
    } else {
        Line::from(Span::styled(
            format!("   {}{}{} ", label, id_part, claude_suffix),
            label_style,
        ))
    };

    // Bottom title: scroll indicator OR claude stats
    let bottom_title = if is_scrolled {
        Line::from(Span::styled(
            " \u{2191} SCROLL ",
            Style::default()
                .fg(ACCENT_CLAUDE)
                .bg(SCROLL_BG)
                .add_modifier(Modifier::BOLD),
        ))
    } else if is_claude {
        let mut spans = Vec::new();

        // Todo progress bar
        let (completed, total) = claude_state.todo_progress();
        if total > 0 {
            let bar = make_progress_bar(completed, total, 10);
            spans.push(Span::styled(
                format!(" \u{2713} {} {}/{} ", bar, completed, total),
                Style::default().fg(ACCENT_GREEN),
            ));
            // Show current in-progress task
            if let Some(current) = claude_state
                .todos
                .iter()
                .find(|t| t.status == "in_progress")
            {
                let short = truncate_to_width(&current.content, 30);
                spans.push(Span::styled(
                    format!("\u{25b6} {} ", short),
                    Style::default().fg(ACCENT_BLUE),
                ));
            }
        }

        // Total tokens used this session
        let total_tokens = claude_state.total_tokens();
        if total_tokens > 0 {
            spans.push(Span::styled(
                format!(" {} tokens ", format_tokens(total_tokens)),
                Style::default().fg(TEXT_DIM),
            ));
        }

        Line::from(spans)
    } else {
        Line::from("")
    };

    let pane_bg = if show_status_bg && !dismissed {
        match pane_status {
            PaneStatus::Done => status_colors.bg_done,
            PaneStatus::Waiting => status_colors.bg_waiting,
            _ => BG,
        }
    } else {
        BG
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(Style::default().fg(border_color))
        .title(pane_title)
        .title_bottom(bottom_title)
        .style(Style::default().bg(pane_bg));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if pane.exited {
        let msg = Paragraph::new("\u{2718} Process exited")
            .style(Style::default().fg(TEXT_DIM).bg(BG))
            .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(msg, inner);
    } else {
        render_terminal_content(pane, is_focused, selection, copy_mode, frame, inner);
    }
}

pub(super) fn render_terminal_content(
    pane: &crate::pane::Pane,
    is_focused: bool,
    selection: Option<&crate::app::TextSelection>,
    copy_mode: Option<&CopyModeState>,
    frame: &mut Frame,
    area: Rect,
) {
    let mut parser = pane.parser.lock().unwrap_or_else(|e| e.into_inner());
    let original_scrollback = parser.screen().scrollback();
    if let Some(cm) = copy_mode {
        parser.screen_mut().set_scrollback(cm.scrollback_offset);
    }
    let screen = parser.screen();

    let rows = area.height as usize;
    let cols = area.width as usize;
    let buf = frame.buffer_mut();

    for row in 0..rows {
        for col in 0..cols {
            let cell = screen.cell(row as u16, col as u16);
            if let Some(cell) = cell {
                let x = area.x + col as u16;
                let y = area.y + row as u16;

                let contents = cell.contents();
                let display_char = if contents.is_empty() { " " } else { contents };

                let fg = vt100_color_to_ratatui(cell.fgcolor());
                let bg = vt100_color_to_ratatui(cell.bgcolor());

                let mut modifiers = Modifier::empty();
                if cell.bold() {
                    modifiers |= Modifier::BOLD;
                }
                if cell.italic() {
                    modifiers |= Modifier::ITALIC;
                }
                if cell.underline() {
                    modifiers |= Modifier::UNDERLINED;
                }

                let style = if cell.inverse() {
                    Style::default().fg(bg).bg(fg).add_modifier(modifiers)
                } else {
                    Style::default().fg(fg).bg(bg).add_modifier(modifiers)
                };

                // Apply selection highlight (only if dragged, not single click)
                let has_selection = selection.is_some_and(|s: &crate::app::TextSelection| {
                    let (sr, sc, er, ec) = s.normalized();
                    (sr != er || sc != ec) && s.contains(row as u32, col as u32)
                });

                let copy_cursor = copy_mode
                    .is_some_and(|cm: &CopyModeState| cm.cursor_row == row as u16 && cm.cursor_col == col as u16);
                let copy_selected = copy_mode.is_some_and(|cm: &CopyModeState| {
                    if let Some((sr, sc)) = cm.selection_start {
                        let min_row = sr.min(cm.cursor_row);
                        let max_row = sr.max(cm.cursor_row);
                        let r = row as u16;
                        let c = col as u16;
                        if r < min_row || r > max_row {
                            return false;
                        }
                        if cm.line_wise {
                            return true;
                        }
                        let (start_col, end_col) = if sr <= cm.cursor_row {
                            (sc, cm.cursor_col)
                        } else {
                            (cm.cursor_col, sc)
                        };
                        if min_row == max_row {
                            c >= start_col && c <= end_col
                        } else if r == min_row {
                            c >= start_col
                        } else if r == max_row {
                            c <= end_col
                        } else {
                            true
                        }
                    } else {
                        false
                    }
                });

                let final_style = if copy_cursor {
                    Style::default().fg(Color::Black).bg(Color::Yellow)
                } else if copy_selected {
                    Style::default()
                        .fg(style.fg.unwrap_or(Color::Reset))
                        .bg(Color::Rgb(0x26, 0x4f, 0x78))
                } else if has_selection {
                    Style::default()
                        .fg(Color::Rgb(0x0d, 0x11, 0x17))
                        .bg(Color::Rgb(0x58, 0xa6, 0xff))
                } else {
                    style
                };

                if let Some(buf_cell) = buf.cell_mut((x, y)) {
                    buf_cell.set_symbol(display_char);
                    buf_cell.set_style(final_style);
                }
            }
        }
    }

    // Restore scrollback before reading cursor position so the cursor
    // reflects the live terminal, not the scrolled-back view.
    if copy_mode.is_some() {
        parser.screen_mut().set_scrollback(original_scrollback);
    }

    // Show cursor when focused (but not during copy mode — the cursor
    // position is meaningless when the user is browsing scrollback).
    // We always ignore hide_cursor for the focused pane: Claude Code and other
    // interactive programs frequently toggle hide/show around redraws, and a
    // missed show_cursor escape (e.g. due to PTY read chunking) would leave
    // the cursor permanently invisible.
    let screen = parser.screen();
    let show_cursor = is_focused && copy_mode.is_none();
    if show_cursor {
        let cursor = screen.cursor_position();
        let cursor_x = area.x + cursor.1;
        let cursor_y = area.y + cursor.0;
        if cursor_x < area.x + area.width && cursor_y < area.y + area.height {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }

    drop(parser);

    // Scrollbar on the right edge
    let (scroll_offset, total_lines) = pane.scrollbar_info();
    if total_lines > rows {
        let scrollbar_x = area.x + area.width - 1;
        let max_scroll = total_lines.saturating_sub(rows);
        let visible_ratio = rows as f32 / total_lines as f32;
        let thumb_height = (area.height as f32 * visible_ratio).max(1.0) as u16;

        // Position: 0 = bottom, max_scroll = top
        let scroll_ratio = if max_scroll > 0 {
            1.0 - (scroll_offset as f32 / max_scroll as f32)
        } else {
            1.0
        };
        let thumb_top = ((area.height - thumb_height) as f32 * scroll_ratio) as u16;

        let buf = frame.buffer_mut();
        for row in 0..area.height {
            let y = area.y + row;
            let is_thumb = row >= thumb_top && row < thumb_top + thumb_height;
            let (sym, style) = if is_thumb {
                (
                    "\u{2588}",
                    Style::default().fg(Color::Rgb(0x58, 0x5e, 0x68)),
                ) // █ thumb
            } else {
                (
                    "\u{2502}",
                    Style::default().fg(Color::Rgb(0x2d, 0x33, 0x3b)),
                ) // │ track
            };
            if let Some(cell) = buf.cell_mut((scrollbar_x, y)) {
                cell.set_symbol(sym);
                cell.set_style(style);
            }
        }
    }
}
