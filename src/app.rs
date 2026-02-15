use std::collections::VecDeque;
use std::time::{Duration, Instant};

use ratatui::style::Color;
use regex::Regex;

use crate::source::FileHistory;

pub const MAX_LOG_LINES: usize = 10_000;
pub const HISTORY_CHUNK: usize = 500;
const EPS_WINDOW_SECS: usize = 60;

pub const HIGHLIGHT_COLORS: [Color; 4] = [
    Color::Magenta,
    Color::Cyan,
    Color::LightYellow,
    Color::LightRed,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub raw: String,
    pub level: LogLevel,
    pub timestamp: Option<String>,
    pub message: Option<String>,
    pub metadata: Option<String>,
    pub extra_lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Filter,
    Search,
    Highlight,
    SavePrompt,
    TimeJump,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Feed,
    Detail,
}

pub struct App {
    pub logs: VecDeque<LogEntry>,
    pub scroll_offset: usize,
    pub selected_index: usize,
    pub frozen: bool,
    pub error_only: bool,
    pub input_mode: InputMode,
    pub view_mode: ViewMode,
    pub filter_text: String,
    pub filter_regex: Option<Regex>,
    pub filename: String,
    pub error_count: u64,
    pub total_count: u64,
    pub eps_history: VecDeque<u64>,
    pub current_eps: u64,
    eps_counter: u64,
    eps_last_tick: Instant,
    pub should_quit: bool,
    pub history: Option<FileHistory>,
    pub needs_history_load: bool,
    pub horizontal_scroll: usize,
    // Multiline grouping
    pub has_structured_logs: bool,
    // Search (? key)
    pub search_text: String,
    pub search_regex: Option<Regex>,
    // Highlight (* key)
    pub highlights: Vec<(Regex, Color)>,
    // Shared input buffer for Search/Highlight/SavePrompt
    pub input_buffer: String,
    // Temporary status message ("Copied!", "Saved 42 entries to file.log")
    pub status_message: Option<(String, Instant)>,
}

impl App {
    pub fn new(filename: String) -> Self {
        Self {
            logs: VecDeque::with_capacity(MAX_LOG_LINES),
            scroll_offset: 0,
            selected_index: 0,
            frozen: false,
            error_only: false,
            input_mode: InputMode::Normal,
            view_mode: ViewMode::Feed,
            filter_text: String::new(),
            filter_regex: None,
            filename,
            error_count: 0,
            total_count: 0,
            eps_history: VecDeque::from(vec![0; EPS_WINDOW_SECS]),
            current_eps: 0,
            eps_counter: 0,
            eps_last_tick: Instant::now(),
            should_quit: false,
            history: None,
            needs_history_load: false,
            horizontal_scroll: 0,
            has_structured_logs: false,
            search_text: String::new(),
            search_regex: None,
            highlights: Vec::new(),
            input_buffer: String::new(),
            status_message: None,
        }
    }

    pub fn add_log(&mut self, entry: LogEntry) {
        if !matches!(entry.level, LogLevel::Unknown) {
            self.has_structured_logs = true;
        }

        // Multiline grouping: in structured logs (Laravel, JSON, Go, etc.),
        // any Unknown-level line after a known-level entry is a continuation
        // (stack trace, JSON body, PHP [stacktrace], etc.)
        if self.has_structured_logs && entry.level == LogLevel::Unknown {
            if let Some(last) = self.logs.back_mut() {
                if last.level != LogLevel::Unknown {
                    last.extra_lines.push(entry.raw);
                    return;
                }
            }
        }

        if matches!(entry.level, LogLevel::Error | LogLevel::Fatal) {
            self.error_count += 1;
        }
        self.total_count += 1;
        self.eps_counter += 1;

        if self.logs.len() >= MAX_LOG_LINES {
            self.logs.pop_front();
            if self.scroll_offset > 0 {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
        }
        self.logs.push_back(entry);
    }

    pub fn tick_eps(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.eps_last_tick) >= Duration::from_secs(1) {
            self.current_eps = self.eps_counter;
            self.eps_history.pop_front();
            self.eps_history.push_back(self.eps_counter);
            self.eps_counter = 0;
            self.eps_last_tick = now;
        }
    }

    pub fn update_filter_regex(&mut self) {
        self.filter_regex = if self.filter_text.is_empty() {
            None
        } else {
            Regex::new(&format!("(?i){}", &self.filter_text))
                .or_else(|_| Regex::new(&format!("(?i){}", regex::escape(&self.filter_text))))
                .ok()
        };
    }

    fn matches_filter(&self, entry: &LogEntry) -> bool {
        if self.error_only && !matches!(entry.level, LogLevel::Error | LogLevel::Fatal) {
            return false;
        }
        if let Some(ref re) = self.filter_regex {
            if !re.is_match(&entry.raw) && !entry.extra_lines.iter().any(|l| re.is_match(l)) {
                return false;
            }
        }
        true
    }

    pub fn visible_logs(&self) -> Vec<(usize, &LogEntry)> {
        self.logs
            .iter()
            .enumerate()
            .filter(|(_, entry)| self.matches_filter(entry))
            .collect()
    }

    pub fn visible_count(&self) -> usize {
        self.logs
            .iter()
            .filter(|entry| self.matches_filter(entry))
            .count()
    }

    pub fn clamp_selection(&mut self) {
        let count = self.visible_count();
        if count == 0 {
            self.selected_index = 0;
        } else if self.selected_index >= count {
            self.selected_index = count - 1;
        }
    }

    pub fn clear_logs(&mut self) {
        self.logs.clear();
        self.scroll_offset = 0;
        self.selected_index = 0;
    }

    pub fn scroll_down(&mut self) {
        let count = self.visible_count();
        if count > 0 && self.selected_index < count - 1 {
            self.selected_index += 1;
        }
    }

    pub fn scroll_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        } else if self.history.as_ref().is_some_and(|h| h.has_more()) {
            self.needs_history_load = true;
        }
    }

    pub fn page_down(&mut self, page_size: usize) {
        let count = self.visible_count();
        if count > 0 {
            self.selected_index = (self.selected_index + page_size).min(count - 1);
        }
    }

    pub fn page_up(&mut self, page_size: usize) {
        self.selected_index = self.selected_index.saturating_sub(page_size);
        if self.selected_index == 0 && self.history.as_ref().is_some_and(|h| h.has_more()) {
            self.needs_history_load = true;
        }
    }

    pub fn jump_to_start(&mut self) {
        self.selected_index = 0;
        if self.history.as_ref().is_some_and(|h| h.has_more()) {
            self.needs_history_load = true;
        }
    }

    pub fn jump_to_end(&mut self) {
        let count = self.visible_count();
        if count > 0 {
            self.selected_index = count - 1;
        }
    }

    pub fn scroll_right(&mut self) {
        self.horizontal_scroll += 20;
    }

    pub fn scroll_left(&mut self) {
        self.horizontal_scroll = self.horizontal_scroll.saturating_sub(20);
    }

    pub fn prepend_logs(&mut self, entries: Vec<LogEntry>) {
        let count = entries.len();
        if count == 0 {
            return;
        }
        for entry in entries.into_iter().rev() {
            self.logs.push_front(entry);
            if self.logs.len() > MAX_LOG_LINES {
                self.logs.pop_back();
            }
        }
        self.selected_index = count;
    }

    // --- Search ---

    pub fn update_search_regex(&mut self) {
        self.search_regex = if self.search_text.is_empty() {
            None
        } else {
            Regex::new(&format!("(?i){}", &self.search_text))
                .or_else(|_| Regex::new(&format!("(?i){}", regex::escape(&self.search_text))))
                .ok()
        };
    }

    pub fn search_next(&mut self) {
        if let Some(ref re) = self.search_regex {
            let visible = self.visible_logs();
            if visible.is_empty() {
                return;
            }
            let start = self.selected_index + 1;
            for i in 0..visible.len() {
                let idx = (start + i) % visible.len();
                if let Some((_, entry)) = visible.get(idx) {
                    if re.is_match(&entry.raw) || entry.extra_lines.iter().any(|l| re.is_match(l)) {
                        self.selected_index = idx;
                        return;
                    }
                }
            }
        }
    }

    pub fn search_prev(&mut self) {
        if let Some(ref re) = self.search_regex {
            let visible = self.visible_logs();
            if visible.is_empty() {
                return;
            }
            let start = if self.selected_index == 0 {
                visible.len() - 1
            } else {
                self.selected_index - 1
            };
            for i in 0..visible.len() {
                let idx = (start + visible.len() - i) % visible.len();
                if let Some((_, entry)) = visible.get(idx) {
                    if re.is_match(&entry.raw) || entry.extra_lines.iter().any(|l| re.is_match(l)) {
                        self.selected_index = idx;
                        return;
                    }
                }
            }
        }
    }

    // --- Highlights ---

    pub fn add_highlight(&mut self, pattern: &str) {
        if pattern.is_empty() {
            self.highlights.clear();
            return;
        }
        let color = HIGHLIGHT_COLORS[self.highlights.len() % HIGHLIGHT_COLORS.len()];
        if let Ok(re) = Regex::new(&format!("(?i){}", pattern))
            .or_else(|_| Regex::new(&format!("(?i){}", regex::escape(pattern))))
        {
            self.highlights.push((re, color));
        }
    }

    // --- Time jump ---

    pub fn jump_to_time(&mut self, time_str: &str) {
        let visible = self.visible_logs();
        for (idx, (_, entry)) in visible.iter().enumerate() {
            // Check parsed timestamp first
            if let Some(ref ts) = entry.timestamp {
                if ts.contains(time_str) {
                    self.selected_index = idx;
                    self.frozen = true; // pause to not lose position
                    return;
                }
            }
            // Fall back to searching raw line
            if entry.raw.contains(time_str) {
                self.selected_index = idx;
                self.frozen = true;
                return;
            }
        }
    }

    // --- Status messages ---

    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some((msg, Instant::now()));
    }

    pub fn clear_expired_status(&mut self) {
        if let Some((_, when)) = &self.status_message {
            if when.elapsed() > Duration::from_secs(3) {
                self.status_message = None;
            }
        }
    }
}
