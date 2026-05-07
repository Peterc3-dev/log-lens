use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

use crate::app::{App, InputMode, PopupState};

/// Process input events. Returns Ok(true) if an event was processed.
pub fn handle_events(app: &mut App) -> std::io::Result<bool> {
    if event::poll(Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            match app.input_mode {
                InputMode::Normal => handle_normal_key(app, key),
                InputMode::FilterInput => handle_input_key(app, key, false),
                InputMode::InverseFilterInput => handle_input_key(app, key, true),
                InputMode::SearchInput => handle_search_key(app, key),
            }
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

fn handle_normal_key(app: &mut App, key: KeyEvent) {
    // Handle popup mode first
    if app.popup == PopupState::BookmarkList {
        handle_bookmark_popup_key(app, key);
        return;
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }

        // Scrolling
        KeyCode::Up | KeyCode::Char('k') => app.scroll_up(1),
        KeyCode::Down | KeyCode::Char('j') => app.scroll_down(1),
        KeyCode::PageUp => app.scroll_up(app.viewport_height),
        KeyCode::PageDown => app.scroll_down(app.viewport_height),
        KeyCode::Home | KeyCode::Char('g') => {
            app.scroll_offset = 0;
            app.auto_scroll = false;
        }
        KeyCode::End | KeyCode::Char('G') => {
            app.scroll_to_bottom();
        }

        // Toggle follow / auto-scroll
        KeyCode::Char('f') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.auto_scroll = !app.auto_scroll;
            if app.auto_scroll {
                app.scroll_to_bottom();
            }
        }

        // Toggle line wrap
        KeyCode::Char('w') => {
            app.line_wrap = !app.line_wrap;
        }

        // Filter input
        KeyCode::Char('/') => {
            app.input_mode = InputMode::FilterInput;
            app.input_buffer.clear();
        }
        KeyCode::Char('!') => {
            app.input_mode = InputMode::InverseFilterInput;
            app.input_buffer.clear();
        }

        // Pop last filter
        KeyCode::Char('d') => {
            app.pop_filter();
        }

        // Search
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.input_mode = InputMode::SearchInput;
            app.input_buffer.clear();
        }
        KeyCode::Char('n') => app.search_next(),
        KeyCode::Char('N') => app.search_prev(),

        // Bookmark
        KeyCode::Char('b') => app.toggle_bookmark(),
        KeyCode::Char('B') => {
            if !app.bookmarks.is_empty() {
                app.popup = PopupState::BookmarkList;
                app.bookmark_scroll = 0;
            }
        }

        // Copy current line to clipboard
        KeyCode::Char('y') => {
            if let Some(content) = app.current_line_content() {
                let _ = copy_to_clipboard(content);
            }
        }

        _ => {}
    }
}

fn handle_input_key(app: &mut App, key: KeyEvent, inverse: bool) {
    match key.code {
        KeyCode::Enter => {
            let pattern = app.input_buffer.clone();
            if !pattern.is_empty() {
                let _ = app.add_filter(pattern, inverse);
            }
            app.input_mode = InputMode::Normal;
            app.input_buffer.clear();
        }
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.input_buffer.clear();
        }
        KeyCode::Backspace => {
            app.input_buffer.pop();
        }
        KeyCode::Char(c) => {
            app.input_buffer.push(c);
        }
        _ => {}
    }
}

fn handle_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            let pattern = app.input_buffer.clone();
            let _ = app.set_search(&pattern);
            app.input_mode = InputMode::Normal;
            app.input_buffer.clear();
        }
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.input_buffer.clear();
        }
        KeyCode::Backspace => {
            app.input_buffer.pop();
        }
        KeyCode::Char(c) => {
            app.input_buffer.push(c);
        }
        _ => {}
    }
}

fn handle_bookmark_popup_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('B') | KeyCode::Char('q') => {
            app.popup = PopupState::None;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.bookmark_scroll = app.bookmark_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.bookmark_scroll + 1 < app.bookmarks.len() {
                app.bookmark_scroll += 1;
            }
        }
        KeyCode::Enter => {
            // Jump to selected bookmark
            if let Some(&line_idx) = app.bookmarks.get(app.bookmark_scroll) {
                // Find position in filtered indices
                if let Some(fi) = app.filtered_indices.iter().position(|&i| i == line_idx) {
                    app.scroll_offset = fi;
                    app.auto_scroll = false;
                }
            }
            app.popup = PopupState::None;
        }
        KeyCode::Char('d') => {
            // Delete selected bookmark
            if app.bookmark_scroll < app.bookmarks.len() {
                app.bookmarks.remove(app.bookmark_scroll);
                if app.bookmark_scroll > 0 && app.bookmark_scroll >= app.bookmarks.len() {
                    app.bookmark_scroll = app.bookmarks.len().saturating_sub(1);
                }
                if app.bookmarks.is_empty() {
                    app.popup = PopupState::None;
                }
            }
        }
        _ => {}
    }
}

/// Copy text to system clipboard using wl-copy (Wayland) or xclip (X11)
fn copy_to_clipboard(text: &str) -> Result<(), std::io::Error> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // Try wl-copy first (Wayland), then xclip (X11)
    let result = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    let mut child = match result {
        Ok(child) => child,
        Err(_) => {
            Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?
        }
    };

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(text.as_bytes())?;
    }
    child.wait()?;
    Ok(())
}
