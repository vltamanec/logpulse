use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use std::time::Duration;

use crate::app::{App, InputMode, ViewMode};

pub fn handle_events(app: &mut App) -> std::io::Result<bool> {
    if event::poll(Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            // Ctrl+C always quits
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                app.should_quit = true;
                return Ok(true);
            }

            match app.view_mode {
                ViewMode::Detail => match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        app.view_mode = ViewMode::Feed;
                    }
                    _ => {}
                },
                ViewMode::Feed => match app.input_mode {
                    InputMode::Filter => match key.code {
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                            app.filter_text.clear();
                            app.update_filter_regex();
                            app.clamp_selection();
                        }
                        KeyCode::Enter => {
                            app.input_mode = InputMode::Normal;
                            app.update_filter_regex();
                            app.clamp_selection();
                        }
                        KeyCode::Backspace => {
                            app.filter_text.pop();
                            app.update_filter_regex();
                        }
                        KeyCode::Char(c) => {
                            app.filter_text.push(c);
                            app.update_filter_regex();
                        }
                        _ => {}
                    },
                    InputMode::Search => match key.code {
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                            app.input_buffer.clear();
                            app.search_text.clear();
                            app.search_regex = None;
                        }
                        KeyCode::Enter => {
                            app.input_mode = InputMode::Normal;
                            app.search_text = app.input_buffer.clone();
                            app.input_buffer.clear();
                            app.update_search_regex();
                            app.search_next();
                        }
                        KeyCode::Backspace => {
                            app.input_buffer.pop();
                        }
                        KeyCode::Char(c) => {
                            app.input_buffer.push(c);
                        }
                        _ => {}
                    },
                    InputMode::Highlight => match key.code {
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                            app.input_buffer.clear();
                        }
                        KeyCode::Enter => {
                            app.input_mode = InputMode::Normal;
                            let pattern = app.input_buffer.clone();
                            app.input_buffer.clear();
                            if pattern.is_empty() {
                                app.highlights.clear();
                                app.set_status("Highlights cleared".to_string());
                            } else {
                                app.add_highlight(&pattern);
                                app.set_status(format!(
                                    "Highlight added ({} active)",
                                    app.highlights.len()
                                ));
                            }
                        }
                        KeyCode::Backspace => {
                            app.input_buffer.pop();
                        }
                        KeyCode::Char(c) => {
                            app.input_buffer.push(c);
                        }
                        _ => {}
                    },
                    InputMode::SavePrompt => match key.code {
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                            app.input_buffer.clear();
                        }
                        KeyCode::Enter => {
                            app.input_mode = InputMode::Normal;
                            let filename = app.input_buffer.clone();
                            app.input_buffer.clear();
                            if !filename.is_empty() {
                                match export_visible_logs(app, &filename) {
                                    Ok(count) => app.set_status(format!(
                                        "Saved {} entries to {}",
                                        count, filename
                                    )),
                                    Err(e) => app.set_status(format!("Save failed: {}", e)),
                                }
                            }
                        }
                        KeyCode::Backspace => {
                            app.input_buffer.pop();
                        }
                        KeyCode::Char(c) => {
                            app.input_buffer.push(c);
                        }
                        _ => {}
                    },
                    InputMode::TimeJump => match key.code {
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                            app.input_buffer.clear();
                        }
                        KeyCode::Enter => {
                            app.input_mode = InputMode::Normal;
                            let time_str = app.input_buffer.clone();
                            app.input_buffer.clear();
                            if !time_str.is_empty() {
                                app.jump_to_time(&time_str);
                                app.set_status(format!("Jumped to {}", time_str));
                            }
                        }
                        KeyCode::Backspace => {
                            app.input_buffer.pop();
                        }
                        KeyCode::Char(c) => {
                            app.input_buffer.push(c);
                        }
                        _ => {}
                    },
                    InputMode::Normal => match key.code {
                        KeyCode::Char('q') => {
                            app.should_quit = true;
                            return Ok(true);
                        }
                        KeyCode::Char(' ') => {
                            app.frozen = !app.frozen;
                        }
                        KeyCode::Char('/') => {
                            app.input_mode = InputMode::Filter;
                            app.filter_text.clear();
                        }
                        KeyCode::Char('e') => {
                            app.error_only = !app.error_only;
                            app.clamp_selection();
                        }
                        KeyCode::Enter => {
                            if app.visible_count() > 0 {
                                app.view_mode = ViewMode::Detail;
                            }
                        }
                        KeyCode::Char('c') => {
                            app.clear_logs();
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            app.scroll_up();
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            app.scroll_down();
                        }
                        KeyCode::Right | KeyCode::Char('l') => {
                            app.scroll_right();
                        }
                        KeyCode::Left | KeyCode::Char('h') => {
                            app.scroll_left();
                        }
                        KeyCode::PageDown => {
                            app.page_down(50);
                        }
                        KeyCode::PageUp => {
                            app.page_up(50);
                        }
                        KeyCode::Home => {
                            app.jump_to_start();
                        }
                        KeyCode::End => {
                            app.jump_to_end();
                        }
                        // Search
                        KeyCode::Char('?') => {
                            app.input_mode = InputMode::Search;
                            app.input_buffer.clear();
                        }
                        KeyCode::Char('n') => {
                            app.search_next();
                        }
                        KeyCode::Char('N') => {
                            app.search_prev();
                        }
                        // Copy to clipboard
                        KeyCode::Char('y') => {
                            let visible = app.visible_logs();
                            if let Some((_, entry)) = visible.get(app.selected_index) {
                                let mut text = entry.raw.clone();
                                for extra in &entry.extra_lines {
                                    text.push('\n');
                                    text.push_str(extra);
                                }
                                match copy_to_clipboard(&text) {
                                    Ok(()) => app.set_status("Copied to clipboard".to_string()),
                                    Err(e) => app.set_status(format!("Copy failed: {}", e)),
                                }
                            }
                        }
                        // Highlight
                        KeyCode::Char('*') => {
                            app.input_mode = InputMode::Highlight;
                            app.input_buffer.clear();
                        }
                        // Export / Save
                        KeyCode::Char('s') => {
                            app.input_mode = InputMode::SavePrompt;
                            app.input_buffer.clear();
                        }
                        // Time jump
                        KeyCode::Char('g') => {
                            app.input_mode = InputMode::TimeJump;
                            app.input_buffer.clear();
                        }
                        _ => {}
                    },
                },
            }
        }
    }
    Ok(false)
}

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    #[cfg(target_os = "macos")]
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "linux")]
    let mut child = {
        // Try xclip first, fall back to xsel
        let result = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        match result {
            Ok(c) => c,
            Err(_) => Command::new("xsel")
                .args(["--clipboard", "--input"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| e.to_string())?,
        }
    };

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Err("clipboard not supported on this OS".to_string());

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
        child.wait().map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn export_visible_logs(app: &App, filename: &str) -> Result<usize, String> {
    use std::io::Write;

    let visible = app.visible_logs();
    let mut file = std::fs::File::create(filename).map_err(|e| e.to_string())?;

    let mut count = 0;
    for (_, entry) in &visible {
        writeln!(file, "{}", entry.raw).map_err(|e| e.to_string())?;
        for extra in &entry.extra_lines {
            writeln!(file, "{}", extra).map_err(|e| e.to_string())?;
        }
        count += 1;
    }

    Ok(count)
}
