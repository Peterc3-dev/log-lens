use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::text::{Line, Span};

use crate::app::{App, InputMode, PopupState};
use crate::highlight;

/// Phosphor green default style
const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 200);
const DIM_GREEN: Color = Color::Rgb(0, 128, 100);
const BG_COLOR: Color = Color::Rgb(8, 12, 8);

pub fn draw(frame: &mut Frame, app: &mut App) {
    let size = frame.area();

    // Layout: [log area] [status bar] [input bar (if active)]
    let input_height = if app.input_mode != InputMode::Normal { 1 } else { 0 };
    let constraints = if input_height > 0 {
        vec![
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ]
    } else {
        vec![
            Constraint::Min(3),
            Constraint::Length(1),
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(size);

    let log_area = chunks[0];
    let status_area = chunks[1];

    // Update viewport height for scroll calculations
    // Subtract 2 for block borders
    app.viewport_height = log_area.height.saturating_sub(2) as usize;

    // Draw log view
    draw_log_view(frame, app, log_area);

    // Draw status bar
    draw_status_bar(frame, app, status_area);

    // Draw input bar if needed
    if input_height > 0 {
        draw_input_bar(frame, app, chunks[2]);
    }

    // Draw bookmark popup if active
    if app.popup == PopupState::BookmarkList {
        draw_bookmark_popup(frame, app, size);
    }
}

fn draw_log_view(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DIM_GREEN))
        .title(Span::styled(" log-lens ", Style::default().fg(PHOSPHOR_GREEN).add_modifier(Modifier::BOLD)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.filtered_indices.is_empty() {
        let msg = if app.lines.is_empty() {
            "Waiting for input..."
        } else {
            "No lines match current filters"
        };
        let p = Paragraph::new(msg)
            .style(Style::default().fg(DIM_GREEN));
        frame.render_widget(p, inner);
        return;
    }

    let height = inner.height as usize;
    let width = inner.width as usize;
    let multi_source = app.source_names.len() > 1;

    // Line number column width
    let max_line = app.filtered_indices.last().copied().unwrap_or(0) + 1;
    let ln_width = format!("{}", max_line).len().max(4);

    let start = app.scroll_offset;
    let end = (start + height).min(app.filtered_indices.len());

    let mut lines_out: Vec<Line> = Vec::with_capacity(height);

    for fi in start..end {
        let li = app.filtered_indices[fi];
        if let Some(log_line) = app.lines.get(li) {
            let content = &log_line.content;
            let is_bookmarked = app.bookmarks.contains(&li);
            let is_search_match = app.search.regex.as_ref()
                .map(|r| r.is_match(content))
                .unwrap_or(false);

            let mut spans: Vec<Span> = Vec::new();

            // Bookmark marker
            if is_bookmarked {
                spans.push(Span::styled("★ ", Style::default().fg(Color::Yellow)));
            } else {
                spans.push(Span::raw("  "));
            }

            // Line number
            let ln_str = format!("{:>width$} │ ", li + 1, width = ln_width);
            spans.push(Span::styled(ln_str, Style::default().fg(DIM_GREEN)));

            // Source tag for multi-file mode
            if multi_source {
                let src_name = app.source_names.get(log_line.source_idx)
                    .map(|s| s.as_str())
                    .unwrap_or("?");
                // Truncate source name
                let short: String = if src_name.len() > 12 {
                    format!("{}… ", &src_name[..11])
                } else {
                    format!("{} ", src_name)
                };
                let color = highlight::source_color(log_line.source_idx);
                spans.push(Span::styled(short, Style::default().fg(color)));
            }

            // Determine base style from log level
            let level = highlight::detect_level(content);
            let base_style = level.map(|l| l.style())
                .unwrap_or(Style::default().fg(PHOSPHOR_GREEN));

            // Check for JSON
            if highlight::is_json_line(content) {
                // Render JSON with key highlighting
                render_json_spans(&mut spans, content, base_style, &app.search, width);
            } else {
                // Render with timestamp + search highlighting
                render_line_spans(&mut spans, content, base_style, &app.search, is_search_match);
            }

            lines_out.push(Line::from(spans));
        }
    }

    let paragraph = if app.line_wrap {
        Paragraph::new(lines_out)
            .style(Style::default().bg(BG_COLOR))
            .wrap(Wrap { trim: false })
    } else {
        Paragraph::new(lines_out)
            .style(Style::default().bg(BG_COLOR))
    };

    frame.render_widget(paragraph, inner);
}

fn render_line_spans<'a>(
    spans: &mut Vec<Span<'a>>,
    content: &'a str,
    base_style: Style,
    search: &crate::app::SearchState,
    is_search_match: bool,
) {
    let timestamp_style = Style::default().fg(Color::Rgb(100, 160, 255));

    // Collect highlight regions: timestamps + search matches
    let mut regions: Vec<(usize, usize, Style)> = Vec::new();

    // Timestamps
    for (s, e) in highlight::find_timestamps(content) {
        regions.push((s, e, timestamp_style));
    }

    // Search highlights
    if is_search_match {
        if let Some(ref regex) = search.regex {
            for m in regex.find_iter(content) {
                regions.push((
                    m.start(),
                    m.end(),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ));
            }
        }
    }

    if regions.is_empty() {
        spans.push(Span::styled(content.to_string(), base_style));
        return;
    }

    // Sort regions by start position; search highlights take priority (added last, so we reverse for stable overlap)
    regions.sort_by_key(|r| (r.0, std::cmp::Reverse(r.1)));

    // Merge/render
    let mut pos = 0;
    for (start, end, style) in &regions {
        let start = *start;
        let end = *end;
        if start > pos {
            spans.push(Span::styled(&content[pos..start], base_style));
        }
        if start >= pos {
            spans.push(Span::styled(&content[start..end], *style));
            pos = end;
        }
    }
    if pos < content.len() {
        spans.push(Span::styled(&content[pos..], base_style));
    }
}

fn render_json_spans<'a>(
    spans: &mut Vec<Span<'a>>,
    content: &'a str,
    base_style: Style,
    search: &crate::app::SearchState,
    _width: usize,
) {
    let key_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let string_style = Style::default().fg(Color::Green);
    let number_style = Style::default().fg(Color::Yellow);
    let _null_style = Style::default().fg(Color::DarkGray);

    // Try to parse and do simple key=value inline rendering
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(content.trim()) {
        if let Some(obj) = v.as_object() {
            spans.push(Span::styled("{ ", base_style));
            let mut first = true;
            for (k, val) in obj {
                if !first {
                    spans.push(Span::styled(", ", base_style));
                }
                first = false;
                spans.push(Span::styled(format!("\"{}\"", k), key_style));
                spans.push(Span::styled(": ", base_style));
                match val {
                    serde_json::Value::String(s) => {
                        let display = format!("\"{}\"", s);
                        // Check for search match within value
                        if let Some(ref regex) = search.regex {
                            if regex.is_match(s) {
                                spans.push(Span::styled(
                                    display,
                                    Style::default().fg(Color::Black).bg(Color::Yellow),
                                ));
                                continue;
                            }
                        }
                        spans.push(Span::styled(display, string_style));
                    }
                    serde_json::Value::Number(n) => {
                        spans.push(Span::styled(n.to_string(), number_style));
                    }
                    serde_json::Value::Bool(b) => {
                        spans.push(Span::styled(
                            b.to_string(),
                            Style::default().fg(Color::Magenta),
                        ));
                    }
                    serde_json::Value::Null => {
                        spans.push(Span::styled("null", Style::default().fg(Color::DarkGray)));
                    }
                    other => {
                        spans.push(Span::styled(other.to_string(), base_style));
                    }
                }
            }
            spans.push(Span::styled(" }", base_style));
            return;
        }
    }

    // Fallback: plain rendering
    spans.push(Span::styled(content.to_string(), base_style));
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let mut parts: Vec<Span> = Vec::new();

    // Mode indicator
    let mode_str = if app.auto_scroll { " FOLLOW " } else { " SCROLL " };
    let mode_style = if app.auto_scroll {
        Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Black).bg(PHOSPHOR_GREEN).add_modifier(Modifier::BOLD)
    };
    parts.push(Span::styled(mode_str, mode_style));
    parts.push(Span::styled(" ", Style::default()));

    // Line wrap indicator
    if app.line_wrap {
        parts.push(Span::styled("[WRAP] ", Style::default().fg(Color::Cyan)));
    }

    // Line counts
    let count_str = format!(
        "Lines: {}/{} ",
        app.filtered_count(),
        app.total_count(),
    );
    parts.push(Span::styled(count_str, Style::default().fg(PHOSPHOR_GREEN)));

    // Filter indicators
    if !app.filters.is_empty() {
        parts.push(Span::styled("│ ", Style::default().fg(DIM_GREEN)));
        parts.push(Span::styled("Filters: ", Style::default().fg(Color::Yellow)));
        for (i, f) in app.filters.iter().enumerate() {
            if i > 0 {
                parts.push(Span::styled(", ", Style::default().fg(DIM_GREEN)));
            }
            let prefix = if f.inverse { "!" } else { "/" };
            parts.push(Span::styled(
                format!("{}{}", prefix, f.pattern),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ));
        }
        parts.push(Span::raw(" "));
    }

    // Search indicator
    if !app.search.pattern.is_empty() {
        parts.push(Span::styled("│ ", Style::default().fg(DIM_GREEN)));
        let match_info = if app.search.match_indices.is_empty() {
            "0/0".to_string()
        } else {
            format!(
                "{}/{}",
                app.search.current_match.map(|i| i + 1).unwrap_or(0),
                app.search.match_indices.len()
            )
        };
        parts.push(Span::styled(
            format!("Search: {} [{}] ", app.search.pattern, match_info),
            Style::default().fg(Color::Cyan),
        ));
    }

    // Bookmark count
    if !app.bookmarks.is_empty() {
        parts.push(Span::styled("│ ", Style::default().fg(DIM_GREEN)));
        parts.push(Span::styled(
            format!("★{} ", app.bookmarks.len()),
            Style::default().fg(Color::Yellow),
        ));
    }

    // Help hint (right-aligned, fill remaining space)
    let used: usize = parts.iter().map(|s| s.content.len()).sum();
    let remaining = (area.width as usize).saturating_sub(used);
    let help = "q:quit /:filter !:inv f:follow w:wrap ^F:search b:mark";
    if remaining > help.len() + 2 {
        let padding = remaining - help.len();
        parts.push(Span::styled(
            " ".repeat(padding),
            Style::default(),
        ));
        parts.push(Span::styled(help, Style::default().fg(DIM_GREEN)));
    }

    let status_line = Line::from(parts);
    let p = Paragraph::new(status_line)
        .style(Style::default().bg(Color::Rgb(20, 30, 20)));
    frame.render_widget(p, area);
}

fn draw_input_bar(frame: &mut Frame, app: &App, area: Rect) {
    let prompt = match app.input_mode {
        InputMode::FilterInput => "/",
        InputMode::InverseFilterInput => "!",
        InputMode::SearchInput => "search: ",
        InputMode::Normal => "",
    };

    let input_line = Line::from(vec![
        Span::styled(prompt, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(&app.input_buffer, Style::default().fg(PHOSPHOR_GREEN)),
        Span::styled("█", Style::default().fg(PHOSPHOR_GREEN)), // cursor
    ]);

    let p = Paragraph::new(input_line)
        .style(Style::default().bg(BG_COLOR));
    frame.render_widget(p, area);
}

fn draw_bookmark_popup(frame: &mut Frame, app: &App, area: Rect) {
    let popup_width = (area.width as usize).min(60).max(30) as u16;
    let popup_height = (app.bookmarks.len() as u16 + 4).min(area.height - 4);

    let x = (area.width - popup_width) / 2;
    let y = (area.height - popup_height) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(
            " Bookmarks (Enter:jump d:delete Esc:close) ",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let mut lines: Vec<Line> = Vec::new();
    for (i, &line_idx) in app.bookmarks.iter().enumerate() {
        let selected = i == app.bookmark_scroll;
        let content = app.lines.get(line_idx)
            .map(|l| l.content.as_str())
            .unwrap_or("<deleted>");

        let prefix = if selected { "▸ " } else { "  " };
        let truncated: String = content.chars().take((inner.width as usize).saturating_sub(10)).collect();
        let line_str = format!("{}L{}: {}", prefix, line_idx + 1, truncated);

        let style = if selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(PHOSPHOR_GREEN)
        };

        lines.push(Line::styled(line_str, style));
    }

    let p = Paragraph::new(lines)
        .style(Style::default().bg(BG_COLOR));
    frame.render_widget(p, inner);
}
