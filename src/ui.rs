use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Sparkline, Wrap},
    Frame,
};

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
    draw_log_feed(frame, app, &visible, chunks[1]);
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

fn draw_log_feed(frame: &mut Frame, app: &App, visible: &[(usize, &LogEntry)], area: Rect) {
    let total_visible = visible.len();

    // Calculate viewport BEFORE creating ListItems
    let height = area.height.saturating_sub(2) as usize; // borders
    let offset = if app.frozen || app.selected_index < total_visible.saturating_sub(height) {
        app.selected_index.saturating_sub(height / 2)
    } else {
        total_visible.saturating_sub(height)
    };

    // Only create ListItems for the visible window (offset..offset+height)
    let window_end = (offset + height + 1).min(total_visible);
    let items: Vec<ListItem> = visible[offset..window_end]
        .iter()
        .enumerate()
        .map(|(i, (_orig_idx, entry))| {
            let display_idx = offset + i;
            let line = colorize_entry(entry, app.horizontal_scroll);
            let style = if display_idx == app.selected_index {
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
    let content = match app.input_mode {
        InputMode::Filter => {
            let input_line = Line::from(vec![
                Span::styled(" Filter: ", Style::default().fg(Color::Yellow)),
                Span::raw(&app.filter_text),
                Span::styled("_", Style::default().fg(Color::Yellow)),
            ]);
            Paragraph::new(input_line)
        }
        InputMode::Normal => {
            let help = Line::from(vec![
                Span::styled(" q", Style::default().fg(Color::Cyan)),
                Span::raw(":quit "),
                Span::styled("Space", Style::default().fg(Color::Cyan)),
                Span::raw(":pause "),
                Span::styled("/", Style::default().fg(Color::Cyan)),
                Span::raw(":filter "),
                Span::styled("e", Style::default().fg(Color::Cyan)),
                Span::raw(":errors "),
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::raw(":detail "),
                Span::styled("c", Style::default().fg(Color::Cyan)),
                Span::raw(":clear "),
                Span::styled("↑↓", Style::default().fg(Color::Cyan)),
                Span::raw(":nav "),
                Span::styled("←→", Style::default().fg(Color::Cyan)),
                Span::raw(":scroll"),
            ]);
            Paragraph::new(help)
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(match app.input_mode {
            InputMode::Filter => " Filter Mode (Esc to cancel, Enter to apply) ",
            InputMode::Normal => " Help ",
        });

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
            Ok(val) => serde_json::to_string_pretty(&val).unwrap_or_else(|_| entry.raw.clone()),
            Err(_) => entry.raw.clone(),
        }
    } else {
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
        detail
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

fn colorize_entry(entry: &LogEntry, h_scroll: usize) -> Line<'_> {
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

    // For Unknown level, just show the raw line (no tag prefix)
    if level_tag.is_empty() {
        let scrolled = skip_chars(&entry.raw, h_scroll);
        return Line::from(Span::styled(scrolled, Style::default().fg(color)));
    }

    let msg = entry.message.as_deref().unwrap_or(&entry.raw);

    if h_scroll >= level_tag.len() {
        // Tag is fully scrolled off — show only the message part
        let msg_scroll = h_scroll - level_tag.len();
        let scrolled = skip_chars(msg, msg_scroll);
        Line::from(Span::styled(scrolled, Style::default().fg(color)))
    } else {
        // Tag is partially or fully visible
        let tag_part = &level_tag[h_scroll..]; // ASCII-safe: level_tag is pure ASCII
        let rest = msg.to_string();
        Line::from(vec![
            Span::styled(
                tag_part.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(rest),
        ])
    }
}

/// Skip first `n` chars, returning the remainder as an owned String.
fn skip_chars(s: &str, n: usize) -> String {
    if n == 0 {
        return s.to_string();
    }
    // Use char_indices for O(n) skip without creating intermediate iterator alloc
    match s.char_indices().nth(n) {
        Some((byte_pos, _)) => s[byte_pos..].to_string(),
        None => String::new(),
    }
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
