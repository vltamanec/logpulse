use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub const MAX_LOG_LINES: usize = 10_000;
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
    pub filename: String,
    pub error_count: u64,
    pub total_count: u64,
    pub eps_history: Vec<u64>,
    pub current_eps: u64,
    eps_counter: u64,
    eps_last_tick: Instant,
    pub should_quit: bool,
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
            filename,
            error_count: 0,
            total_count: 0,
            eps_history: vec![0; EPS_WINDOW_SECS],
            current_eps: 0,
            eps_counter: 0,
            eps_last_tick: Instant::now(),
            should_quit: false,
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
            self.eps_history.remove(0);
            self.eps_history.push(self.eps_counter);
            self.eps_counter = 0;
            self.eps_last_tick = now;
        }
    }

    pub fn visible_logs(&self) -> Vec<(usize, &LogEntry)> {
        self.logs
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                if self.error_only && !matches!(entry.level, LogLevel::Error | LogLevel::Fatal) {
                    return false;
                }
                if !self.filter_text.is_empty()
                    && !entry
                        .raw
                        .to_lowercase()
                        .contains(&self.filter_text.to_lowercase())
                {
                    return false;
                }
                true
            })
            .collect()
    }

    pub fn clear_logs(&mut self) {
        self.logs.clear();
        self.scroll_offset = 0;
        self.selected_index = 0;
    }

    pub fn scroll_down(&mut self) {
        let visible = self.visible_logs().len();
        if visible > 0 && self.selected_index < visible - 1 {
            self.selected_index += 1;
        }
    }

    pub fn scroll_up(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }
}
