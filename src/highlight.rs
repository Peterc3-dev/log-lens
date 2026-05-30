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
    Regex::new(
        r#"(?i)(\[WARN(ING)?\]|WARN(ING)?[:\s]|level=warn(ing)?|"level"\s*:\s*"warn(ing)?")"#,
    )
    .unwrap()
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
        r"(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}", // ISO 8601
        r"(\.\d+)?",                                // optional fractional
        r"([Zz]|[+-]\d{2}:?\d{2})?",                // optional timezone
        r"|[A-Z][a-z]{2}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2}", // syslog format
        r"|\d{2}/[A-Z][a-z]{2}/\d{4}:\d{2}:\d{2}:\d{2}", // Apache CLF
        r"|\d{10,13})"                              // Unix epoch
    ))
    .unwrap()
});

/// Find timestamp spans in a line
pub fn find_timestamps(line: &str) -> Vec<(usize, usize)> {
    TIMESTAMP_RE
        .find_iter(line)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_level_bracket_forms() {
        assert_eq!(detect_level("[ERROR] boom"), Some(LogLevel::Error));
        assert_eq!(detect_level("[WARN] careful"), Some(LogLevel::Warn));
        assert_eq!(detect_level("[WARNING] careful"), Some(LogLevel::Warn));
        assert_eq!(detect_level("[INFO] hello"), Some(LogLevel::Info));
        assert_eq!(detect_level("[DEBUG] details"), Some(LogLevel::Debug));
        assert_eq!(detect_level("[TRACE] noise"), Some(LogLevel::Trace));
    }

    #[test]
    fn detect_level_keyvalue_and_json_forms() {
        assert_eq!(
            detect_level("ts=1 level=error msg=x"),
            Some(LogLevel::Error)
        );
        assert_eq!(
            detect_level(r#"{"level":"warning","msg":"x"}"#),
            Some(LogLevel::Warn)
        );
        assert_eq!(detect_level("INFO: starting up"), Some(LogLevel::Info));
    }

    #[test]
    fn detect_level_is_case_insensitive() {
        assert_eq!(detect_level("[error] boom"), Some(LogLevel::Error));
        assert_eq!(detect_level("Level=Info ready"), Some(LogLevel::Info));
    }

    #[test]
    fn detect_level_none_for_plain_text() {
        assert_eq!(detect_level("just a plain line of text"), None);
        // "error" embedded in a word should not match the delimited patterns
        assert_eq!(detect_level("terrorist database loaded"), None);
    }

    #[test]
    fn detect_level_priority_error_over_others() {
        // A line mentioning multiple levels resolves to the most severe.
        assert_eq!(
            detect_level("[ERROR] failed after [INFO] retry"),
            Some(LogLevel::Error)
        );
    }

    #[test]
    fn find_timestamps_iso8601() {
        let line = "2026-05-30T12:34:56.789Z something happened";
        let spans = find_timestamps(line);
        assert_eq!(spans.len(), 1);
        let (s, e) = spans[0];
        assert_eq!(&line[s..e], "2026-05-30T12:34:56.789Z");
    }

    #[test]
    fn find_timestamps_space_separated() {
        let line = "2026-05-30 12:34:56 boot complete";
        let spans = find_timestamps(line);
        assert_eq!(spans.len(), 1);
        let (s, e) = spans[0];
        assert_eq!(&line[s..e], "2026-05-30 12:34:56");
    }

    #[test]
    fn find_timestamps_none_when_absent() {
        assert!(find_timestamps("no timestamp here at all").is_empty());
    }

    #[test]
    fn is_json_line_detection() {
        assert!(is_json_line(r#"{"a":1}"#));
        assert!(is_json_line("  { \"a\": 1 }  "));
        assert!(!is_json_line("plain text"));
        assert!(!is_json_line("{not closed"));
        assert!(!is_json_line("[1,2,3]")); // arrays are not treated as JSON object lines
    }

    #[test]
    fn pretty_json_valid_and_invalid() {
        let pretty = pretty_json(r#"{"b":2,"a":1}"#).expect("valid json");
        assert!(pretty.contains('\n'));
        assert!(pretty.contains("\"a\""));
        assert!(pretty_json("{not valid}").is_none());
    }

    #[test]
    fn source_color_wraps_around() {
        // Indices wrap modulo the palette length and stay stable per slot.
        assert_eq!(source_color(0), source_color(8));
        assert_eq!(source_color(3), source_color(11));
        assert_ne!(source_color(0), source_color(1));
    }
}
