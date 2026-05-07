use std::collections::VecDeque;
use regex::Regex;

/// Represents the source of a log line (which file it came from, or stdin)
#[derive(Clone, Debug)]
pub struct LogLine {
    pub content: String,
    pub source_idx: usize, // index into source names
}

/// A compiled filter with display string
#[derive(Clone)]
pub struct Filter {
    pub pattern: String,
    pub regex: Regex,
    pub inverse: bool, // true = hide matching, false = show only matching
}

/// Search state
pub struct SearchState {
    pub pattern: String,
    pub regex: Option<Regex>,
    pub current_match: Option<usize>, // index into filtered_indices
    pub match_indices: Vec<usize>,    // indices into filtered_indices that match
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            pattern: String::new(),
            regex: None,
            current_match: None,
            match_indices: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.pattern.clear();
        self.regex = None;
        self.current_match = None;
        self.match_indices.clear();
    }
}

/// Input mode for the TUI
#[derive(PartialEq, Clone, Debug)]
pub enum InputMode {
    Normal,
    FilterInput,       // typing a regex filter
    InverseFilterInput, // typing an inverse filter
    SearchInput,       // typing a search
}

/// Bookmark list popup state
#[derive(PartialEq, Clone, Debug)]
pub enum PopupState {
    None,
    BookmarkList,
}

/// Main application state
pub struct App {
    pub lines: VecDeque<LogLine>,
    pub buffer_size: usize,
    pub source_names: Vec<String>,

    // View state
    pub scroll_offset: usize,    // first visible line (in filtered view)
    pub auto_scroll: bool,       // tail -f mode
    pub line_wrap: bool,         // word wrap toggle
    pub viewport_height: usize,  // current terminal height for log area

    // Filtering
    pub filters: Vec<Filter>,
    pub filtered_indices: Vec<usize>, // indices into `lines` that pass all filters

    // Search
    pub search: SearchState,

    // Input
    pub input_mode: InputMode,
    pub input_buffer: String,

    // Bookmarks (indices into `lines`)
    pub bookmarks: Vec<usize>,

    // Popup
    pub popup: PopupState,
    pub bookmark_scroll: usize,

    // Follow mode (for file tailing)
    pub _follow_mode: bool,

    // Running
    pub should_quit: bool,

    // Needs refilter flag
    pub needs_refilter: bool,
}

impl App {
    pub fn new(buffer_size: usize, source_names: Vec<String>, follow_mode: bool) -> Self {
        Self {
            lines: VecDeque::with_capacity(buffer_size),
            buffer_size,
            source_names,
            scroll_offset: 0,
            auto_scroll: true,
            line_wrap: false,
            viewport_height: 24,
            filters: Vec::new(),
            filtered_indices: Vec::new(),
            search: SearchState::new(),
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            bookmarks: Vec::new(),
            popup: PopupState::None,
            bookmark_scroll: 0,
            _follow_mode: follow_mode,
            should_quit: false,
            needs_refilter: true,
        }
    }

    /// Add a log line to the ring buffer
    pub fn add_line(&mut self, content: String, source_idx: usize) {
        let line = LogLine {
            content,
            source_idx,
        };

        if self.lines.len() >= self.buffer_size {
            self.lines.pop_front();
            self.needs_refilter = true; // indices shifted
        }

        self.lines.push_back(line);

        // Check if this new line passes filters (fast path: avoid full refilter)
        if !self.needs_refilter {
            let idx = self.lines.len() - 1;
            if self.line_passes_filters(idx) {
                self.filtered_indices.push(idx);
            }
        }
    }

    /// Check if a line at the given index passes all active filters
    fn line_passes_filters(&self, idx: usize) -> bool {
        if let Some(line) = self.lines.get(idx) {
            for filter in &self.filters {
                let matches = filter.regex.is_match(&line.content);
                if filter.inverse && matches {
                    return false;
                }
                if !filter.inverse && !matches {
                    return false;
                }
            }
            true
        } else {
            false
        }
    }

    /// Rebuild filtered indices from scratch
    pub fn refilter(&mut self) {
        self.filtered_indices.clear();
        for i in 0..self.lines.len() {
            if self.line_passes_filters(i) {
                self.filtered_indices.push(i);
            }
        }
        self.needs_refilter = false;
        self.rebuild_search_matches();
    }

    /// Add a filter
    pub fn add_filter(&mut self, pattern: String, inverse: bool) -> Result<(), String> {
        let regex = Regex::new(&pattern).map_err(|e| e.to_string())?;
        self.filters.push(Filter {
            pattern,
            regex,
            inverse,
        });
        self.needs_refilter = true;
        Ok(())
    }

    /// Remove the last filter
    pub fn pop_filter(&mut self) {
        if self.filters.pop().is_some() {
            self.needs_refilter = true;
        }
    }

    /// Set search pattern
    pub fn set_search(&mut self, pattern: &str) -> Result<(), String> {
        if pattern.is_empty() {
            self.search.clear();
            return Ok(());
        }
        let regex = Regex::new(pattern).map_err(|e| e.to_string())?;
        self.search.pattern = pattern.to_string();
        self.search.regex = Some(regex);
        self.rebuild_search_matches();
        // Jump to first match at or after current scroll
        if !self.search.match_indices.is_empty() {
            let first = self.search.match_indices.iter()
                .position(|&i| i >= self.scroll_offset)
                .unwrap_or(0);
            self.search.current_match = Some(first);
            self.scroll_offset = self.search.match_indices[first];
            self.auto_scroll = false;
        }
        Ok(())
    }

    /// Rebuild search match indices based on current filtered view
    fn rebuild_search_matches(&mut self) {
        self.search.match_indices.clear();
        if let Some(ref regex) = self.search.regex {
            for (fi, &li) in self.filtered_indices.iter().enumerate() {
                if let Some(line) = self.lines.get(li) {
                    if regex.is_match(&line.content) {
                        self.search.match_indices.push(fi);
                    }
                }
            }
        }
        // Adjust current_match if it's out of bounds
        if let Some(cm) = self.search.current_match {
            if cm >= self.search.match_indices.len() {
                self.search.current_match = if self.search.match_indices.is_empty() {
                    None
                } else {
                    Some(self.search.match_indices.len() - 1)
                };
            }
        }
    }

    /// Next search match
    pub fn search_next(&mut self) {
        if self.search.match_indices.is_empty() {
            return;
        }
        let next = match self.search.current_match {
            Some(idx) => (idx + 1) % self.search.match_indices.len(),
            None => 0,
        };
        self.search.current_match = Some(next);
        self.scroll_offset = self.search.match_indices[next];
        self.auto_scroll = false;
    }

    /// Previous search match
    pub fn search_prev(&mut self) {
        if self.search.match_indices.is_empty() {
            return;
        }
        let prev = match self.search.current_match {
            Some(0) => self.search.match_indices.len() - 1,
            Some(idx) => idx - 1,
            None => self.search.match_indices.len() - 1,
        };
        self.search.current_match = Some(prev);
        self.scroll_offset = self.search.match_indices[prev];
        self.auto_scroll = false;
    }

    /// Toggle bookmark on the line currently at the top of viewport
    pub fn toggle_bookmark(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let fi = self.scroll_offset.min(self.filtered_indices.len().saturating_sub(1));
        let line_idx = self.filtered_indices[fi];
        if let Some(pos) = self.bookmarks.iter().position(|&b| b == line_idx) {
            self.bookmarks.remove(pos);
        } else {
            self.bookmarks.push(line_idx);
            self.bookmarks.sort();
        }
    }

    /// Get the content of the line at the current scroll position
    pub fn current_line_content(&self) -> Option<&str> {
        if self.filtered_indices.is_empty() {
            return None;
        }
        let fi = self.scroll_offset.min(self.filtered_indices.len().saturating_sub(1));
        let li = self.filtered_indices[fi];
        self.lines.get(li).map(|l| l.content.as_str())
    }

    /// Scroll up by n lines
    pub fn scroll_up(&mut self, n: usize) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    /// Scroll down by n lines
    pub fn scroll_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(n);
        let max = self.filtered_indices.len().saturating_sub(1);
        if self.scroll_offset >= max {
            self.scroll_offset = max;
        }
    }

    /// Scroll to bottom and enable auto-scroll
    pub fn scroll_to_bottom(&mut self) {
        self.auto_scroll = true;
        if !self.filtered_indices.is_empty() {
            self.scroll_offset = self.filtered_indices.len().saturating_sub(self.viewport_height);
        }
    }

    /// If auto-scroll is on, adjust scroll to show latest lines
    pub fn maybe_auto_scroll(&mut self) {
        if self.auto_scroll && !self.filtered_indices.is_empty() {
            let total = self.filtered_indices.len();
            if total > self.viewport_height {
                self.scroll_offset = total - self.viewport_height;
            } else {
                self.scroll_offset = 0;
            }
        }
    }

    /// Total filtered line count
    pub fn filtered_count(&self) -> usize {
        self.filtered_indices.len()
    }

    /// Total line count
    pub fn total_count(&self) -> usize {
        self.lines.len()
    }
}
