use ratatui::style::{Color, Modifier, Style};
use regex::Regex;
use std::sync::LazyLock;

/// Log level with associated color
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn style(self) -> Style {
        match self {
            LogLevel::Error => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            LogLevel::Warn => Style::default().fg(Color::Yellow),
            LogLevel::Info => Style::default().fg(Color::Green),
            LogLevel::Debug => Style::default().fg(Color::Cyan),
            LogLevel::Trace => Style::default().fg(Color::DarkGray),
        }
    }
}

// Compiled regexes for log level detection
static ERROR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(\[ERROR\]|ERROR[:\s]|level=error|"level"\s*:\s*"error")"#).unwrap()
});
static WARN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(\[WARN(ING)?\]|WARN(ING)?[:\s]|level=warn(ing)?|"level"\s*:\s*"warn(ing)?")"#).unwrap()
});
static INFO_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(\[INFO\]|INFO[:\s]|level=info|"level"\s*:\s*"info")"#).unwrap()
});
static DEBUG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(\[DEBUG\]|DEBUG[:\s]|level=debug|"level"\s*:\s*"debug")"#).unwrap()
});
static TRACE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(\[TRACE\]|TRACE[:\s]|level=trace|"level"\s*:\s*"trace")"#).unwrap()
});

/// Detect log level from a line
pub fn detect_level(line: &str) -> Option<LogLevel> {
    // Check in priority order (most severe first)
    if ERROR_RE.is_match(line) {
        Some(LogLevel::Error)
    } else if WARN_RE.is_match(line) {
        Some(LogLevel::Warn)
    } else if INFO_RE.is_match(line) {
        Some(LogLevel::Info)
    } else if DEBUG_RE.is_match(line) {
        Some(LogLevel::Debug)
    } else if TRACE_RE.is_match(line) {
        Some(LogLevel::Trace)
    } else {
        None
    }
}

// Timestamp detection regex
static TIMESTAMP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}",     // ISO 8601
        r"(\.\d+)?",                                        // optional fractional
        r"([Zz]|[+-]\d{2}:?\d{2})?",                       // optional timezone
        r"|[A-Z][a-z]{2}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2}",  // syslog format
        r"|\d{2}/[A-Z][a-z]{2}/\d{4}:\d{2}:\d{2}:\d{2}",  // Apache CLF
        r"|\d{10,13})"                                       // Unix epoch
    )).unwrap()
});

/// Find timestamp spans in a line
pub fn find_timestamps(line: &str) -> Vec<(usize, usize)> {
    TIMESTAMP_RE.find_iter(line)
        .map(|m| (m.start(), m.end()))
        .collect()
}

/// Check if a line looks like JSON
pub fn is_json_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('{') && trimmed.ends_with('}')
}

/// Pretty-format a JSON line. Returns None if it's not valid JSON.
pub fn pretty_json(line: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    serde_json::to_string_pretty(&v).ok()
}

/// Source colors for multi-file mode
pub fn source_color(idx: usize) -> Color {
    const COLORS: [Color; 8] = [
        Color::Green,
        Color::Cyan,
        Color::Magenta,
        Color::Yellow,
        Color::Blue,
        Color::LightRed,
        Color::LightGreen,
        Color::LightCyan,
    ];
    COLORS[idx % COLORS.len()]
}
