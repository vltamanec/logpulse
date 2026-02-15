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
  logpulse /var/log/syslog                              # Local file
  logpulse app.log nginx.log                             # Multiple files
  logpulse --format laravel app.log                      # Force parser
  docker logs -f myapp 2>&1 | logpulse                   # Pipe stdin
  logpulse docker myapi                                  # Smart match (Swarm/Compose)
  logpulse docker myapi /var/log/app.log                 # File inside container
  logpulse ssh user@host docker myapi                    # Remote Docker via SSH
  logpulse ssh user@host /var/log/app.log                # Remote file via SSH
  logpulse ssh user@host -J bastion.corp.com docker api  # Via jump host / proxy
  logpulse k8s my-pod -n staging                         # Kubernetes pod
  logpulse k8s -l app=api -n prod                        # K8s by label
  logpulse compose api                                   # Docker Compose service

\x1b[1mHotkeys:\x1b[0m
  q        Quit              Space    Pause / Resume
  /        Filter (regex)    ?        Search (n/N navigate)
  e        Error-only mode   *        Highlight pattern
  Enter    Detail view       y        Copy to clipboard
  c        Clear buffer      s        Save visible to file
  g        Jump to time      j/k ↑/↓  Navigate
  PgDn/PgUp  Jump 50 lines   Home/End  First/Last
  ←→       Horizontal scroll Ctrl+C   Force quit

\x1b[1mUpdate:\x1b[0m
  curl -fsSL https://raw.githubusercontent.com/vltamanec/logpulse/main/install.sh | sh")]
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
    /// Monitor Docker container (smart prefix match + auto-reconnect)
    #[command(
        after_help = "Smart matching: 'logpulse docker myapi' finds myapi.1.abc123 in Swarm.
Auto-reconnects when container restarts or redeploys."
    )]
    Docker {
        /// Container name or prefix (e.g. 'myapi' matches 'myapi.1.abc123')
        container: String,
        /// Path to log file inside the container (omit for stdout)
        file: Option<String>,
    },

    /// Monitor via SSH (remote files or remote Docker containers)
    #[command(after_help = "\x1b[1mExamples:\x1b[0m
  logpulse ssh user@host /var/log/app.log                # Remote file
  logpulse ssh user@host docker myapi                    # Remote container
  logpulse ssh user@host docker myapi /var/log/app.log   # File in remote container
  logpulse ssh prod-server docker myapi                  # Via ~/.ssh/config
  logpulse ssh user@host -J bastion.corp.com docker api  # Via jump host
  logpulse ssh user@host -p 2222 -i ~/.ssh/id_ed25519 /var/log/app.log

Also respects ~/.ssh/config for keys, ports, ProxyJump, ProxyCommand.")]
    Ssh {
        /// SSH target (user@host or host from ~/.ssh/config)
        target: String,
        /// SSH port (default: 22)
        #[arg(short = 'p', long)]
        port: Option<u16>,
        /// Path to private key
        #[arg(short = 'i', long)]
        key: Option<String>,
        /// Jump host (ProxyJump), e.g. bastion.corp.com
        #[arg(short = 'J', long)]
        jump: Option<String>,
        /// 'docker <prefix> [file]' or '/path/to/file.log'
        args: Vec<String>,
    },

    /// Monitor Kubernetes pod logs
    #[command(after_help = "\x1b[1mExamples:\x1b[0m
  logpulse k8s my-pod                                    # Pod stdout
  logpulse k8s my-pod -n staging                         # Specific namespace
  logpulse k8s my-pod -c sidecar                         # Specific container
  logpulse k8s -l app=api -n prod                        # Find pod by label
  logpulse k8s my-pod /var/log/app.log                   # File inside pod")]
    K8s {
        /// Pod name (omit if using --label)
        pod: Option<String>,
        /// Namespace
        #[arg(short, long, default_value = "default")]
        namespace: String,
        /// Container name (for multi-container pods)
        #[arg(short, long)]
        container: Option<String>,
        /// Label selector to find pod (e.g. 'app=api')
        #[arg(short, long)]
        label: Option<String>,
        /// Path to log file inside pod (omit for stdout)
        file: Option<String>,
    },

    /// Monitor Docker Compose service
    #[command(after_help = "\x1b[1mExamples:\x1b[0m
  logpulse compose api                                   # Service logs
  logpulse compose api -f docker-compose.prod.yml        # Custom compose file")]
    Compose {
        /// Service name
        service: String,
        /// Path to compose file
        #[arg(short, long)]
        file: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

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

    let (rx, name, history) = match cli.command {
        Some(Commands::Docker { container, file }) => {
            let (rx, name) = source::start_docker_source(container, file).await?;
            (rx, name, None)
        }
        Some(Commands::Ssh {
            target,
            port,
            key,
            jump,
            args,
        }) => {
            let opts = source::SshOpts {
                target,
                port,
                key,
                jump,
            };
            let (rx, name) = parse_ssh_args(opts, args).await?;
            (rx, name, None)
        }
        Some(Commands::K8s {
            pod,
            namespace,
            container,
            label,
            file,
        }) => {
            let (rx, name) =
                source::start_k8s_source(pod, namespace, container, label, file).await?;
            (rx, name, None)
        }
        Some(Commands::Compose { service, file }) => {
            let (rx, name) = source::start_compose_source(service, file).await?;
            (rx, name, None)
        }
        None => {
            let is_tty = atty::is(atty::Stream::Stdin);

            if cli.files.is_empty() && !is_tty {
                let (rx, name) = source::start_stdin_source().await?;
                (rx, name, None)
            } else if cli.files.is_empty() {
                eprintln!("Usage: logpulse <FILE>... | logpulse docker <NAME> | logpulse ssh ... | logpulse k8s ...");
                eprintln!("Try: logpulse --help");
                std::process::exit(1);
            } else if cli.files.len() == 1 && cli.files[0].to_string_lossy() == "-" {
                let (rx, name) = source::start_stdin_source().await?;
                (rx, name, None)
            } else {
                source::start_multi_file_source(cli.files).await?
            }
        }
    };

    run_tui(rx, name, format_name, history).await
}

/// Parse SSH subcommand args: ssh user@host docker myapi [file] OR ssh user@host /path/to/file
async fn parse_ssh_args(
    opts: source::SshOpts,
    args: Vec<String>,
) -> Result<(mpsc::UnboundedReceiver<String>, String), Box<dyn std::error::Error>> {
    if args.is_empty() {
        return Err("ssh requires additional arguments: docker <name> or /path/to/file".into());
    }

    if args[0] == "docker" {
        if args.len() < 2 {
            return Err("usage: logpulse ssh <target> docker <container> [file]".into());
        }
        let prefix = args[1].clone();
        let file = args.get(2).cloned();
        source::start_ssh_docker_source(opts, prefix, file).await
    } else {
        source::start_ssh_file_source(opts, args[0].clone()).await
    }
}

async fn run_tui(
    mut rx: mpsc::UnboundedReceiver<String>,
    name: String,
    format_override: Option<&str>,
    history: Option<source::FileHistory>,
) -> Result<(), Box<dyn std::error::Error>> {
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

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(name);
    app.history = history;

    for line in &initial_lines {
        let entry = detected_parser.parse(line);
        app.add_log(entry);
    }
    drop(initial_lines);

    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        if event::handle_events(&mut app)? {
            break;
        }

        if app.should_quit {
            break;
        }

        // Lazy history: load older lines when user scrolls to top
        if app.needs_history_load {
            app.needs_history_load = false;
            if let Some(ref mut hist) = app.history {
                let raw_lines = hist.load_older(app::HISTORY_CHUNK);
                let entries: Vec<_> = raw_lines
                    .iter()
                    .map(|line| detected_parser.parse(line))
                    .collect();
                app.prepend_logs(entries);
            }
        }

        // Drain available lines in batches to keep UI responsive.
        // When frozen, leave lines in the channel (don't lose them).
        if !app.frozen {
            let mut drained = 0;
            while let Ok(line) = rx.try_recv() {
                let entry = detected_parser.parse(&line);
                app.add_log(entry);
                drained += 1;
                if drained >= 5000 {
                    break;
                }
            }
        }

        app.tick_eps();
        app.clear_expired_status();
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
