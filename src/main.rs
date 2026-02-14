mod app;
mod event;
mod parser;
mod source;
mod ui;

use std::io;
use std::path::PathBuf;

use clap::{CommandFactory, Parser as ClapParser, Subcommand, ValueEnum};
use clap_complete::{generate, Shell};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use app::App;
use parser::{detect_parser, get_parser_by_name, LogParser, PlainParser};

#[derive(Debug, Clone, ValueEnum)]
enum FormatArg {
    Json,
    Laravel,
    Django,
    Go,
    Nginx,
    Plain,
    Auto,
}

#[derive(ClapParser)]
#[command(name = "logpulse")]
#[command(about = "High-performance TUI log analyzer with smart format detection")]
#[command(version)]
#[command(after_help = "\x1b[1mExamples:\x1b[0m
  logpulse /var/log/syslog                        # Monitor a local file
  logpulse app.log nginx.log                       # Monitor multiple files
  logpulse --format laravel app.log                # Force Laravel parser
  docker logs -f myapp 2>&1 | logpulse             # Pipe from Docker stdout
  logpulse docker myapp                            # Docker container stdout
  logpulse docker myapp /var/log/app.log           # File inside container

\x1b[1mHotkeys:\x1b[0m
  q        Quit
  Space    Pause / Resume
  /        Filter (regex)
  e        Toggle error-only mode
  Enter    Detail view (JSON pretty-print)
  c        Clear buffer
  j/k ↑/↓  Navigate")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Log files to monitor (auto-detects stdin when piped)
    files: Vec<PathBuf>,

    /// Force log format instead of auto-detection
    #[arg(short, long, value_enum, default_value = "auto")]
    format: FormatArg,

    /// Generate shell completions
    #[arg(long, value_enum)]
    completions: Option<Shell>,
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

    // Handle --completions
    if let Some(shell) = cli.completions {
        let mut cmd = Cli::command();
        generate(shell, &mut cmd, "logpulse", &mut io::stdout());
        return Ok(());
    }

    let format_name = match cli.format {
        FormatArg::Auto => None,
        FormatArg::Json => Some("json"),
        FormatArg::Laravel => Some("laravel"),
        FormatArg::Django => Some("django"),
        FormatArg::Go => Some("go"),
        FormatArg::Nginx => Some("nginx"),
        FormatArg::Plain => Some("plain"),
    };

    let (rx, name) = match cli.command {
        Some(Commands::Docker { container, file }) => {
            source::start_docker_source(container, file).await?
        }
        None => {
            let is_tty = atty::is(atty::Stream::Stdin);

            if cli.files.is_empty() && !is_tty {
                // Piped stdin: docker logs -f app | logpulse
                source::start_stdin_source().await?
            } else if cli.files.is_empty() {
                eprintln!(
                    "Usage: logpulse <FILE>... or pipe via stdin or logpulse docker <CONTAINER>"
                );
                eprintln!("Try: logpulse --help");
                std::process::exit(1);
            } else if cli.files.len() == 1 && cli.files[0].to_string_lossy() == "-" {
                source::start_stdin_source().await?
            } else {
                source::start_multi_file_source(cli.files).await?
            }
        }
    };

    run_tui(rx, name, format_name).await
}

async fn run_tui(
    mut rx: mpsc::UnboundedReceiver<String>,
    name: String,
    format_override: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Collect initial lines for parser detection
    let mut initial_lines: Vec<String> = Vec::new();
    while let Ok(line) = rx.try_recv() {
        initial_lines.push(line);
    }

    let detected_parser: Box<dyn LogParser> = if let Some(fmt) = format_override {
        get_parser_by_name(fmt)
    } else {
        let sample_refs: Vec<&str> = initial_lines.iter().map(|s| s.as_str()).take(20).collect();
        if sample_refs.is_empty() {
            Box::new(PlainParser)
        } else {
            detect_parser(&sample_refs)
        }
    };

    eprintln!("Format: {}", detected_parser.name());

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(name);

    for line in &initial_lines {
        let entry = detected_parser.parse(line);
        app.add_log(entry);
    }
    drop(initial_lines);

    let mut tick_interval = interval(Duration::from_millis(100));

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

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
