use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{DragTarget, FocusTarget};

use super::{
    truncate_to_width,
    BORDER, PANEL_BG, TEXT, TEXT_DIM, ACCENT_GREEN, ACCENT_CLAUDE,
    LINE_NUM_COLOR,
};

pub(super) fn render_preview(app: &mut crate::app::App, frame: &mut Frame, area: Rect) {
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
                            .map(|s: &String| s.chars().count())
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

pub(super) fn diff_spans_with_scroll(
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
