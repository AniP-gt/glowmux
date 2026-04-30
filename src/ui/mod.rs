use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::app::{App, SidebarMode};

mod panes;
mod preview;
mod sidebar;
mod dialogs;
mod status_bar;

use panes::render_panes;
use preview::render_preview;
use sidebar::{render_file_tree, render_pane_list_sidebar, render_pane_list_overlay, render_filetree_action_popup};
use dialogs::{render_feature_toggle, render_pane_create_dialog, render_close_confirm_dialog, render_worktree_cleanup_dialog, render_settings_panel, render_layout_picker};
use status_bar::{render_status_bar, render_status_flash};

// ─── Theme (Claude-inspired) ──────────────────────────────
pub(super) const BG: Color = Color::Rgb(0x0d, 0x11, 0x17);
pub(super) const PANEL_BG: Color = Color::Rgb(0x13, 0x17, 0x1f);
pub(super) const BORDER: Color = Color::Rgb(0x2d, 0x33, 0x3b);
pub(super) const FOCUS_BORDER: Color = Color::Rgb(0x58, 0xa6, 0xff);
pub(super) const TEXT: Color = Color::Rgb(0xe6, 0xed, 0xf3);
pub(super) const TEXT_DIM: Color = Color::Rgb(0x6e, 0x76, 0x81);
pub(super) const ACCENT_GREEN: Color = Color::Rgb(0x3f, 0xb9, 0x50);
pub(super) const ACCENT_BLUE: Color = Color::Rgb(0x58, 0xa6, 0xff);
pub(super) const ACCENT_CLAUDE: Color = Color::Rgb(0xd9, 0x77, 0x57);
pub(super) const HEADER_BG: Color = Color::Rgb(0x16, 0x1b, 0x22);
pub(super) const ACTIVE_TAB_BG: Color = Color::Rgb(0x0d, 0x11, 0x17);
pub(super) const ACTIVE_BG: Color = Color::Rgb(0x1c, 0x23, 0x33);
pub(super) const LINE_NUM_COLOR: Color = Color::Rgb(0x3d, 0x44, 0x4d);
pub(super) const SCROLL_BG: Color = Color::Rgb(0x2a, 0x1f, 0x14);

const MIN_TERMINAL_WIDTH: u16 = 40;
const MIN_TERMINAL_HEIGHT: u16 = 10;
const MIN_PANE_AREA_WIDTH: u16 = 20;

// ─── File type icons ──────────────────────────────────────
pub(super) fn file_icon(name: &str) -> (&'static str, Color) {
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

pub(super) fn git_status_icon(state: Option<crate::git_status::GitFileState>) -> (&'static str, Color) {
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

// ─── Helpers ──────────────────────────────────────────────

/// Build a progress bar string like `▓▓▓▓░░░░░░`.
pub(super) fn make_progress_bar(current: usize, total: usize, width: usize) -> String {
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
pub(super) fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

pub(super) fn truncate_to_width(s: &str, max_width: usize) -> String {
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
