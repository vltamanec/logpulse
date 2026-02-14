use regex::Regex;
use std::sync::LazyLock;

use crate::app::{LogEntry, LogLevel};

pub trait LogParser: Send + Sync {
    fn name(&self) -> &str;
    fn can_parse(&self, line: &str) -> bool;
    fn parse(&self, line: &str) -> LogEntry;
}

fn detect_level(text: &str) -> LogLevel {
    let upper = text.to_uppercase();
    if upper.contains("FATAL") || upper.contains("EMERGENCY") || upper.contains("CRITICAL") {
        LogLevel::Fatal
    } else if upper.contains("ERROR") || upper.contains("ERR") {
        LogLevel::Error
    } else if upper.contains("WARN") || upper.contains("WARNING") {
        LogLevel::Warn
    } else if upper.contains("INFO") {
        LogLevel::Info
    } else if upper.contains("DEBUG") || upper.contains("DBG") {
        LogLevel::Debug
    } else if upper.contains("TRACE") {
        LogLevel::Trace
    } else {
        LogLevel::Unknown
    }
}

// --- Generic JSON Parser ---
pub struct JsonParser;

static JSON_LEVEL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""(?:level|severity|lvl)"\s*:\s*"([^"]+)""#).unwrap());
static JSON_MSG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""(?:msg|message|text)"\s*:\s*"([^"]+)""#).unwrap());

impl LogParser for JsonParser {
    fn name(&self) -> &str {
        "JSON"
    }

    fn can_parse(&self, line: &str) -> bool {
        let trimmed = line.trim();
        trimmed.starts_with('{') && trimmed.ends_with('}')
    }

    fn parse(&self, line: &str) -> LogEntry {
        let level = JSON_LEVEL_RE
            .captures(line)
            .map(|c| detect_level(&c[1]))
            .unwrap_or_else(|| detect_level(line));

        let message = JSON_MSG_RE.captures(line).map(|c| c[1].to_string());

        LogEntry {
            raw: line.to_string(),
            level,
            timestamp: None,
            message,
            metadata: Some(line.to_string()),
        }
    }
}

// --- Laravel Parser ---
// Format: [YYYY-MM-DD HH:MM:SS] env.LEVEL: message
pub struct LaravelParser;

static LARAVEL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\[(\d{4}-\d{2}-\d{2}\s\d{2}:\d{2}:\d{2})\]\s+\w+\.(\w+):\s+(.*)$").unwrap()
});

impl LogParser for LaravelParser {
    fn name(&self) -> &str {
        "Laravel"
    }

    fn can_parse(&self, line: &str) -> bool {
        LARAVEL_RE.is_match(line)
    }

    fn parse(&self, line: &str) -> LogEntry {
        if let Some(caps) = LARAVEL_RE.captures(line) {
            LogEntry {
                raw: line.to_string(),
                level: detect_level(&caps[2]),
                timestamp: Some(caps[1].to_string()),
                message: Some(caps[3].to_string()),
                metadata: None,
            }
        } else {
            fallback_parse(line)
        }
    }
}

// --- Django Parser ---
// Format: [DD/Month/YYYY HH:MM:SS] LEVEL [logger] message
pub struct DjangoParser;

static DJANGO_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\[(\d{2}/\w+/\d{4}\s\d{2}:\d{2}:\d{2})\]\s+(\w+)\s+\[([^\]]+)\]\s+(.*)$").unwrap()
});

impl LogParser for DjangoParser {
    fn name(&self) -> &str {
        "Django"
    }

    fn can_parse(&self, line: &str) -> bool {
        DJANGO_RE.is_match(line)
    }

    fn parse(&self, line: &str) -> LogEntry {
        if let Some(caps) = DJANGO_RE.captures(line) {
            LogEntry {
                raw: line.to_string(),
                level: detect_level(&caps[2]),
                timestamp: Some(caps[1].to_string()),
                message: Some(caps[4].to_string()),
                metadata: Some(caps[3].to_string()),
            }
        } else {
            fallback_parse(line)
        }
    }
}

// --- Go Common Log Parser ---
// Supports slog-style and standard log package output
pub struct GoLogParser;

static GO_SLOG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(time=\S+)\s+level=(\w+)\s+(?:source=\S+\s+)?msg=(.*)$").unwrap()
});
static GO_STD_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{4}/\d{2}/\d{2}\s\d{2}:\d{2}:\d{2})\s+(.*)$").unwrap());

impl LogParser for GoLogParser {
    fn name(&self) -> &str {
        "Go"
    }

    fn can_parse(&self, line: &str) -> bool {
        GO_SLOG_RE.is_match(line) || GO_STD_RE.is_match(line)
    }

    fn parse(&self, line: &str) -> LogEntry {
        if let Some(caps) = GO_SLOG_RE.captures(line) {
            return LogEntry {
                raw: line.to_string(),
                level: detect_level(&caps[2]),
                timestamp: Some(caps[1].to_string()),
                message: Some(caps[3].to_string()),
                metadata: None,
            };
        }
        if let Some(caps) = GO_STD_RE.captures(line) {
            return LogEntry {
                raw: line.to_string(),
                level: detect_level(&caps[2]),
                timestamp: Some(caps[1].to_string()),
                message: Some(caps[2].to_string()),
                metadata: None,
            };
        }
        fallback_parse(line)
    }
}

// --- Nginx/Apache Parser ---
// Combined log format: IP - - [timestamp] "METHOD /path HTTP/x.x" status size
pub struct NginxApacheParser;

static NGINX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^(\S+)\s+\S+\s+\S+\s+\[([^\]]+)\]\s+"([^"]+)"\s+(\d{3})\s+(\d+)"#).unwrap()
});

impl LogParser for NginxApacheParser {
    fn name(&self) -> &str {
        "Nginx/Apache"
    }

    fn can_parse(&self, line: &str) -> bool {
        NGINX_RE.is_match(line)
    }

    fn parse(&self, line: &str) -> LogEntry {
        if let Some(caps) = NGINX_RE.captures(line) {
            let status: u16 = caps[4].parse().unwrap_or(0);
            let level = match status {
                200..=299 => LogLevel::Info,
                300..=399 => LogLevel::Debug,
                400..=499 => LogLevel::Warn,
                500..=599 => LogLevel::Error,
                _ => LogLevel::Unknown,
            };
            LogEntry {
                raw: line.to_string(),
                level,
                timestamp: Some(caps[2].to_string()),
                message: Some(format!("{} -> {}", &caps[3], status)),
                metadata: Some(caps[1].to_string()),
            }
        } else {
            fallback_parse(line)
        }
    }
}

// --- Plain Fallback ---
pub struct PlainParser;

impl LogParser for PlainParser {
    fn name(&self) -> &str {
        "Plain"
    }

    fn can_parse(&self, _line: &str) -> bool {
        true
    }

    fn parse(&self, line: &str) -> LogEntry {
        fallback_parse(line)
    }
}

fn fallback_parse(line: &str) -> LogEntry {
    LogEntry {
        raw: line.to_string(),
        level: detect_level(line),
        timestamp: None,
        message: Some(line.to_string()),
        metadata: None,
    }
}

/// Auto-detect the best parser from a set of sample lines.
pub fn detect_parser(sample_lines: &[&str]) -> Box<dyn LogParser> {
    let parsers: Vec<Box<dyn LogParser>> = vec![
        Box::new(JsonParser),
        Box::new(LaravelParser),
        Box::new(DjangoParser),
        Box::new(GoLogParser),
        Box::new(NginxApacheParser),
    ];

    let mut best: Option<Box<dyn LogParser>> = None;
    let mut best_score = 0;

    for parser in parsers {
        let score = sample_lines.iter().filter(|l| parser.can_parse(l)).count();
        if score > best_score {
            best_score = score;
            best = Some(parser);
        }
    }

    if best_score > 0 {
        best.unwrap()
    } else {
        Box::new(PlainParser)
    }
}
