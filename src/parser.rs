use regex::Regex;
use std::sync::LazyLock;

use crate::app::{LogEntry, LogLevel};

pub trait LogParser: Send + Sync {
    fn name(&self) -> &str;
    fn can_parse(&self, line: &str) -> bool;
    fn parse(&self, line: &str) -> LogEntry;
}

/// Case-insensitive substring check without allocating a new String.
fn contains_ci(haystack: &str, needle: &str) -> bool {
    if needle.len() > haystack.len() {
        return false;
    }
    let needle_bytes = needle.as_bytes();
    haystack
        .as_bytes()
        .windows(needle_bytes.len())
        .any(|window| window.eq_ignore_ascii_case(needle_bytes))
}

fn detect_level(text: &str) -> LogLevel {
    if contains_ci(text, "FATAL") || contains_ci(text, "EMERGENCY") || contains_ci(text, "CRITICAL")
    {
        LogLevel::Fatal
    } else if contains_ci(text, "ERROR") || contains_ci(text, "ERR") {
        LogLevel::Error
    } else if contains_ci(text, "WARN") || contains_ci(text, "WARNING") {
        LogLevel::Warn
    } else if contains_ci(text, "INFO") {
        LogLevel::Info
    } else if contains_ci(text, "DEBUG") || contains_ci(text, "DBG") {
        LogLevel::Debug
    } else if contains_ci(text, "TRACE") {
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
            extra_lines: Vec::new(),
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
                extra_lines: Vec::new(),
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
                extra_lines: Vec::new(),
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
                extra_lines: Vec::new(),
            };
        }
        if let Some(caps) = GO_STD_RE.captures(line) {
            return LogEntry {
                raw: line.to_string(),
                level: detect_level(&caps[2]),
                timestamp: Some(caps[1].to_string()),
                message: Some(caps[2].to_string()),
                metadata: None,
                extra_lines: Vec::new(),
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
                extra_lines: Vec::new(),
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
        extra_lines: Vec::new(),
    }
}

/// Get a parser by name (for --format flag).
pub fn get_parser_by_name(name: &str) -> Box<dyn LogParser> {
    match name.to_lowercase().as_str() {
        "json" => Box::new(JsonParser),
        "laravel" => Box::new(LaravelParser),
        "django" => Box::new(DjangoParser),
        "go" => Box::new(GoLogParser),
        "nginx" | "apache" => Box::new(NginxApacheParser),
        _ => Box::new(PlainParser),
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- JSON Parser ---
    #[test]
    fn json_can_parse() {
        let p = JsonParser;
        assert!(p.can_parse(r#"{"level":"error","msg":"fail"}"#));
        assert!(p.can_parse(r#"  {"key": "value"}  "#));
        assert!(!p.can_parse("not json at all"));
        assert!(!p.can_parse("{incomplete"));
    }

    #[test]
    fn json_parse_fields() {
        let p = JsonParser;
        let entry = p.parse(r#"{"level":"error","msg":"connection failed","service":"api"}"#);
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.message.as_deref(), Some("connection failed"));
    }

    #[test]
    fn json_parse_severity_alias() {
        let p = JsonParser;
        let entry = p.parse(r#"{"severity":"WARNING","text":"slow query"}"#);
        assert_eq!(entry.level, LogLevel::Warn);
        assert_eq!(entry.message.as_deref(), Some("slow query"));
    }

    // --- Laravel Parser ---
    #[test]
    fn laravel_can_parse() {
        let p = LaravelParser;
        assert!(p.can_parse("[2024-01-15 10:30:01] production.ERROR: Connection refused"));
        assert!(p.can_parse("[2024-12-31 23:59:59] local.INFO: ok"));
        assert!(!p.can_parse("not laravel"));
        assert!(!p.can_parse(r#"{"level":"error"}"#));
    }

    #[test]
    fn laravel_parse_fields() {
        let p = LaravelParser;
        let entry =
            p.parse("[2024-01-15 10:30:01] production.ERROR: Connection refused to database");
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.timestamp.as_deref(), Some("2024-01-15 10:30:01"));
        assert_eq!(
            entry.message.as_deref(),
            Some("Connection refused to database")
        );
    }

    // --- Django Parser ---
    #[test]
    fn django_can_parse() {
        let p = DjangoParser;
        assert!(p.can_parse("[15/Jan/2024 10:30:11] ERROR [django.request] Internal Server Error"));
        assert!(!p.can_parse("[2024-01-15 10:30:01] production.ERROR: msg"));
        assert!(!p.can_parse("plain text"));
    }

    #[test]
    fn django_parse_fields() {
        let p = DjangoParser;
        let entry = p.parse(
            "[15/Jan/2024 10:30:11] ERROR [django.request] Internal Server Error: /api/data",
        );
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.timestamp.as_deref(), Some("15/Jan/2024 10:30:11"));
        assert_eq!(
            entry.message.as_deref(),
            Some("Internal Server Error: /api/data")
        );
        assert_eq!(entry.metadata.as_deref(), Some("django.request"));
    }

    // --- Go Parser ---
    #[test]
    fn go_slog_can_parse() {
        let p = GoLogParser;
        assert!(p.can_parse("time=2024-01-15T10:30:09Z level=ERROR msg=\"panic recovered\""));
        assert!(p.can_parse("time=2024-01-15T10:30:10Z level=INFO msg=\"health check passed\""));
    }

    #[test]
    fn go_std_can_parse() {
        let p = GoLogParser;
        assert!(p.can_parse("2024/01/15 10:30:01 Starting server on :8080"));
        assert!(!p.can_parse("plain text without timestamp"));
    }

    #[test]
    fn go_slog_parse_fields() {
        let p = GoLogParser;
        let entry = p.parse("time=2024-01-15T10:30:09Z level=ERROR msg=\"panic recovered\"");
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(
            entry.timestamp.as_deref(),
            Some("time=2024-01-15T10:30:09Z")
        );
    }

    // --- Nginx/Apache Parser ---
    #[test]
    fn nginx_can_parse() {
        let p = NginxApacheParser;
        assert!(p.can_parse(
            r#"192.168.1.1 - - [15/Jan/2024:10:30:07 +0000] "GET /api/users HTTP/1.1" 200 1234"#
        ));
        assert!(!p.can_parse("not nginx"));
    }

    #[test]
    fn nginx_parse_status_levels() {
        let p = NginxApacheParser;

        let e200 = p.parse(r#"10.0.0.1 - - [15/Jan/2024:10:30:07 +0000] "GET / HTTP/1.1" 200 512"#);
        assert_eq!(e200.level, LogLevel::Info);

        let e404 =
            p.parse(r#"10.0.0.1 - - [15/Jan/2024:10:30:07 +0000] "GET /missing HTTP/1.1" 404 0"#);
        assert_eq!(e404.level, LogLevel::Warn);

        let e500 =
            p.parse(r#"10.0.0.1 - - [15/Jan/2024:10:30:07 +0000] "POST /api HTTP/1.1" 500 89"#);
        assert_eq!(e500.level, LogLevel::Error);
    }

    // --- Plain Parser ---
    #[test]
    fn plain_detects_levels() {
        let p = PlainParser;
        assert_eq!(p.parse("ERROR: something broke").level, LogLevel::Error);
        assert_eq!(p.parse("WARN: disk full").level, LogLevel::Warn);
        assert_eq!(p.parse("INFO: all good").level, LogLevel::Info);
        assert_eq!(p.parse("just text").level, LogLevel::Unknown);
    }

    // --- Auto-detection ---
    #[test]
    fn detect_parser_json() {
        let lines = vec![
            r#"{"level":"info","msg":"start"}"#,
            r#"{"level":"error","msg":"fail"}"#,
        ];
        let p = detect_parser(&lines);
        assert_eq!(p.name(), "JSON");
    }

    #[test]
    fn detect_parser_laravel() {
        let lines = vec![
            "[2024-01-15 10:30:01] production.ERROR: Connection refused",
            "[2024-01-15 10:30:02] production.INFO: Request ok",
        ];
        let p = detect_parser(&lines);
        assert_eq!(p.name(), "Laravel");
    }

    #[test]
    fn detect_parser_mixed_falls_back() {
        let lines = vec!["just plain text", "another line", "nothing special"];
        let p = detect_parser(&lines);
        assert_eq!(p.name(), "Plain");
    }

    #[test]
    fn detect_parser_empty() {
        let lines: Vec<&str> = vec![];
        let p = detect_parser(&lines);
        assert_eq!(p.name(), "Plain");
    }

    // --- get_parser_by_name ---
    #[test]
    fn get_parser_by_name_works() {
        assert_eq!(get_parser_by_name("json").name(), "JSON");
        assert_eq!(get_parser_by_name("laravel").name(), "Laravel");
        assert_eq!(get_parser_by_name("django").name(), "Django");
        assert_eq!(get_parser_by_name("go").name(), "Go");
        assert_eq!(get_parser_by_name("nginx").name(), "Nginx/Apache");
        assert_eq!(get_parser_by_name("unknown").name(), "Plain");
    }

    // --- Edge cases ---
    #[test]
    fn empty_line() {
        let p = PlainParser;
        let entry = p.parse("");
        assert_eq!(entry.level, LogLevel::Unknown);
        assert_eq!(entry.raw, "");
    }

    #[test]
    fn malformed_json() {
        let p = JsonParser;
        // can_parse returns false for incomplete JSON
        assert!(!p.can_parse("{incomplete"));
        // But if forced, parse still works
        let entry = p.parse("{incomplete");
        assert_eq!(entry.level, LogLevel::Unknown);
    }
}
