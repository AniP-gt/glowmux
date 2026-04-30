use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{
    CloseConfirmDialog, CloseConfirmFocus, LaunchMode, PaneCreateDialog, PaneCreateField,
    WorktreeCleanupDialog, FEATURES, SETTINGS_ITEMS,
};
use crate::keybinding;

use super::{
    truncate_to_width,
    PANEL_BG, TEXT, TEXT_DIM, ACCENT_GREEN, ACCENT_BLUE, FOCUS_BORDER,
};

pub(super) fn render_feature_toggle(app: &crate::app::App, frame: &mut Frame, area: Rect) {
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

pub(super) fn render_pane_create_dialog(frame: &mut Frame, area: Rect, dialog: &PaneCreateDialog) {
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
pub(super) fn prompt_wrap_lines_ui(s: &str, width: usize) -> Vec<(usize, usize)> {
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
pub(super) fn prompt_cursor_row_col(s: &str, cursor: usize, width: usize) -> (usize, usize) {
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

pub(super) fn render_close_confirm_dialog(frame: &mut Frame, area: Rect, dialog: &CloseConfirmDialog) {
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

pub(super) fn render_worktree_cleanup_dialog(frame: &mut Frame, area: Rect, dialog: &WorktreeCleanupDialog) {
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

pub(super) fn render_settings_panel(app: &crate::app::App, frame: &mut Frame, area: Rect) {
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

pub(super) fn render_layout_picker(app: &crate::app::App, frame: &mut Frame, area: Rect) {
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
