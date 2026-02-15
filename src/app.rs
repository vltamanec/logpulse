use std::collections::VecDeque;
use std::time::{Duration, Instant};

use regex::Regex;

use crate::source::FileHistory;

pub const MAX_LOG_LINES: usize = 10_000;
pub const HISTORY_CHUNK: usize = 500;
const EPS_WINDOW_SECS: usize = 60;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Filter,
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
        }
    }

    pub fn add_log(&mut self, entry: LogEntry) {
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
            // Try regex first, fall back to literal escaped pattern
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
            if !re.is_match(&entry.raw) {
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

    /// Count visible entries without allocating a Vec.
    pub fn visible_count(&self) -> usize {
        self.logs
            .iter()
            .filter(|entry| self.matches_filter(entry))
            .count()
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
            // Signal the main loop to load older lines with the proper parser
            self.needs_history_load = true;
        }
    }

    pub fn scroll_right(&mut self) {
        self.horizontal_scroll += 20;
    }

    pub fn scroll_left(&mut self) {
        self.horizontal_scroll = self.horizontal_scroll.saturating_sub(20);
    }

    /// Prepend parsed log entries (older history) to the front of the buffer.
    /// Adjusts selected_index so the cursor stays on the same line.
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
}
