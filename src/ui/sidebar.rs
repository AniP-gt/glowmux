use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{DragTarget, FocusTarget, PaneStatus};

use super::{
    file_icon, git_status_icon, truncate_to_width,
    BORDER, PANEL_BG, TEXT, TEXT_DIM, ACCENT_GREEN, ACCENT_BLUE,
    FOCUS_BORDER, ACTIVE_BG,
};
use super::panes::StatusColors;

pub(super) fn render_file_tree(app: &mut crate::app::App, frame: &mut Frame, area: Rect) {
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
        let git_state = git_status.and_then(|snapshot: &crate::git_status::GitStatusSnapshot| snapshot.state_for(&entry.path));
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

pub(super) fn build_pane_list_lines<'a>(app: &'a crate::app::App, is_focused: bool, available_width: u16) -> Vec<Line<'a>> {
    let overlay = &app.pane_list_overlay;
    let status_colors = StatusColors::from_config(&app.config.status);
    let highlight_bg = if is_focused { FOCUS_BORDER } else { BORDER };
    let mut lines: Vec<Line> = Vec::new();

    for (i, &pane_id) in overlay.pane_ids.iter().enumerate() {
        let label = app
            .pane_display_title(pane_id)
            .map(|s: &str| s.to_string())
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

pub(super) fn render_pane_list_sidebar(app: &crate::app::App, frame: &mut Frame, area: Rect) {
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

pub(super) fn render_pane_list_overlay(app: &crate::app::App, frame: &mut Frame, area: Rect) {
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
        .title_alignment(ratatui::layout::Alignment::Center)
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

pub(super) fn render_filetree_action_popup(app: &crate::app::App, frame: &mut Frame, area: Rect) {
    let popup = &app.filetree_action_popup;
    let popup_w = 26u16.min(area.width.saturating_sub(4));
    let popup_h = 8u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = Rect::new(x, y, popup_w, popup_h);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Open File ")
        .title_alignment(ratatui::layout::Alignment::Center)
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
