use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Sparkline, Wrap},
    Frame,
};
use regex::Regex;

use crate::app::{App, InputMode, LogEntry, LogLevel, ViewMode};

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(5),    // log feed
            Constraint::Length(3), // footer / filter bar
        ])
        .split(frame.area());

    draw_header(frame, app, chunks[0]);

    // Build visible list once, reuse for feed + detail modal
    let visible = app.visible_logs();

    // Collect highlight regexes for rendering
    let mut hl_patterns: Vec<(&Regex, Style)> = Vec::new();
    if let Some(ref re) = app.search_regex {
        hl_patterns.push((re, Style::default().bg(Color::Yellow).fg(Color::Black)));
    }
    for (re, color) in &app.highlights {
        hl_patterns.push((re, Style::default().fg(*color).add_modifier(Modifier::BOLD)));
    }

    draw_log_feed(frame, app, &visible, &hl_patterns, chunks[1]);
    draw_footer(frame, app, chunks[2]);

    if app.view_mode == ViewMode::Detail {
        draw_detail_modal(frame, app, &visible);
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40), // stats
            Constraint::Percentage(60), // sparkline
        ])
        .split(area);

    // Stats
    let frozen_indicator = if app.frozen { " [PAUSED]" } else { "" };
    let error_only_indicator = if app.error_only { " [ERRORS]" } else { "" };

    let stats_text = format!(
        " {} | EPS: {} | Errors: {} | Total: {}{}{}",
        app.filename,
        app.current_eps,
        app.error_count,
        app.total_count,
        frozen_indicator,
        error_only_indicator
    );

    let stats = Paragraph::new(stats_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" LogPulse ")
            .style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(stats, header_chunks[0]);

    // Sparkline
    let spark_data: Vec<u64> = app.eps_history.iter().copied().collect();
    let sparkline = Sparkline::default()
        .block(Block::default().borders(Borders::ALL).title(" Activity "))
        .data(&spark_data)
        .style(Style::default().fg(Color::Green));
    frame.render_widget(sparkline, header_chunks[1]);
}

fn draw_log_feed(
    frame: &mut Frame,
    app: &App,
    visible: &[(usize, &LogEntry)],
    hl_patterns: &[(&Regex, Style)],
    area: Rect,
) {
    let total_visible = visible.len();
    if total_visible == 0 {
        let list = List::new(Vec::<ListItem>::new()).block(
            Block::default().borders(Borders::ALL).title(if app.frozen {
                " Log Feed [PAUSED - Space to resume] "
            } else {
                " Log Feed "
            }),
        );
        frame.render_widget(list, area);
        return;
    }

    // Defensive clamp — prevents panic if selected_index is stale
    let selected = app.selected_index.min(total_visible - 1);

    // Calculate viewport BEFORE creating ListItems
    let height = area.height.saturating_sub(2) as usize; // borders
    let offset = if app.frozen || selected < total_visible.saturating_sub(height) {
        selected.saturating_sub(height / 2)
    } else {
        total_visible.saturating_sub(height)
    };

    // Only create ListItems for the visible window
    let window_end = (offset + height + 1).min(total_visible);
    let items: Vec<ListItem> = visible[offset..window_end]
        .iter()
        .enumerate()
        .map(|(i, (_orig_idx, entry))| {
            let display_idx = offset + i;
            let line = colorize_entry(entry, app.horizontal_scroll, hl_patterns);
            let style = if display_idx == selected {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let title = if app.frozen {
        " Log Feed [PAUSED - Space to resume] "
    } else {
        " Log Feed "
    };

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title).style(
        Style::default().fg(if app.frozen {
            Color::Yellow
        } else {
            Color::White
        }),
    ));

    frame.render_widget(list, area);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let (content, title) = match app.input_mode {
        InputMode::Filter => {
            let input_line = Line::from(vec![
                Span::styled(" Filter: ", Style::default().fg(Color::Yellow)),
                Span::raw(&app.filter_text),
                Span::styled("_", Style::default().fg(Color::Yellow)),
            ]);
            (
                Paragraph::new(input_line),
                " Filter Mode (Esc cancel, Enter apply) ",
            )
        }
        InputMode::Search => {
            let input_line = Line::from(vec![
                Span::styled(" Search: ", Style::default().fg(Color::Yellow)),
                Span::raw(&app.input_buffer),
                Span::styled("_", Style::default().fg(Color::Yellow)),
            ]);
            (
                Paragraph::new(input_line),
                " Search Mode (Esc cancel, Enter apply, n/N navigate) ",
            )
        }
        InputMode::Highlight => {
            let input_line = Line::from(vec![
                Span::styled(
                    format!(" Highlight ({} active): ", app.highlights.len()),
                    Style::default().fg(Color::Magenta),
                ),
                Span::raw(&app.input_buffer),
                Span::styled("_", Style::default().fg(Color::Magenta)),
            ]);
            (
                Paragraph::new(input_line),
                " Highlight Mode (Esc cancel, Enter empty=clear) ",
            )
        }
        InputMode::SavePrompt => {
            let input_line = Line::from(vec![
                Span::styled(" Save to: ", Style::default().fg(Color::Green)),
                Span::raw(&app.input_buffer),
                Span::styled("_", Style::default().fg(Color::Green)),
            ]);
            (
                Paragraph::new(input_line),
                " Save Mode (Esc cancel, Enter save) ",
            )
        }
        InputMode::TimeJump => {
            let input_line = Line::from(vec![
                Span::styled(" Jump to time: ", Style::default().fg(Color::Cyan)),
                Span::raw(&app.input_buffer),
                Span::styled("_", Style::default().fg(Color::Cyan)),
            ]);
            (
                Paragraph::new(input_line),
                " Time Jump (e.g. 14:30, 2024-01-15) ",
            )
        }
        InputMode::Normal => {
            // Show status message if active, otherwise show help
            if let Some((ref msg, _)) = app.status_message {
                let status_line = Line::from(Span::styled(
                    format!(" {}", msg),
                    Style::default().fg(Color::Green),
                ));
                (Paragraph::new(status_line), " Status ")
            } else {
                let help = Line::from(vec![
                    Span::styled(" q", Style::default().fg(Color::Cyan)),
                    Span::raw(":quit "),
                    Span::styled("Space", Style::default().fg(Color::Cyan)),
                    Span::raw(":pause "),
                    Span::styled("/", Style::default().fg(Color::Cyan)),
                    Span::raw(":filter "),
                    Span::styled("?", Style::default().fg(Color::Cyan)),
                    Span::raw(":search "),
                    Span::styled("e", Style::default().fg(Color::Cyan)),
                    Span::raw(":errors "),
                    Span::styled("y", Style::default().fg(Color::Cyan)),
                    Span::raw(":copy "),
                    Span::styled("*", Style::default().fg(Color::Cyan)),
                    Span::raw(":mark "),
                    Span::styled("s", Style::default().fg(Color::Cyan)),
                    Span::raw(":save "),
                    Span::styled("g", Style::default().fg(Color::Cyan)),
                    Span::raw(":goto "),
                    Span::styled("Enter", Style::default().fg(Color::Cyan)),
                    Span::raw(":detail"),
                ]);
                (Paragraph::new(help), " Help ")
            }
        }
    };

    let block = Block::default().borders(Borders::ALL).title(title);
    frame.render_widget(content.block(block), area);
}

fn draw_detail_modal(frame: &mut Frame, app: &App, visible: &[(usize, &LogEntry)]) {
    let entry = match visible.get(app.selected_index) {
        Some((_, e)) => e,
        None => return,
    };

    let area = centered_rect(80, 80, frame.area());
    frame.render_widget(Clear, area);

    let content = if entry.raw.trim().starts_with('{') {
        // Try to pretty-print JSON
        match serde_json::from_str::<serde_json::Value>(entry.raw.trim()) {
            Ok(val) => {
                let mut s =
                    serde_json::to_string_pretty(&val).unwrap_or_else(|_| entry.raw.clone());
                if !entry.extra_lines.is_empty() {
                    s.push_str("\n\n--- Continuation ---\n");
                    for line in &entry.extra_lines {
                        s.push_str(line);
                        s.push('\n');
                    }
                }
                s
            }
            Err(_) => build_detail_text(entry),
        }
    } else {
        build_detail_text(entry)
    };

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Detail View (Esc to close) ")
                .style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn build_detail_text(entry: &LogEntry) -> String {
    let mut detail = String::new();
    if let Some(ts) = &entry.timestamp {
        detail.push_str(&format!("Timestamp: {}\n", ts));
    }
    detail.push_str(&format!("Level: {:?}\n", entry.level));
    if let Some(msg) = &entry.message {
        detail.push_str(&format!("Message: {}\n", msg));
    }
    if let Some(meta) = &entry.metadata {
        detail.push_str(&format!("Metadata: {}\n", meta));
    }
    detail.push_str(&format!("\n--- Raw ---\n{}", entry.raw));
    if !entry.extra_lines.is_empty() {
        detail.push_str("\n\n--- Continuation ---\n");
        for line in &entry.extra_lines {
            detail.push_str(line);
            detail.push('\n');
        }
    }
    detail
}

fn colorize_entry(
    entry: &LogEntry,
    h_scroll: usize,
    hl_patterns: &[(&Regex, Style)],
) -> Line<'static> {
    let color = level_color(entry.level);
    let level_tag = match entry.level {
        LogLevel::Fatal => "[FATAL] ",
        LogLevel::Error => "[ERROR] ",
        LogLevel::Warn => "[WARN]  ",
        LogLevel::Info => "[INFO]  ",
        LogLevel::Debug => "[DEBUG] ",
        LogLevel::Trace => "[TRACE] ",
        LogLevel::Unknown => "",
    };

    // Build base display text
    let msg = entry.message.as_deref().unwrap_or(&entry.raw);
    let base_text = if level_tag.is_empty() {
        entry.raw.clone()
    } else {
        format!("{}{}", level_tag, msg)
    };

    // Apply horizontal scroll
    let display_text = skip_chars(&base_text, h_scroll);

    if display_text.is_empty() {
        return Line::from(Span::raw(""));
    }

    let base_style = Style::default().fg(color);

    // Build spans — with or without inline highlighting
    let mut spans = if hl_patterns.is_empty() {
        // Fast path: no highlights
        if !level_tag.is_empty() && h_scroll < level_tag.len() {
            let tag_end = level_tag.len() - h_scroll;
            let tag_part = display_text[..tag_end.min(display_text.len())].to_string();
            let rest = if tag_end < display_text.len() {
                display_text[tag_end..].to_string()
            } else {
                String::new()
            };
            vec![
                Span::styled(
                    tag_part,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(rest, base_style),
            ]
        } else {
            vec![Span::styled(display_text.clone(), base_style)]
        }
    } else {
        // Highlight path: find all match ranges, split into spans
        apply_highlights(&display_text, base_style, hl_patterns)
    };

    // Append multiline indicator
    if !entry.extra_lines.is_empty() {
        spans.push(Span::styled(
            format!(" [+{} lines]", entry.extra_lines.len()),
            Style::default().fg(Color::DarkGray),
        ));
    }

    Line::from(spans)
}

/// Split text into spans at highlight match boundaries.
fn apply_highlights(
    text: &str,
    base_style: Style,
    patterns: &[(&Regex, Style)],
) -> Vec<Span<'static>> {
    // Collect all match ranges
    let mut ranges: Vec<(usize, usize, Style)> = Vec::new();
    for (re, style) in patterns {
        for m in re.find_iter(text) {
            ranges.push((m.start(), m.end(), *style));
        }
    }

    if ranges.is_empty() {
        return vec![Span::styled(text.to_string(), base_style)];
    }

    ranges.sort_by_key(|r| (r.0, std::cmp::Reverse(r.1)));

    // Remove overlapping ranges (first match wins)
    let mut filtered: Vec<(usize, usize, Style)> = Vec::new();
    for r in ranges {
        if filtered.last().is_none_or(|last| r.0 >= last.1) {
            filtered.push(r);
        }
    }

    let mut spans = Vec::new();
    let mut pos = 0;

    for (start, end, hl_style) in &filtered {
        if pos < *start {
            spans.push(Span::styled(text[pos..*start].to_string(), base_style));
        }
        spans.push(Span::styled(text[*start..*end].to_string(), *hl_style));
        pos = *end;
    }
    if pos < text.len() {
        spans.push(Span::styled(text[pos..].to_string(), base_style));
    }

    spans
}

fn level_color(level: LogLevel) -> Color {
    match level {
        LogLevel::Fatal => Color::Red,
        LogLevel::Error => Color::Red,
        LogLevel::Warn => Color::Yellow,
        LogLevel::Info => Color::Green,
        LogLevel::Debug => Color::Blue,
        LogLevel::Trace => Color::DarkGray,
        LogLevel::Unknown => Color::White,
    }
}

/// Skip first `n` chars, returning the remainder as an owned String.
fn skip_chars(s: &str, n: usize) -> String {
    if n == 0 {
        return s.to_string();
    }
    match s.char_indices().nth(n) {
        Some((byte_pos, _)) => s[byte_pos..].to_string(),
        None => String::new(),
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
