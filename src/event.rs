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
                        }
                        KeyCode::Enter => {
                            app.input_mode = InputMode::Normal;
                            app.update_filter_regex();
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
                            app.selected_index = 0;
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
                        _ => {}
                    },
                },
            }
        }
    }
    Ok(false)
}
