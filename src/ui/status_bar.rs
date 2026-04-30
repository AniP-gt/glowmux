use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{FocusTarget, PaneStatus};
use crate::keybinding;

use super::{
    make_progress_bar, format_tokens, truncate_to_width,
    HEADER_BG, TEXT_DIM, ACCENT_GREEN, ACCENT_BLUE, ACCENT_CLAUDE,
};

pub(super) fn render_status_bar(app: &crate::app::App, frame: &mut Frame, area: Rect) {
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

pub(super) fn render_status_flash(msg: &str, frame: &mut Frame, area: Rect) {
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

