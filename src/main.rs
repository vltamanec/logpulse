mod app;
mod event;
mod parser;
mod source;
mod ui;

use std::io;
use std::path::PathBuf;

use clap::{Parser as ClapParser, Subcommand};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use app::App;
use parser::{detect_parser, LogParser, PlainParser};

#[derive(ClapParser)]
#[command(name = "logpulse")]
#[command(about = "High-performance TUI log analyzer with smart format detection")]
#[command(version)]
#[command(after_help = "\x1b[1mExamples:\x1b[0m
  logpulse /var/log/syslog                        # Monitor a local file
  logpulse /var/log/laravel.log                    # Auto-detects Laravel format
  docker logs -f myapp 2>&1 | logpulse -           # Pipe from Docker stdout
  logpulse docker myapp                            # Docker container stdout
  logpulse docker myapp /var/log/app.log           # File inside container

\x1b[1mHotkeys:\x1b[0m
  q        Quit
  Space    Pause / Resume
  /        Filter (type to search)
  e        Toggle error-only mode
  Enter    Detail view (JSON pretty-print)
  c        Clear buffer
  j/k ↑/↓  Navigate")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Log file to monitor (use "-" for stdin)
    file: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Monitor logs from a Docker container
    #[command(after_help = "\x1b[1mExamples:\x1b[0m
  logpulse docker my-laravel                       # Container stdout/stderr
  logpulse docker my-laravel /var/log/laravel.log  # Specific file inside
  logpulse docker my-go-app /app/logs/server.log   # Go app in container

Auto-detects bash/sh inside the container.")]
    Docker {
        /// Container name or ID
        container: String,

        /// Path to log file inside the container (omit to use docker logs)
        file: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let (rx, name) = match cli.command {
        Some(Commands::Docker { container, file }) => {
            source::start_docker_source(container, file).await?
        }
        None => {
            let file = cli.file.unwrap_or_else(|| {
                eprintln!(
                    "Usage: logpulse <FILE> or logpulse - (stdin) or logpulse docker <CONTAINER>"
                );
                std::process::exit(1);
            });

            if file.to_string_lossy() == "-" {
                source::start_stdin_source().await?
            } else {
                source::start_file_source(file).await?
            }
        }
    };

    run_tui(rx, name).await
}

async fn run_tui(
    mut rx: mpsc::UnboundedReceiver<String>,
    name: String,
) -> Result<(), Box<dyn std::error::Error>> {
    // Collect initial lines for parser detection
    let mut initial_lines: Vec<String> = Vec::new();
    // Drain whatever is already buffered (non-blocking)
    while let Ok(line) = rx.try_recv() {
        initial_lines.push(line);
    }

    let sample_refs: Vec<&str> = initial_lines.iter().map(|s| s.as_str()).take(20).collect();
    let detected_parser: Box<dyn LogParser> = if sample_refs.is_empty() {
        Box::new(PlainParser)
    } else {
        detect_parser(&sample_refs)
    };

    eprintln!("Detected format: {}", detected_parser.name());

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(name);

    // Parse initial lines
    for line in &initial_lines {
        let entry = detected_parser.parse(line);
        app.add_log(entry);
    }
    drop(initial_lines);

    let mut tick_interval = interval(Duration::from_millis(100));

    // Main loop
    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        if event::handle_events(&mut app)? {
            break;
        }

        if app.should_quit {
            break;
        }

        tokio::select! {
            Some(line) = rx.recv() => {
                if !app.frozen {
                    let entry = detected_parser.parse(&line);
                    app.add_log(entry);
                }
            }
            _ = tick_interval.tick() => {
                app.tick_eps();
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
