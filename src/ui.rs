use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{
    App, CloseConfirmDialog, CloseConfirmFocus, CopyModeState, DragTarget, FocusTarget, LaunchMode,
    PaneCreateDialog, PaneCreateField, PaneStatus, SidebarMode, WorktreeCleanupDialog, FEATURES,
    SETTINGS_ITEMS,
};
use crate::keybinding;

// ─── Theme (Claude-inspired) ──────────────────────────────
const BG: Color = Color::Rgb(0x0d, 0x11, 0x17);
const PANEL_BG: Color = Color::Rgb(0x13, 0x17, 0x1f);
const BORDER: Color = Color::Rgb(0x2d, 0x33, 0x3b);
const FOCUS_BORDER: Color = Color::Rgb(0x58, 0xa6, 0xff);
const TEXT: Color = Color::Rgb(0xe6, 0xed, 0xf3);
const TEXT_DIM: Color = Color::Rgb(0x6e, 0x76, 0x81);
const ACCENT_GREEN: Color = Color::Rgb(0x3f, 0xb9, 0x50);
const ACCENT_BLUE: Color = Color::Rgb(0x58, 0xa6, 0xff);
const ACCENT_CLAUDE: Color = Color::Rgb(0xd9, 0x77, 0x57);
const HEADER_BG: Color = Color::Rgb(0x16, 0x1b, 0x22);
const ACTIVE_TAB_BG: Color = Color::Rgb(0x0d, 0x11, 0x17);
const ACTIVE_BG: Color = Color::Rgb(0x1c, 0x23, 0x33);
const LINE_NUM_COLOR: Color = Color::Rgb(0x3d, 0x44, 0x4d);
const SCROLL_BG: Color = Color::Rgb(0x2a, 0x1f, 0x14);

const MIN_TERMINAL_WIDTH: u16 = 40;
const MIN_TERMINAL_HEIGHT: u16 = 10;
const MIN_PANE_AREA_WIDTH: u16 = 20;

// ─── File type icons ──────────────────────────────────────
fn file_icon(name: &str) -> (&'static str, Color) {
    let ext = name.rsplit('.').next().unwrap_or("");
    match ext {
        "rs" => ("\u{1f980} ", Color::Rgb(0xde, 0x93, 0x5f)), // 🦀 orange
        "toml" => ("\u{2699}\u{fe0f} ", Color::Rgb(0x9e, 0x9e, 0x9e)), // ⚙️ gray
        "lock" => ("\u{1f512} ", Color::Rgb(0x9e, 0x9e, 0x9e)), // 🔒
        "md" => ("\u{1f4c4} ", Color::Rgb(0x58, 0xa6, 0xff)), // 📄 blue
        "json" => ("{ ", Color::Rgb(0xf1, 0xe0, 0x5a)),       // { yellow
        "yaml" | "yml" => ("~ ", Color::Rgb(0xf1, 0xe0, 0x5a)), // ~ yellow
        "js" => ("\u{26a1} ", Color::Rgb(0xf1, 0xe0, 0x5a)),  // ⚡ yellow
        "ts" | "tsx" => ("\u{26a1} ", Color::Rgb(0x31, 0x78, 0xc6)), // ⚡ blue
        "jsx" => ("\u{26a1} ", Color::Rgb(0x61, 0xda, 0xfb)), // ⚡ cyan
        "py" => ("\u{1f40d} ", Color::Rgb(0x35, 0x72, 0xa5)), // 🐍 blue
        "sh" | "bash" | "zsh" => ("$ ", Color::Rgb(0x3f, 0xb9, 0x50)), // $ green
        "css" | "scss" => ("# ", Color::Rgb(0x56, 0x3d, 0x7c)), // # purple
        "html" => ("< ", Color::Rgb(0xe3, 0x4c, 0x26)),       // < orange
        "gitignore" => ("\u{2022} ", Color::Rgb(0xf0, 0x50, 0x33)), // • git red
        _ => ("\u{2022} ", TEXT_DIM),                         // • default
    }
}

fn git_status_icon(state: Option<crate::git_status::GitFileState>) -> (&'static str, Color) {
    match state {
        Some(crate::git_status::GitFileState::Modified) => ("M ", Color::Yellow),
        Some(crate::git_status::GitFileState::Added) => ("+ ", ACCENT_GREEN),
        Some(crate::git_status::GitFileState::Deleted) => ("- ", Color::Rgb(0xf8, 0x70, 0x70)),
        Some(crate::git_status::GitFileState::Renamed) => ("→ ", ACCENT_BLUE),
        Some(crate::git_status::GitFileState::Untracked) => ("? ", Color::Magenta),
        Some(crate::git_status::GitFileState::Ignored) => ("◌ ", TEXT_DIM),
        Some(crate::git_status::GitFileState::Conflicted) => ("! ", Color::Red),
        None => ("  ", PANEL_BG),
    }
}

// ─── Main render ──────────────────────────────────────────

pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    app.last_term_size = (area.width, area.height);

    if area.width < MIN_TERMINAL_WIDTH || area.height < MIN_TERMINAL_HEIGHT {
        let msg = Paragraph::new("Terminal too small")
            .style(Style::default().fg(TEXT_DIM).bg(BG))
            .alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return;
    }

    let bg_block = Block::default().style(Style::default().bg(BG));
    frame.render_widget(bg_block, area);

    let show_status =
        app.status_bar_visible || app.rename_input.is_some() || app.pane_rename_input.is_some();
    let status_h = if show_status { 1 } else { 0 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),        // tab bar
            Constraint::Min(1),           // main area
            Constraint::Length(status_h), // status bar
        ])
        .split(area);

    render_tab_bar(app, frame, chunks[0]);
    render_main_area(app, frame, chunks[1]);
    if show_status {
        render_status_bar(app, frame, chunks[2]);
    }

    if app.layout_picker.visible {
        render_layout_picker(app, frame, area);
    }

    if app.feature_toggle.visible {
        render_feature_toggle(app, frame, area);
    }

    if app.settings_panel.visible {
        render_settings_panel(app, frame, area);
    }

    if let Some((ref msg, _)) = app.status_flash {
        render_status_flash(msg, frame, area);
    }

    if app.pane_create_dialog.visible {
        render_pane_create_dialog(frame, area, &app.pane_create_dialog);
    }
    if app.close_confirm_dialog.visible {
        render_close_confirm_dialog(frame, area, &app.close_confirm_dialog);
    }
    if let Some(ref d) = app.worktree_cleanup_dialog {
        if d.visible {
            render_worktree_cleanup_dialog(frame, area, d);
        }
    }

    if app.pane_list_overlay.visible {
        render_pane_list_overlay(app, frame, area);
    }

    if app.filetree_action_popup.visible {
        render_filetree_action_popup(app, frame, area);
    }
}

// ─── Tab bar ──────────────────────────────────────────────

fn render_tab_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    let mut spans = Vec::new();
    let mut tab_rects = Vec::new();
    let mut x = area.x;

    // Logo
    spans.push(Span::styled(
        " \u{25c8} ",
        Style::default()
            .fg(ACCENT_CLAUDE)
            .bg(HEADER_BG)
            .add_modifier(Modifier::BOLD),
    ));
    x += 3;

    for (i, ws) in app.workspaces.iter().enumerate() {
        let is_active = i == app.active_tab;
        let renaming = is_active && app.rename_input.is_some();

        let label = if renaming {
            let buf = app.rename_input.as_deref().unwrap_or("");
            // Block cursor at end; placeholder when empty keeps the tab visible.
            format!(" {}\u{2588} ", buf)
        } else {
            format!(" {} ", ws.display_name())
        };
        let label_width = unicode_width::UnicodeWidthStr::width(label.as_str()) as u16;

        if renaming {
            spans.push(Span::styled(
                label.clone(),
                Style::default()
                    .fg(TEXT)
                    .bg(ACTIVE_TAB_BG)
                    .add_modifier(Modifier::BOLD),
            ));
        } else if is_active {
            // Active tab: underline bar ▔ effect via bold + brighter bg
            spans.push(Span::styled(
                label.clone(),
                Style::default()
                    .fg(TEXT)
                    .bg(ACTIVE_TAB_BG)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ));
        } else {
            spans.push(Span::styled(
                label.clone(),
                Style::default().fg(TEXT_DIM).bg(HEADER_BG),
            ));
        }

        tab_rects.push((i, Rect::new(x, area.y, label_width, 1)));
        x += label_width;

        spans.push(Span::styled(" ", Style::default().bg(HEADER_BG)));
        x += 1;
    }

    // [+] button
    let plus_label = " + ";
    spans.push(Span::styled(
        plus_label,
        Style::default().fg(ACCENT_GREEN).bg(HEADER_BG),
    ));
    let plus_rect = Rect::new(x, area.y, plus_label.len() as u16, 1);
    x += plus_label.len() as u16;

    // Fill remaining
    let remaining = area.width.saturating_sub(x - area.x);
    if remaining > 0 {
        spans.push(Span::styled(
            " ".repeat(remaining as usize),
            Style::default().bg(HEADER_BG),
        ));
    }

    app.last_tab_rects = tab_rects;
    app.last_new_tab_rect = Some(plus_rect);

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ─── Main area ────────────────────────────────────────────

fn render_main_area(app: &mut App, frame: &mut Frame, area: Rect) {
    // Preview zoom: expand the preview to the full content area.
    if app.preview_zoomed && app.ws().preview.is_active() {
        app.ws_mut().last_file_tree_rect = None;
        app.ws_mut().last_preview_rect = Some(area);
        // Clear stale pane rects so mouse drag hit-testing doesn't fire on ghost areas.
        app.ws_mut().last_pane_rects = Vec::new();
        render_preview(app, frame, area);
        return;
    }

    let tree_width = app.file_tree_width;
    let preview_width = app.preview_width;

    let mut has_tree = app.ws().sidebar_mode != SidebarMode::None;
    let mut has_preview = app.ws().preview.is_active();

    let needed = MIN_PANE_AREA_WIDTH
        + if has_tree { tree_width } else { 0 }
        + if has_preview { preview_width } else { 0 };
    if area.width < needed && has_preview {
        has_preview = false;
    }
    let needed = MIN_PANE_AREA_WIDTH + if has_tree { tree_width } else { 0 };
    if area.width < needed && has_tree {
        has_tree = false;
    }

    let swapped = app.layout_swapped;

    let mut constraints = Vec::new();
    if has_tree {
        constraints.push(Constraint::Length(tree_width));
    }
    if swapped && has_preview {
        constraints.push(Constraint::Length(preview_width));
    }
    constraints.push(Constraint::Min(20));
    if !swapped && has_preview {
        constraints.push(Constraint::Length(preview_width));
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    let mut idx = 0;

    if has_tree {
        let sidebar_rect = chunks[idx];
        app.ws_mut().last_file_tree_rect = Some(sidebar_rect);
        match app.ws().sidebar_mode {
            SidebarMode::FileTree => render_file_tree(app, frame, sidebar_rect),
            SidebarMode::PaneList => render_pane_list_sidebar(app, frame, sidebar_rect),
            SidebarMode::None => {}
        }
        idx += 1;
    } else {
        app.ws_mut().last_file_tree_rect = None;
    }

    if swapped && has_preview {
        app.ws_mut().last_preview_rect = Some(chunks[idx]);
        render_preview(app, frame, chunks[idx]);
        idx += 1;
    }

    render_panes(app, frame, chunks[idx]);
    idx += 1;

    if !swapped && has_preview {
        app.ws_mut().last_preview_rect = Some(chunks[idx]);
        render_preview(app, frame, chunks[idx]);
    }

    if !has_preview {
        app.ws_mut().last_preview_rect = None;
    }
}

// ─── File tree ────────────────────────────────────────────

fn render_file_tree(app: &mut App, frame: &mut Frame, area: Rect) {
    let is_focused = app.ws().focus_target == FocusTarget::FileTree;
    let is_border_active = matches!(
        app.dragging.as_ref().or(app.hover_border.as_ref()),
        Some(DragTarget::FileTreeBorder)
    );
    let border_color = if is_border_active {
        ACCENT_GREEN
    } else if is_focused {
        FOCUS_BORDER
    } else {
        BORDER
    };

    let title_style = if is_focused {
        Style::default()
            .fg(ACCENT_BLUE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_DIM)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(" FILES ", title_style))
        .style(Style::default().bg(PANEL_BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let visible_height = inner.height as usize;
    app.ws_mut().file_tree.ensure_visible(visible_height);

    let entries = app.ws().file_tree.visible_entries();
    let scroll = app.ws().file_tree.scroll_offset;
    let selected = app.ws().file_tree.selected_index;
    let max_width = inner.width as usize;
    let git_status = app.ws().git_status.as_ref();

    for (i, entry) in entries.iter().skip(scroll).take(visible_height).enumerate() {
        let y = inner.y + i as u16;
        let entry_index = scroll + i;
        let is_selected = entry_index == selected;

        // Selection indicator bar on the left
        let indicator = if is_selected { "\u{258e}" } else { " " }; // ▎ or space
        let indicator_style = if is_selected {
            Style::default().fg(ACCENT_BLUE).bg(ACTIVE_BG)
        } else {
            Style::default().fg(PANEL_BG).bg(PANEL_BG)
        };

        // Tree indent with connector lines
        let indent = if entry.depth > 0 {
            let mut s = String::new();
            for _ in 0..entry.depth.saturating_sub(1) {
                s.push_str("\u{2502} "); // │
            }
            s.push_str("\u{251c}\u{2500}"); // ├─
            s
        } else {
            String::new()
        };

        // Icon + name
        let (icon, name_display, name_color) = if entry.is_dir {
            let icon = if entry.is_expanded {
                "\u{1f4c2} "
            } else {
                "\u{1f4c1} "
            }; // 📂 / 📁
            (icon, &entry.name, ACCENT_BLUE)
        } else {
            let (icon, color) = file_icon(&entry.name);
            (icon, &entry.name, color)
        };

        let content = format!("{}{}{}", indent, icon, name_display);
        let truncated = truncate_to_width(&content, max_width.saturating_sub(3));
        let git_state = git_status.and_then(|snapshot| snapshot.state_for(&entry.path));
        let (git_icon, git_color) = git_status_icon(git_state);

        // Build styled spans
        let mut spans = vec![Span::styled(indicator, indicator_style)];

        let git_style = if is_selected {
            Style::default()
                .fg(git_color)
                .bg(ACTIVE_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(git_color).bg(PANEL_BG)
        };
        spans.push(Span::styled(git_icon, git_style));

        let content_style = if is_selected {
            Style::default()
                .fg(TEXT)
                .bg(ACTIVE_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(name_color).bg(PANEL_BG)
        };

        spans.push(Span::styled(truncated, content_style));

        // Fill remaining width
        let line_widget = Paragraph::new(Line::from(spans));
        frame.render_widget(line_widget, Rect::new(inner.x, y, inner.width, 1));
    }
}

// ─── Panes ────────────────────────────────────────────────

fn render_panes(app: &mut App, frame: &mut Frame, area: Rect) {
    let rects = app.ws().layout.calculate_rects(area);
    app.ws_mut().last_pane_rects = rects.clone();

    for &(pane_id, rect) in &rects {
        if let Some(pane) = app.ws_mut().panes.get_mut(&pane_id) {
            let inner_rows = rect.height.saturating_sub(2);
            let inner_cols = rect.width.saturating_sub(2);
            let _ = pane.resize(inner_rows, inner_cols); // now returns Result<bool>
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
                .map(|p| (pane_id, p.pane_cwd()))
        })
        .collect();
    let mut cwd_counts = std::collections::HashMap::new();
    for (_, cwd) in &pane_cwds {
        *cwd_counts.entry(cwd.clone()).or_insert(0usize) += 1;
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
            let is_focused = pane_id == focused_id && focus_target == FocusTarget::Pane;
            let pane_sel = selection.as_ref().filter(
                |s| matches!(s.target, crate::app::SelectionTarget::Pane(id) if id == pane_id),
            );
            let claude_state = app.claude_monitor.state(pane_id);
            let pane_status = app.pane_status(pane_id);
            let dismissed = app.pane_state_dismissed(pane_id);
            let ai_title = app.pane_display_title(pane_id).map(|s| s.to_string());
            let rename_buf = app.pane_rename_input.as_ref().and_then(|(id, buf)| {
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

fn str_to_border_type(s: &str) -> BorderType {
    match s {
        "plain" => BorderType::Plain,
        "double" => BorderType::Double,
        "thick" => BorderType::Thick,
        "none" => BorderType::Plain, // ratatui has no "none" variant; borders hidden via Borders::NONE
        _ => BorderType::Rounded,
    }
}

fn parse_color_str(s: &str) -> Option<Color> {
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
struct StatusColors {
    running: Color,
    done: Color,
    waiting: Color,
    bg_done: Color,
    bg_waiting: Color,
}

impl StatusColors {
    fn from_config(cfg: &crate::config::StatusConfig) -> Self {
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

#[allow(clippy::too_many_arguments)]
fn render_single_pane(
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
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
    } else {
        render_terminal_content(pane, is_focused, selection, copy_mode, frame, inner);
    }
}

fn render_terminal_content(
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
                let has_selection = selection.is_some_and(|s| {
                    let (sr, sc, er, ec) = s.normalized();
                    (sr != er || sc != ec) && s.contains(row as u32, col as u32)
                });

                let copy_cursor = copy_mode
                    .is_some_and(|cm| cm.cursor_row == row as u16 && cm.cursor_col == col as u16);
                let copy_selected = copy_mode.is_some_and(|cm| {
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

// ─── Preview ──────────────────────────────────────────────

fn render_preview(app: &mut App, frame: &mut Frame, area: Rect) {
    // Extract values we need before any mutable borrow.
    let is_focused = app.ws().focus_target == FocusTarget::Preview;
    let filename = app.ws().preview.filename();
    let diff_mode = app.ws().preview.diff_mode;
    let title = if diff_mode {
        format!(" {} [diff] ", filename)
    } else {
        format!(" {} ", filename)
    };
    let is_image = app.ws().preview.is_image();
    let is_binary = app.ws().preview.is_binary;
    let line_count = if diff_mode {
        app.ws().preview.diff_lines.len()
    } else {
        app.ws().preview.lines.len()
    };
    let scroll_pos = app.ws().preview.scroll_offset;

    let is_border_active = matches!(
        app.dragging.as_ref().or(app.hover_border.as_ref()),
        Some(DragTarget::PreviewBorder)
    );
    let border_color = if is_border_active {
        ACCENT_GREEN
    } else if is_focused {
        ACCENT_CLAUDE
    } else {
        BORDER
    };

    // Line count in bottom-right
    let line_info = if is_image {
        Span::styled(" image ", Style::default().fg(TEXT_DIM))
    } else if !is_binary {
        Span::styled(
            format!(" {}/{} ", scroll_pos + 1, line_count),
            Style::default().fg(TEXT_DIM),
        )
    } else {
        Span::default()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            title,
            Style::default()
                .fg(ACCENT_CLAUDE)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(line_info)
        .style(Style::default().bg(PANEL_BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Image preview
    if is_image {
        let is_dragging = app.dragging.is_some();
        if is_dragging {
            // Skip expensive Sixel re-encode during drag; show placeholder.
            let placeholder = Paragraph::new("Resizing...")
                .alignment(ratatui::layout::Alignment::Center)
                .style(Style::default().fg(TEXT_DIM).bg(PANEL_BG));
            frame.render_widget(placeholder, inner);
        } else if let Some(ref mut protocol) = app.ws_mut().preview.image_protocol {
            let image_widget = ratatui_image::StatefulImage::default().resize(
                ratatui_image::Resize::Fit(Some(ratatui_image::FilterType::CatmullRom)),
            );
            frame.render_stateful_widget(image_widget, inner, protocol);
        }
        return;
    }

    if is_binary {
        let msg = Paragraph::new("\u{2718} Binary file")
            .style(Style::default().fg(TEXT_DIM).bg(PANEL_BG));
        frame.render_widget(msg, inner);
        return;
    }

    let ws = app.ws();
    let visible_height = inner.height as usize;
    let scroll = ws.preview.scroll_offset;
    let h_scroll = ws.preview.h_scroll_offset;
    let has_highlights = !ws.preview.highlighted_lines.is_empty();

    // Diff mode: render colored unified-diff lines instead of syntax
    // highlight. Selection overlay below still applies because lines
    // are addressable by index just like the normal preview.
    if ws.preview.diff_mode && !ws.preview.diff_lines.is_empty() {
        for i in 0..visible_height {
            let line_idx = scroll + i;
            if line_idx >= ws.preview.diff_lines.len() {
                break;
            }
            let y = inner.y + i as u16;
            let dl = &ws.preview.diff_lines[line_idx];
            let max_content = inner.width as usize;
            let spans = if dl.styled_spans.is_empty() {
                let dropped: String = dl.text.chars().skip(h_scroll).collect();
                let content = truncate_to_width(&dropped, max_content);
                let style = match dl.kind {
                    crate::preview::DiffLineKind::Added => Style::default().fg(ACCENT_GREEN),
                    crate::preview::DiffLineKind::Removed => {
                        Style::default().fg(Color::Rgb(0xf8, 0x70, 0x70))
                    }
                    crate::preview::DiffLineKind::Hunk => Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                    crate::preview::DiffLineKind::Header => {
                        Style::default().fg(TEXT_DIM).add_modifier(Modifier::BOLD)
                    }
                    crate::preview::DiffLineKind::Context => Style::default().fg(TEXT),
                };
                vec![Span::styled(content, style)]
            } else {
                diff_spans_with_scroll(&dl.styled_spans, h_scroll, max_content)
            };
            let paragraph = Paragraph::new(Line::from(spans)).style(Style::default().bg(PANEL_BG));
            frame.render_widget(paragraph, Rect::new(inner.x, y, inner.width, 1));
        }
    } else {
        for i in 0..visible_height {
            let line_idx = scroll + i;
            if line_idx >= ws.preview.lines.len() {
                break;
            }

            let y = inner.y + i as u16;
            let line_num = line_idx + 1;
            let num_str = format!("{:>4}\u{2502}", line_num);
            let max_content = (inner.width as usize).saturating_sub(5);

            let mut spans = vec![Span::styled(num_str, Style::default().fg(LINE_NUM_COLOR))];

            if has_highlights && line_idx < ws.preview.highlighted_lines.len() {
                // Drop `h_scroll` chars from the start of the line, walking
                // spans so syntax highlighting is preserved.
                let mut chars_skipped = 0usize;
                let mut used_width = 0usize;
                for styled_span in &ws.preview.highlighted_lines[line_idx] {
                    if used_width >= max_content {
                        break;
                    }

                    let span_chars = styled_span.text.chars().count();
                    let visible_text: std::borrow::Cow<'_, str> =
                        if chars_skipped + span_chars <= h_scroll {
                            // Entire span is off-screen to the left.
                            chars_skipped += span_chars;
                            continue;
                        } else if chars_skipped >= h_scroll {
                            std::borrow::Cow::Borrowed(styled_span.text.as_str())
                        } else {
                            // Partially skip into this span.
                            let skip_in_span = h_scroll - chars_skipped;
                            chars_skipped = h_scroll;
                            let remainder: String =
                                styled_span.text.chars().skip(skip_in_span).collect();
                            std::borrow::Cow::Owned(remainder)
                        };

                    if visible_text.is_empty() {
                        continue;
                    }
                    let remaining = max_content - used_width;
                    let text = truncate_to_width(&visible_text, remaining);
                    used_width += unicode_width::UnicodeWidthStr::width(text.as_str());
                    let (r, g, b) = styled_span.fg;
                    spans.push(Span::styled(text, Style::default().fg(Color::Rgb(r, g, b))));
                }
            } else {
                let plain = &ws.preview.lines[line_idx];
                let dropped: String = plain.chars().skip(h_scroll).collect();
                let content = truncate_to_width(&dropped, max_content);
                spans.push(Span::styled(content, Style::default().fg(TEXT)));
            }

            let paragraph = Paragraph::new(Line::from(spans)).style(Style::default().bg(PANEL_BG));
            frame.render_widget(paragraph, Rect::new(inner.x, y, inner.width, 1));
        }
    }

    // Selection highlight overlay. The selection is stored in SOURCE
    // coordinates (absolute line index + char offset into the line),
    // so we subtract the current scroll + h_scroll to produce screen
    // positions. Cells outside the visible window are skipped. The
    // highlighted band is also clamped to the actual line length so
    // it never paints past the text that would actually be copied.
    if let Some(sel) = app.selection.as_ref() {
        if matches!(sel.target, crate::app::SelectionTarget::Preview) {
            let (sr, sc, er, ec) = sel.normalized();
            if sr != er || sc != ec {
                let content = sel.content_rect;
                let scroll_v = ws.preview.scroll_offset as i64;
                let h_scroll = ws.preview.h_scroll_offset as i64;
                let buf = frame.buffer_mut();

                for abs_row in sr..=er {
                    let screen_row_i = abs_row as i64 - scroll_v;
                    if screen_row_i < 0 {
                        continue;
                    }
                    if screen_row_i >= content.height as i64 {
                        break;
                    }
                    let y = content.y + screen_row_i as u16;

                    // Line's actual character count (sets the right
                    // clamp for the highlight band).
                    let line_chars = if ws.preview.diff_mode && !ws.preview.diff_lines.is_empty() {
                        ws.preview
                            .diff_lines
                            .get(abs_row as usize)
                            .map(|line| line.text.chars().count())
                            .unwrap_or(0)
                    } else {
                        ws.preview
                            .lines
                            .get(abs_row as usize)
                            .map(|s| s.chars().count())
                            .unwrap_or(0)
                    };
                    if line_chars == 0 {
                        continue;
                    }

                    let src_col_start = if abs_row == sr { sc as usize } else { 0 };
                    let src_col_end_inclusive = if abs_row == er {
                        ec as usize
                    } else {
                        line_chars.saturating_sub(1)
                    };
                    let src_col_end_clamped =
                        src_col_end_inclusive.min(line_chars.saturating_sub(1));
                    if src_col_start > src_col_end_clamped {
                        continue;
                    }

                    for src_col in src_col_start..=src_col_end_clamped {
                        let screen_col_i = src_col as i64 - h_scroll;
                        if screen_col_i < 0 {
                            continue;
                        }
                        if screen_col_i >= content.width as i64 {
                            break;
                        }
                        let x = content.x + screen_col_i as u16;
                        if let Some(cell) = buf.cell_mut((x, y)) {
                            cell.set_style(
                                Style::default()
                                    .fg(Color::Rgb(0x0d, 0x11, 0x17))
                                    .bg(Color::Rgb(0x58, 0xa6, 0xff)),
                            );
                        }
                    }
                }
            }
        }
    }
}

// ─── Status bar (context-aware) ───────────────────────────

fn render_status_bar(app: &App, frame: &mut Frame, area: Rect) {
    if !app.config.features.status_bar
        && app.rename_input.is_none()
        && app.pane_rename_input.is_none()
    {
        let empty = Paragraph::new("").style(Style::default().bg(HEADER_BG));
        frame.render_widget(empty, area);
        return;
    }

    let focus = app.ws().focus_target;

    let hints = if app.rename_input.is_some() || app.pane_rename_input.is_some() {
        Line::from(vec![
            Span::styled(" Enter", Style::default().fg(ACCENT_BLUE)),
            Span::styled(" Confirm  ", Style::default().fg(TEXT_DIM)),
            Span::styled("Esc", Style::default().fg(ACCENT_BLUE)),
            Span::styled(" Cancel  ", Style::default().fg(TEXT_DIM)),
            Span::styled("Empty Enter", Style::default().fg(ACCENT_BLUE)),
            Span::styled(" Reset", Style::default().fg(TEXT_DIM)),
        ])
    } else {
        let kb = &app.config.keybindings;
        match focus {
            FocusTarget::Preview => Line::from(vec![
                Span::styled(" Scroll  ", Style::default().fg(TEXT_DIM)),
                Span::styled(
                    format!(" {}", keybinding::keybinding_display(&kb.pane_close)),
                    Style::default().fg(ACCENT_BLUE),
                ),
                Span::styled(" Close  ", Style::default().fg(TEXT_DIM)),
                Span::styled(
                    keybinding::keybinding_display(&kb.preview_swap),
                    Style::default().fg(ACCENT_BLUE),
                ),
                Span::styled(" Swap  ", Style::default().fg(TEXT_DIM)),
                Span::styled(
                    keybinding::keybinding_display(&kb.quit),
                    Style::default().fg(ACCENT_BLUE),
                ),
                Span::styled(" Quit", Style::default().fg(TEXT_DIM)),
            ]),
            FocusTarget::FileTree => Line::from(vec![
                Span::styled(" j/k", Style::default().fg(ACCENT_BLUE)),
                Span::styled(" Move  ", Style::default().fg(TEXT_DIM)),
                Span::styled("Enter", Style::default().fg(ACCENT_BLUE)),
                Span::styled(" Open  ", Style::default().fg(TEXT_DIM)),
                Span::styled("d", Style::default().fg(ACCENT_BLUE)),
                Span::styled(" Diff  ", Style::default().fg(TEXT_DIM)),
                Span::styled(".", Style::default().fg(ACCENT_BLUE)),
                Span::styled(" Hidden  ", Style::default().fg(TEXT_DIM)),
                Span::styled("Esc", Style::default().fg(ACCENT_BLUE)),
                Span::styled(" Back  ", Style::default().fg(TEXT_DIM)),
                Span::styled(
                    keybinding::keybinding_display(&kb.file_tree),
                    Style::default().fg(ACCENT_BLUE),
                ),
                Span::styled(" Close  ", Style::default().fg(TEXT_DIM)),
                Span::styled(
                    keybinding::keybinding_display(&kb.quit),
                    Style::default().fg(ACCENT_BLUE),
                ),
                Span::styled(" Quit", Style::default().fg(TEXT_DIM)),
            ]),
            FocusTarget::Pane => Line::from(vec![
                Span::styled(
                    format!(" {}", keybinding::keybinding_display(&kb.split_vertical)),
                    Style::default().fg(ACCENT_BLUE),
                ),
                Span::styled(" VSplit  ", Style::default().fg(TEXT_DIM)),
                Span::styled(
                    keybinding::keybinding_display(&kb.split_horizontal),
                    Style::default().fg(ACCENT_BLUE),
                ),
                Span::styled(" HSplit  ", Style::default().fg(TEXT_DIM)),
                Span::styled(
                    keybinding::keybinding_display(&kb.pane_close),
                    Style::default().fg(ACCENT_BLUE),
                ),
                Span::styled(" Close  ", Style::default().fg(TEXT_DIM)),
                Span::styled(
                    keybinding::keybinding_display(&kb.tab_new),
                    Style::default().fg(ACCENT_BLUE),
                ),
                Span::styled(" New Tab  ", Style::default().fg(TEXT_DIM)),
                Span::styled(
                    keybinding::keybinding_display(&kb.tab_rename),
                    Style::default().fg(ACCENT_BLUE),
                ),
                Span::styled(" Rename  ", Style::default().fg(TEXT_DIM)),
                Span::styled(
                    keybinding::keybinding_display(&kb.file_tree),
                    Style::default().fg(ACCENT_BLUE),
                ),
                Span::styled(" Tree  ", Style::default().fg(TEXT_DIM)),
                Span::styled(
                    keybinding::keybinding_display(&kb.preview_swap),
                    Style::default().fg(ACCENT_BLUE),
                ),
                Span::styled(" Swap  ", Style::default().fg(TEXT_DIM)),
                Span::styled(
                    keybinding::keybinding_display(&kb.quit),
                    Style::default().fg(ACCENT_BLUE),
                ),
                Span::styled(" Quit", Style::default().fg(TEXT_DIM)),
            ]),
            FocusTarget::PaneList => Line::from(vec![Span::styled(
                " j/k: Move  Enter: Select  Esc: Close",
                Style::default().fg(TEXT_DIM),
            )]),
        }
    };

    let status = Paragraph::new(hints).style(Style::default().bg(HEADER_BG));
    frame.render_widget(status, area);

    // Global pane status counts
    let mut running_count = 0usize;
    let mut done_count = 0usize;
    let mut waiting_count = 0usize;
    for state in app.pane_states.values() {
        match state.status {
            PaneStatus::Running => running_count += 1,
            PaneStatus::Done => done_count += 1,
            PaneStatus::Waiting => waiting_count += 1,
            PaneStatus::Idle => {}
        }
    }

    // Right-side info: Claude state of focused pane
    let focused_id = app.ws().focused_pane_id;
    let claude_state = app.claude_monitor.state(focused_id);
    let has_claude = app
        .ws()
        .panes
        .get(&focused_id)
        .is_some_and(|p| p.is_claude_running());

    let mut right_spans = Vec::new();

    if has_claude {
        // Model
        if let Some(model) = claude_state.short_model() {
            right_spans.push(Span::styled(
                format!(" \u{1f9e0} {} ", model),
                Style::default().fg(ACCENT_CLAUDE),
            ));
        }

        // Context usage
        if claude_state.context_tokens > 0 {
            let ratio = claude_state.context_usage();
            let bar = make_progress_bar((ratio * 10.0) as usize, 10, 6);
            let color = if ratio > 0.9 {
                Color::Rgb(0xf8, 0x51, 0x49) // red
            } else if ratio > 0.7 {
                Color::Rgb(0xd2, 0x99, 0x22) // yellow
            } else {
                ACCENT_GREEN
            };
            right_spans.push(Span::styled(
                format!(
                    " {} {}/{} ",
                    bar,
                    format_tokens(claude_state.context_tokens),
                    format_tokens(claude_state.context_limit())
                ),
                Style::default().fg(color),
            ));
        }
    }

    // Git branch (even without claude)
    if let Some(ref branch) = claude_state.git_branch {
        let short = truncate_to_width(branch, 20);
        right_spans.push(Span::styled(
            format!(" \u{2387} {} ", short),
            Style::default().fg(ACCENT_BLUE),
        ));
    }

    // Pane status counts
    if running_count > 0 || done_count > 0 || waiting_count > 0 {
        right_spans.push(Span::styled(
            format!(
                " \u{23f5}:{} \u{2713}:{} \u{23f8}:{} ",
                running_count, done_count, waiting_count
            ),
            Style::default().fg(TEXT_DIM),
        ));
    }

    // Worktrees count
    let wt_count = app.ws().worktrees.len();
    if wt_count > 0 {
        right_spans.push(Span::styled(
            format!(" worktrees:{} ", wt_count),
            Style::default().fg(TEXT_DIM),
        ));
    }

    // Prefix mode indicator
    if app.prefix_active {
        right_spans.push(Span::styled(
            " [PREFIX] ",
            Style::default()
                .fg(Color::Rgb(0xff, 0xd7, 0x00))
                .add_modifier(Modifier::BOLD),
        ));
    }

    // AI title indicator
    let ai_label = if app.ai_title_enabled {
        "AI:on"
    } else {
        "AI:off"
    };
    right_spans.push(Span::styled(
        format!(" {} ", ai_label),
        Style::default().fg(if app.ai_title_enabled {
            ACCENT_GREEN
        } else {
            TEXT_DIM
        }),
    ));

    // Update notice (highest priority — overrides above if present)
    if let Some(new_version) = app.version_info.update_available() {
        right_spans.push(Span::styled(
            format!(" \u{2191} v{} ", new_version),
            Style::default()
                .fg(ACCENT_CLAUDE)
                .add_modifier(Modifier::BOLD),
        ));
    }

    if !right_spans.is_empty() {
        let total_width: u16 = right_spans
            .iter()
            .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()) as u16)
            .sum();
        if area.width > total_width {
            let right_rect = Rect::new(area.x + area.width - total_width, area.y, total_width, 1);
            let widget =
                Paragraph::new(Line::from(right_spans)).style(Style::default().bg(HEADER_BG));
            frame.render_widget(widget, right_rect);
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────

/// Build a progress bar string like `▓▓▓▓░░░░░░`.
fn make_progress_bar(current: usize, total: usize, width: usize) -> String {
    if total == 0 {
        return String::new();
    }
    let filled = ((current as f32 / total as f32) * width as f32).round() as usize;
    let filled = filled.min(width);
    let mut s = String::with_capacity(width * 3);
    for _ in 0..filled {
        s.push('\u{2593}'); // ▓
    }
    for _ in filled..width {
        s.push('\u{2591}'); // ░
    }
    s
}

/// Format token count: 1234 → "1.2k", 1234567 → "1.2M"
fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn truncate_to_width(s: &str, max_width: usize) -> String {
    let mut result = String::new();
    let mut width = 0;
    for ch in s.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > max_width {
            break;
        }
        result.push(ch);
        width += ch_width;
    }
    result
}

fn diff_spans_with_scroll(
    spans: &[crate::preview::DiffStyledSpan],
    h_scroll: usize,
    max_width: usize,
) -> Vec<Span<'static>> {
    let mut visible = Vec::new();
    let mut skipped = 0usize;
    let mut used_width = 0usize;

    for span in spans {
        if used_width >= max_width {
            break;
        }

        let span_chars = span.text.chars().count();
        if skipped + span_chars <= h_scroll {
            skipped += span_chars;
            continue;
        }

        let text = if skipped >= h_scroll {
            span.text.clone()
        } else {
            let skip_in_span = h_scroll - skipped;
            skipped = h_scroll;
            span.text.chars().skip(skip_in_span).collect()
        };

        if text.is_empty() {
            continue;
        }

        let remaining = max_width.saturating_sub(used_width);
        let truncated = truncate_to_width(&text, remaining);
        if truncated.is_empty() {
            continue;
        }

        used_width += unicode_width::UnicodeWidthStr::width(truncated.as_str());
        let mut style = Style::default();
        if let Some(fg) = span.fg {
            style = style.fg(fg);
        }
        if let Some(bg) = span.bg {
            style = style.bg(bg);
        }
        if span.bold {
            style = style.add_modifier(Modifier::BOLD);
        }

        visible.push(Span::styled(truncated, style));
    }

    visible
}

fn render_feature_toggle(app: &App, frame: &mut Frame, area: Rect) {
    // Build the keybinding cheatsheet rows that are appended below the toggle list.
    let kb = &app.config.keybindings;
    let cheatsheet: &[(&str, &str)] = &[
        (kb.zoom.as_str(), "zoom"),
        (kb.layout_cycle.as_str(), "layout cycle"),
        (kb.layout_picker.as_str(), "layout picker"),
        (kb.pane_left.as_str(), "pane left"),
        (kb.pane_right.as_str(), "pane right"),
        (kb.pane_up.as_str(), "pane up"),
        (kb.pane_down.as_str(), "pane down"),
        (kb.pane_next.as_str(), "pane next"),
        (kb.pane_prev.as_str(), "pane prev"),
        (kb.pane_create.as_str(), "pane create"),
        (kb.pane_close.as_str(), "pane close"),
        (kb.split_vertical.as_str(), "split vertical"),
        (kb.split_horizontal.as_str(), "split horizontal"),
        (kb.tab_new.as_str(), "tab new"),
        (kb.tab_rename.as_str(), "tab rename"),
        (kb.tab_next.as_str(), "tab next"),
        (kb.tab_prev.as_str(), "tab prev"),
        (kb.file_tree.as_str(), "file tree"),
        (kb.preview_swap.as_str(), "preview swap"),
        (kb.feature_toggle.as_str(), "feature toggle"),
        (kb.clipboard_copy.as_str(), "clipboard copy"),
        (kb.ai_title_toggle.as_str(), "ai title toggle"),
        (kb.quit.as_str(), "quit"),
        ("alt+s", "status bar toggle"),
        ("alt+1-9", "jump to tab N"),
    ];

    // Sized to fit features + divider + cheatsheet + 3 lines of chrome (title,
    // separator, hints). Capped to area height so it never overflows.
    let dialog_width = 56u16;
    let content_lines = (FEATURES.len() + cheatsheet.len() + 10) as u16;
    let dialog_height = content_lines.saturating_add(3).min(area.height);

    let x = area.x + area.width.saturating_sub(dialog_width) / 2;
    let y = area.y + area.height.saturating_sub(dialog_height) / 2;
    let dialog_rect = Rect::new(x, y, dialog_width.min(area.width), dialog_height);

    frame.render_widget(Clear, dialog_rect);

    let selected = app.feature_toggle.selected;

    let outer_block = Block::default()
        .title(" Features ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FOCUS_BORDER))
        .style(Style::default().bg(PANEL_BG));
    frame.render_widget(outer_block, dialog_rect);

    let inner = Rect::new(
        dialog_rect.x + 2,
        dialog_rect.y + 1,
        dialog_rect.width.saturating_sub(4),
        dialog_rect.height.saturating_sub(2),
    );

    let mut lines: Vec<Line> = Vec::new();

    for (i, &(key, desc)) in FEATURES.iter().enumerate() {
        let enabled = app.feature_toggle.pending.get_by_key(key);
        let checkbox = if enabled { "[\u{2705}]" } else { "[  ]" };
        let is_selected = i == selected;

        let style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(FOCUS_BORDER)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };

        let marker = if is_selected { " > " } else { "   " };

        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(FOCUS_BORDER)),
            Span::styled(format!("{} {}  {}", checkbox, key, desc), style),
        ]));
    }

    // Divider + Keybindings cheatsheet
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Keybindings",
        Style::default()
            .fg(ACCENT_BLUE)
            .add_modifier(Modifier::BOLD),
    )));
    for (binding, action) in cheatsheet {
        let display = keybinding::keybinding_display(binding);
        // 14-col left column keeps the action descriptions vertically aligned.
        lines.push(Line::from(vec![
            Span::styled(
                format!("   {:<14}", display),
                Style::default().fg(ACCENT_BLUE),
            ),
            Span::styled(action.to_string(), Style::default().fg(TEXT_DIM)),
        ]));
    }

    let prefix_display = keybinding::keybinding_display(kb.prefix.as_str());
    let quit_display = keybinding::keybinding_display(kb.quit.as_str());
    let layout_cycle_display = keybinding::keybinding_display(kb.layout_cycle.as_str());
    let prefix_ops: &[(&str, &str)] = &[
        (&quit_display, "quit"),
        (&layout_cycle_display, "layout cycle"),
        ("[", "copy mode"),
        ("w", "pane list"),
    ];
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Prefix operations",
        Style::default().fg(ACCENT_BLUE).add_modifier(Modifier::BOLD),
    )));
    for (key, action) in prefix_ops {
        let combo = format!("{}+{}", prefix_display, key);
        lines.push(Line::from(vec![
            Span::styled(format!("   {:<14}", combo), Style::default().fg(ACCENT_BLUE)),
            Span::styled(action.to_string(), Style::default().fg(TEXT_DIM)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " j/k: move  Space: toggle  Enter/q: apply  Esc: cancel",
        Style::default().fg(TEXT_DIM),
    )));

    // Keep the selected feature row centered when the content overflows the
    // dialog. Without this scroll the list clips on small terminals.
    let visible_height = inner.height as usize;
    let scroll_top = selected.saturating_sub(visible_height / 2);
    let scroll_top = scroll_top.min(lines.len().saturating_sub(visible_height));

    let para = Paragraph::new(lines)
        .style(Style::default().bg(PANEL_BG))
        .scroll((scroll_top as u16, 0));
    frame.render_widget(para, inner);
}

fn vt100_color_to_ratatui(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(idx) => Color::Indexed(idx),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn render_pane_create_dialog(frame: &mut Frame, area: Rect, dialog: &PaneCreateDialog) {
    // Inner row layout (Single mode):
    //   0  Mode toggle  [Single]/[Multi]
    //   1  Branch
    //   2  Base
    //   3  Worktree
    //   4  Agent
    //   5  "Prompt:" label + prompt box top border
    //   6-12  7 visible prompt rows
    //   13 prompt box bottom border
    //   14 buttons
    //   15 error
    //   16 hint
    // Multi mode reuses rows 0, 5..13 (mode + prompt) and renders agent
    // checkboxes in rows 1..4, leaving 14/15/16 for buttons/error/hint.
    const PROMPT_VISIBLE: usize = 7;
    // popup_h = 1 mode + 4 fields + 1 label + PROMPT_VISIBLE rows + 2 border
    //          + 1 gap + 1 buttons + 2 hint/err + 2 spare
    let popup_h = (1 + 4 + 1 + PROMPT_VISIBLE as u16 + 2 + 1 + 1 + 2 + 2)
        .min(area.height.saturating_sub(4));
    let popup_w = area.width.saturating_sub(8).max(70).min(area.width.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = Rect::new(x, y, popup_w, popup_h);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" New Pane ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(PANEL_BG));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let hl = Style::default().fg(Color::Yellow);
    let normal = Style::default().fg(TEXT);
    let multi = dialog.launch_mode == LaunchMode::Multi;

    // -- Mode toggle row (row 0) --
    let mode_focused = dialog.focused_field == PaneCreateField::LaunchModeToggle;
    let single_active = dialog.launch_mode == LaunchMode::Single;
    let multi_active = dialog.launch_mode == LaunchMode::Multi;
    let single_style = if mode_focused && single_active {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else if single_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let multi_style = if mode_focused && multi_active {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else if multi_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let mode_line = Line::from(vec![
        Span::raw("Mode: "),
        Span::styled("[Single]", single_style),
        Span::raw("  "),
        Span::styled("[Multi]", multi_style),
    ]);
    frame.render_widget(
        Paragraph::new(mode_line),
        Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), 1),
    );

    // -- Single-only fields (Branch / Base / Worktree / Agent) on rows 1..4 --
    if !multi {
        let branch_style = if dialog.focused_field == PaneCreateField::BranchName {
            hl
        } else {
            normal
        };
        frame.render_widget(
            Paragraph::new(format!("Branch: [{}]", dialog.branch_name)).style(branch_style),
            Rect::new(inner.x + 1, inner.y + 1, inner.width.saturating_sub(2), 1),
        );

        let base_style = if dialog.focused_field == PaneCreateField::BaseBranch {
            hl
        } else {
            normal
        };
        frame.render_widget(
            Paragraph::new(format!("Base:   [{}]", dialog.base_branch)).style(base_style),
            Rect::new(inner.x + 1, inner.y + 2, inner.width.saturating_sub(2), 1),
        );

        let wt_style = if dialog.focused_field == PaneCreateField::WorktreeToggle {
            hl
        } else {
            normal
        };
        let wt_check = if dialog.worktree_enabled { "x" } else { " " };
        frame.render_widget(
            Paragraph::new(format!("Worktree: [{}] create", wt_check)).style(wt_style),
            Rect::new(inner.x + 1, inner.y + 3, inner.width.saturating_sub(2), 1),
        );

        let agent_style = if dialog.focused_field == PaneCreateField::AgentField {
            hl
        } else {
            normal
        };
        let agent_display = if dialog.agent.is_empty() {
            "none"
        } else {
            &dialog.agent
        };
        frame.render_widget(
            Paragraph::new(format!("Agent:  [{}]", agent_display)).style(agent_style),
            Rect::new(inner.x + 1, inner.y + 4, inner.width.saturating_sub(2), 1),
        );
    } else {
        // Multi mode: render up to 4 agent checkboxes on rows 1..4.
        for (i, label) in dialog.agent_labels.iter().enumerate().take(4) {
            let checked = dialog.agent_checks.get(i).copied().unwrap_or(false);
            let is_focused = dialog.focused_field == PaneCreateField::MultiCheck(i);
            let check_str = if checked { "[x]" } else { "[ ]" };
            let check_style = if checked {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let row_style = if is_focused { hl } else { normal };
            let line = Line::from(vec![
                Span::styled(check_str, check_style),
                Span::styled(format!(" {}", label), row_style),
            ]);
            frame.render_widget(
                Paragraph::new(line),
                Rect::new(
                    inner.x + 1,
                    inner.y + 1 + i as u16,
                    inner.width.saturating_sub(2),
                    1,
                ),
            );
        }
    }

    // -- Prompt multiline text area (row 5..) --
    let prompt_focused = dialog.focused_field == PaneCreateField::PromptField;
    let prompt_border_style = if prompt_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let label_style = if prompt_focused { hl } else { normal };
    frame.render_widget(
        Paragraph::new("Prompt:").style(label_style),
        Rect::new(inner.x + 1, inner.y + 5, 7, 1),
    );

    let box_x = inner.x + 9;
    let box_w = inner.width.saturating_sub(10);
    let box_y_top = inner.y + 5;
    // Draw border using a Block
    let text_area_outer = Rect::new(box_x, box_y_top, box_w, PROMPT_VISIBLE as u16 + 2);
    let text_area_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(prompt_border_style)
        .style(Style::default().bg(PANEL_BG));
    let text_inner = text_area_block.inner(text_area_outer);
    frame.render_widget(text_area_block, text_area_outer);

    // Build wrapped lines for the prompt
    let col_width = text_inner.width as usize;
    let rows = prompt_wrap_lines_ui(&dialog.prompt, col_width);
    let total_rows = rows.len();

    // Compute cursor visual row/col
    let (cursor_row, cursor_col) = prompt_cursor_row_col(&dialog.prompt, dialog.prompt_cursor, col_width);

    // Scroll: keep cursor_row within [scroll, scroll + PROMPT_VISIBLE)
    let scroll = {
        let mut s = dialog.prompt_scroll;
        if cursor_row < s {
            s = cursor_row;
        }
        if cursor_row >= s + PROMPT_VISIBLE {
            s = cursor_row + 1 - PROMPT_VISIBLE;
        }
        s
    };

    // Render visible rows
    for vis_row in 0..PROMPT_VISIBLE {
        let abs_row = scroll + vis_row;
        let render_y = text_inner.y + vis_row as u16;
        if abs_row >= total_rows {
            break;
        }
        let (row_start, row_char_len) = rows[abs_row];
        let row_text: String = dialog.prompt[row_start..]
            .chars()
            .take(row_char_len)
            .collect();

        if prompt_focused && abs_row == cursor_row {
            // Split at cursor column and render with cursor highlight
            let left: String = row_text.chars().take(cursor_col).collect();
            let cursor_char: String = row_text.chars().nth(cursor_col).map(|c| c.to_string()).unwrap_or_else(|| " ".to_string());
            let right: String = row_text.chars().skip(cursor_col + 1).collect();

            let spans = vec![
                Span::styled(left, hl),
                Span::styled(cursor_char, Style::default().fg(Color::Black).bg(Color::Yellow)),
                Span::styled(right, hl),
            ];
            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect::new(text_inner.x, render_y, text_inner.width, 1),
            );
        } else {
            frame.render_widget(
                Paragraph::new(row_text).style(normal),
                Rect::new(text_inner.x, render_y, text_inner.width, 1),
            );
        }
    }

    // Scrollbar indicator (right edge of border) when content overflows
    if total_rows > PROMPT_VISIBLE {
        let bar_h = PROMPT_VISIBLE;
        let thumb_size = (PROMPT_VISIBLE * bar_h / total_rows).max(1).min(bar_h);
        let max_scroll = total_rows.saturating_sub(PROMPT_VISIBLE);
        let thumb_pos = if max_scroll == 0 {
            0
        } else {
            (scroll * bar_h.saturating_sub(thumb_size) / max_scroll).min(bar_h.saturating_sub(thumb_size))
        };
        for i in 0..bar_h {
            let bar_char = if i >= thumb_pos && i < thumb_pos + thumb_size {
                "█"
            } else {
                "░"
            };
            frame.render_widget(
                Paragraph::new(bar_char).style(Style::default().fg(Color::DarkGray)),
                Rect::new(
                    text_area_outer.x + text_area_outer.width.saturating_sub(1),
                    text_area_outer.y + 1 + i as u16,
                    1,
                    1,
                ),
            );
        }
    }

    // Buttons row: below the text area box
    let buttons_y = box_y_top + PROMPT_VISIBLE as u16 + 2 + 1;

    let ai_focused = dialog.focused_field == PaneCreateField::AiGenerate;
    let ok_focused = dialog.focused_field == PaneCreateField::OkButton;
    let cancel_focused = dialog.focused_field == PaneCreateField::CancelButton;

    let ai_label = if dialog.generating_name {
        "[generating...]"
    } else {
        "[AI Generate]"
    };
    let ai_style = if ai_focused {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan)
    };
    let ok_style = if ok_focused {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else if dialog.generating_name {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(ACCENT_GREEN)
    };
    let cancel_style = if cancel_focused {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Red)
    };

    if !multi {
        frame.render_widget(
            Paragraph::new(ai_label).style(ai_style),
            Rect::new(inner.x + 1, buttons_y, 15, 1),
        );
    }
    frame.render_widget(
        Paragraph::new("[OK]").style(ok_style),
        Rect::new(inner.x + inner.width.saturating_sub(16), buttons_y, 4, 1),
    );
    frame.render_widget(
        Paragraph::new("[Cancel]").style(cancel_style),
        Rect::new(inner.x + inner.width.saturating_sub(10), buttons_y, 8, 1),
    );

    // Error / Hints
    if let Some(ref err) = dialog.error_msg {
        let err_y = inner.y + inner.height.saturating_sub(2);
        frame.render_widget(
            Paragraph::new(err.as_str()).style(Style::default().fg(Color::Red)),
            Rect::new(inner.x + 1, err_y, inner.width.saturating_sub(2), 1),
        );
    }

    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_text = if prompt_focused {
        "Tab: next  Enter: newline  Alt+Enter: confirm  Esc: cancel"
    } else {
        "Tab: next  Enter: confirm  Esc: cancel"
    };
    frame.render_widget(
        Paragraph::new(hint_text).style(Style::default().fg(TEXT_DIM)),
        Rect::new(inner.x + 1, hint_y, inner.width.saturating_sub(2), 1),
    );
}

/// Wrap `s` into visual rows of `width` chars, respecting '\n'.
/// Returns vec of (byte_start, char_len) per row.
fn prompt_wrap_lines_ui(s: &str, width: usize) -> Vec<(usize, usize)> {
    if width == 0 {
        return vec![(0, 0)];
    }
    let mut rows: Vec<(usize, usize)> = Vec::new();
    let mut byte_pos = 0usize;
    for logical_line in s.split('\n') {
        let chars: Vec<(usize, char)> = logical_line.char_indices().collect();
        if chars.is_empty() {
            rows.push((byte_pos, 0));
            byte_pos += 1;
            continue;
        }
        let mut start_char = 0usize;
        loop {
            let end_char = (start_char + width).min(chars.len());
            let row_byte_start = byte_pos + chars[start_char].0;
            rows.push((row_byte_start, end_char - start_char));
            start_char = end_char;
            if start_char >= chars.len() {
                break;
            }
        }
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

/// Return (row_index, col_in_row) for cursor byte position.
fn prompt_cursor_row_col(s: &str, cursor: usize, width: usize) -> (usize, usize) {
    if width == 0 {
        return (0, 0);
    }
    let rows = prompt_wrap_lines_ui(s, width);
    for (i, &(start, char_len)) in rows.iter().enumerate() {
        let next_start = if i + 1 < rows.len() {
            rows[i + 1].0
        } else {
            s.len() + 1
        };
        if cursor >= start && cursor < next_start {
            let col = s[start..cursor.min(s.len())].chars().count().min(char_len);
            return (i, col);
        }
    }
    let last = rows.len().saturating_sub(1);
    (last, rows.last().map(|r| r.1).unwrap_or(0))
}

fn render_close_confirm_dialog(frame: &mut Frame, area: Rect, dialog: &CloseConfirmDialog) {
    let popup_w = 40u16.min(area.width.saturating_sub(4));
    let popup_h = 5u16;
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = Rect::new(x, y, popup_w, popup_h);

    frame.render_widget(Clear, popup_area);

    let title = format!(" Close pane [{}]? ", dialog.pane_id);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(PANEL_BG));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let yes_style = if dialog.focused == CloseConfirmFocus::Yes {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(ACCENT_GREEN)
    };
    let no_style = if dialog.focused == CloseConfirmFocus::No {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT)
    };

    frame.render_widget(
        Paragraph::new("[Yes]").style(yes_style),
        Rect::new(inner.x + 2, inner.y + 1, 5, 1),
    );
    frame.render_widget(
        Paragraph::new("[No]").style(no_style),
        Rect::new(inner.x + inner.width.saturating_sub(7), inner.y + 1, 4, 1),
    );
}

fn render_worktree_cleanup_dialog(frame: &mut Frame, area: Rect, dialog: &WorktreeCleanupDialog) {
    let popup_w = 50u16.min(area.width.saturating_sub(4));
    let popup_h = 6u16;
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = Rect::new(x, y, popup_w, popup_h);

    frame.render_widget(Clear, popup_area);

    let title = format!(" Remove worktree [{}]? ", dialog.branch);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(PANEL_BG));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let msg = format!("Branch merged. Remove {}?", dialog.worktree_path.display());
    let msg_trunc = truncate_to_width(&msg, inner.width.saturating_sub(2) as usize);
    frame.render_widget(
        Paragraph::new(msg_trunc).style(Style::default().fg(TEXT)),
        Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), 1),
    );

    let yes_style = if dialog.focused == CloseConfirmFocus::Yes {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(ACCENT_GREEN)
    };
    let no_style = if dialog.focused == CloseConfirmFocus::No {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT)
    };

    frame.render_widget(
        Paragraph::new("[Yes]").style(yes_style),
        Rect::new(inner.x + 2, inner.y + 2, 5, 1),
    );
    frame.render_widget(
        Paragraph::new("[No]").style(no_style),
        Rect::new(inner.x + inner.width.saturating_sub(7), inner.y + 2, 4, 1),
    );
}

fn render_settings_panel(app: &App, frame: &mut Frame, area: Rect) {
    let dialog_width = 60u16;
    let dialog_height = (SETTINGS_ITEMS.len() as u16) + 6;

    let x = area.x + area.width.saturating_sub(dialog_width) / 2;
    let y = area.y + area.height.saturating_sub(dialog_height) / 2;
    let dialog_rect = Rect::new(
        x,
        y,
        dialog_width.min(area.width),
        dialog_height.min(area.height),
    );

    frame.render_widget(Clear, dialog_rect);

    let selected = app.settings_panel.selected;

    let outer_block = Block::default()
        .title(" \u{2699} Settings ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FOCUS_BORDER))
        .style(Style::default().bg(PANEL_BG));
    frame.render_widget(outer_block, dialog_rect);

    let inner = Rect::new(
        dialog_rect.x + 2,
        dialog_rect.y + 1,
        dialog_rect.width.saturating_sub(4),
        dialog_rect.height.saturating_sub(2),
    );

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, &(key_name, desc)) in SETTINGS_ITEMS.iter().enumerate() {
        let is_selected = i == selected;
        let value = app.get_setting_value(key_name);

        let marker = if is_selected { " > " } else { "   " };

        if is_selected && app.settings_panel.editing {
            let display = format!("{}: [{}_ ]", desc, app.settings_panel.edit_buffer);
            lines.push(Line::from(vec![
                Span::styled(marker, Style::default().fg(FOCUS_BORDER)),
                Span::styled(
                    display,
                    Style::default()
                        .fg(Color::Rgb(0xff, 0xd7, 0x00))
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        } else {
            let display = format!("{}: {}", desc, value);
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(FOCUS_BORDER)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };

            lines.push(Line::from(vec![
                Span::styled(marker, Style::default().fg(FOCUS_BORDER)),
                Span::styled(display, style),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " j/k: Move  Enter: Edit  q: Close",
        Style::default().fg(TEXT_DIM),
    )));

    let para = Paragraph::new(lines).style(Style::default().bg(PANEL_BG));
    frame.render_widget(para, inner);
}

fn render_status_flash(msg: &str, frame: &mut Frame, area: Rect) {
    let msg_width = unicode_width::UnicodeWidthStr::width(msg) as u16 + 4;
    let flash_width = msg_width.min(area.width);
    let x = area.x + area.width.saturating_sub(flash_width) / 2;
    let y = area.y + area.height.saturating_sub(2);
    let flash_rect = Rect::new(x, y, flash_width, 1);

    let widget = Paragraph::new(format!(" {} ", msg))
        .style(
            Style::default()
                .fg(Color::Rgb(0xff, 0xd7, 0x00))
                .bg(Color::Rgb(0x2e, 0x2a, 0x1a))
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center);
    frame.render_widget(widget, flash_rect);
}

fn render_layout_picker(app: &App, frame: &mut Frame, area: Rect) {
    let dialog_width = 52u16;
    let dialog_height = 14u16;

    let x = area.x + area.width.saturating_sub(dialog_width) / 2;
    let y = area.y + area.height.saturating_sub(dialog_height) / 2;
    let dialog_rect = Rect::new(
        x,
        y,
        dialog_width.min(area.width),
        dialog_height.min(area.height),
    );

    frame.render_widget(Clear, dialog_rect);

    let pane_count = app.ws().layout.pane_count();
    let selected = app.layout_picker.selected;

    let labels = [
        "[1] Stack",
        "[2] Two Split",
        "[3] Grid",
        "[4] Main+Sub",
        "[5] Big1+3",
        "[6] Auto",
    ];
    let min_counts = [1usize, 2, 4, 3, 4, 1];

    let outer_block = Block::default()
        .title(" Layout Picker ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FOCUS_BORDER))
        .style(Style::default().bg(PANEL_BG));
    frame.render_widget(outer_block, dialog_rect);

    let inner = Rect::new(
        dialog_rect.x + 2,
        dialog_rect.y + 1,
        dialog_rect.width.saturating_sub(4),
        dialog_rect.height.saturating_sub(2),
    );

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, label) in labels.iter().enumerate() {
        let available = pane_count >= min_counts[i];
        let style = if !available {
            Style::default().fg(TEXT_DIM)
        } else if selected == i {
            Style::default()
                .fg(Color::Black)
                .bg(FOCUS_BORDER)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };

        let marker = if selected == i { " > " } else { "   " };
        let suffix = if !available { " (need more panes)" } else { "" };

        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(FOCUS_BORDER)),
            Span::styled(*label, style),
            Span::styled(suffix, Style::default().fg(TEXT_DIM)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " 1-6: select  j/k: move  Enter: apply  Esc: close",
        Style::default().fg(TEXT_DIM),
    )));

    let para = Paragraph::new(lines).style(Style::default().bg(PANEL_BG));
    frame.render_widget(para, inner);
}

fn build_pane_list_lines(app: &App, is_focused: bool, available_width: u16) -> Vec<Line<'_>> {
    let overlay = &app.pane_list_overlay;
    let status_colors = StatusColors::from_config(&app.config.status);
    let highlight_bg = if is_focused { FOCUS_BORDER } else { BORDER };
    let mut lines: Vec<Line> = Vec::new();

    for (i, &pane_id) in overlay.pane_ids.iter().enumerate() {
        let label = app
            .pane_display_title(pane_id)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Pane {}", pane_id));

        let dot_info: Option<(&str, Color)> = if app.config.features.status_dot {
            match app.pane_status(pane_id) {
                PaneStatus::Idle => None,
                PaneStatus::Running => Some(("\u{25cf}", status_colors.running)),
                PaneStatus::Done => Some(("\u{25cf}", status_colors.done)),
                PaneStatus::Waiting => Some(("\u{25cf}", status_colors.waiting)),
            }
        } else {
            None
        };

        let branch_name = app
            .ws()
            .panes
            .get(&pane_id)
            .and_then(|p| p.branch_name.as_deref())
            .unwrap_or("");
        let branch_part = if branch_name.is_empty() {
            String::new()
        } else {
            format!("  {}", branch_name)
        };

        let is_selected = i == overlay.selected;
        let style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(highlight_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };

        // marker(3) + "[N] "(4+digits) + "  "(2) + dot+space(2 if present) + branch
        let marker = if is_selected { " > " } else { "   " };
        let prefix_width = 3usize + format!("[{}] ", i).len();
        let dot_width = if dot_info.is_some() { 2usize } else { 0 };
        let branch_width = unicode_width::UnicodeWidthStr::width(branch_part.as_str());
        let suffix_width = 2 + dot_width + branch_width; // "  " separator + dot + branch
        let title_budget = (available_width as usize)
            .saturating_sub(prefix_width)
            .saturating_sub(suffix_width);
        let truncated_label = truncate_to_width(&label, title_budget);

        let mut row_spans = vec![Span::styled(marker, Style::default().fg(FOCUS_BORDER))];
        row_spans.push(Span::styled(format!("[{}] {}  ", i, truncated_label), style));
        if let Some((dot, color)) = dot_info {
            row_spans.push(Span::styled(
                format!("{} ", dot),
                Style::default().fg(color),
            ));
        }
        row_spans.push(Span::styled(branch_part, style));
        lines.push(Line::from(row_spans));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " j/k: Move  Enter: Select  Esc: Close",
        Style::default().fg(TEXT_DIM),
    )));

    lines
}

fn render_pane_list_sidebar(app: &App, frame: &mut Frame, area: Rect) {
    let is_focused = app.ws().focus_target == FocusTarget::PaneList;
    let border_color = if is_focused { FOCUS_BORDER } else { BORDER };

    let title_style = if is_focused {
        Style::default()
            .fg(ACCENT_BLUE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_DIM)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(" Panes ", title_style))
        .style(Style::default().bg(PANEL_BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let lines = build_pane_list_lines(app, is_focused, inner.width);

    let para = Paragraph::new(lines).style(Style::default().bg(PANEL_BG));
    frame.render_widget(para, inner);
}

fn render_pane_list_overlay(app: &App, frame: &mut Frame, area: Rect) {
    let overlay = &app.pane_list_overlay;
    let count = overlay.pane_ids.len();
    let dialog_width = 50u16;
    let dialog_height = (count as u16).saturating_add(4);

    let x = area.x + area.width.saturating_sub(dialog_width) / 2;
    let y = area.y + area.height.saturating_sub(dialog_height) / 2;
    let dialog_rect = Rect::new(
        x,
        y,
        dialog_width.min(area.width),
        dialog_height.min(area.height),
    );

    if dialog_rect.height < 4 {
        return;
    }

    frame.render_widget(Clear, dialog_rect);

    let outer_block = Block::default()
        .title(" Pane List ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FOCUS_BORDER))
        .style(Style::default().bg(PANEL_BG));
    let inner = outer_block.inner(dialog_rect);
    frame.render_widget(outer_block, dialog_rect);

    let lines = build_pane_list_lines(app, true, inner.width);

    let para = Paragraph::new(lines).style(Style::default().bg(PANEL_BG));
    frame.render_widget(para, inner);
}

fn render_filetree_action_popup(app: &App, frame: &mut Frame, area: Rect) {
    let popup = &app.filetree_action_popup;
    let popup_w = 26u16.min(area.width.saturating_sub(4));
    let popup_h = 8u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = Rect::new(x, y, popup_w, popup_h);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Open File ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FOCUS_BORDER))
        .style(Style::default().bg(PANEL_BG));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let options = ["Preview", "Open in Editor"];

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, label) in options.iter().enumerate() {
        let is_selected = i == popup.selected;
        let style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(FOCUS_BORDER)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };
        let marker = if is_selected { " > " } else { "   " };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(FOCUS_BORDER)),
            Span::styled(format!("[{}]", label), style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Enter: Select  Esc: Close",
        Style::default().fg(TEXT_DIM),
    )));

    let para = Paragraph::new(lines).style(Style::default().bg(PANEL_BG));
    frame.render_widget(para, inner);
}
