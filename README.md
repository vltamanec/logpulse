<h1 align="center">LogPulse</h1>

<p align="center">
  <b>High-performance TUI log analyzer with smart format detection</b>
</p>

<p align="center">
  <a href="https://github.com/vltamanec/logpulse/releases/latest"><img src="https://img.shields.io/github/v/release/vltamanec/logpulse?style=flat-square&color=blue" alt="Release"></a>
  <a href="https://github.com/vltamanec/logpulse/actions"><img src="https://img.shields.io/github/actions/workflow/status/vltamanec/logpulse/ci.yml?style=flat-square" alt="CI"></a>
  <a href="https://crates.io/crates/logpulse"><img src="https://img.shields.io/crates/v/logpulse?style=flat-square&color=orange" alt="crates.io"></a>
  <img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="License">
</p>

<p align="center">
  A <code>tail -f</code> replacement that actually understands your logs.<br>
  Zero config. Auto-detects Laravel, Django, Go, Nginx, JSON.<br>
  Works with local files, stdin, and Docker containers.
</p>

---

## Install

**One-liner** (Linux / macOS):

```sh
curl -fsSL https://raw.githubusercontent.com/vltamanec/logpulse/main/install.sh | sh
```

**Cargo** (from source):

```sh
cargo install logpulse
```

**Manual download**: grab a binary from [Releases](https://github.com/vltamanec/logpulse/releases/latest).

| Platform | Binary |
|----------|--------|
| Linux x86_64 | `logpulse-vX.X.X-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `logpulse-vX.X.X-aarch64-unknown-linux-gnu.tar.gz` |
| macOS x86_64 | `logpulse-vX.X.X-x86_64-apple-darwin.tar.gz` |
| macOS ARM64 (Apple Silicon) | `logpulse-vX.X.X-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `logpulse-vX.X.X-x86_64-pc-windows-msvc.zip` |

## Quick Start

```sh
# Local log file
logpulse /var/log/syslog

# Pipe from Docker (stdout)
docker logs -f my-app 2>&1 | logpulse -

# Docker container — log file inside
logpulse docker my-app /var/log/app.log

# Docker container — stdout
logpulse docker my-app
```

## Hotkeys

| Key | Action |
|-----|--------|
| `q` | Quit |
| `Space` | Pause / Resume (freeze mode) |
| `/` | Filter — type to search, Enter to apply, Esc to cancel |
| `e` | Toggle error-only mode |
| `Enter` | Detail view (JSON pretty-print / stacktrace) |
| `c` | Clear screen buffer |
| `j` / `k` or `Up` / `Down` | Navigate log lines |
| `Esc` | Close detail view / cancel filter |
| `Ctrl+C` | Force quit |

## Supported Formats

LogPulse auto-detects the log format from the first lines. No configuration needed.

| Format | Example |
|--------|---------|
| **JSON** | `{"level":"error","msg":"failed","service":"api"}` |
| **Laravel** | `[2024-01-15 10:30:01] production.ERROR: Connection refused` |
| **Django** | `[15/Jan/2024 10:30:11] ERROR [django.request] Internal Server Error` |
| **Go (slog)** | `time=2024-01-15T10:30:09Z level=ERROR msg="panic recovered"` |
| **Nginx/Apache** | `192.168.1.1 - - [15/Jan/2024:10:30:07] "GET /api" 500 89` |
| **Plain text** | Anything else — level detected by keywords |

## How It Works

```
┌─ LogPulse ──────────────────────────┬─ Activity ─────────────────┐
│ app.log | EPS: 42 | Errors: 3      │ ▁▂▃▅▇▅▃▂▁▂▃▅▇█▇▅▃▂▁      │
├─ Log Feed ──────────────────────────┴────────────────────────────┤
│ [ERROR] Connection refused to database                           │
│ [INFO]  Request processed successfully                           │
│ [WARN]  Slow query detected (2.5s)                              │
│ [DEBUG] Cache hit for key=user:123                               │
│ [ERROR] Undefined variable: $user                                │
├─ Help ──────────────────────────────────────────────────────────┤
│ q:quit Space:pause /:filter e:errors Enter:detail c:clear ↑↓:nav│
└─────────────────────────────────────────────────────────────────┘
```

## Requirements

- **Local mode**: just the binary
- **Docker mode**: `docker` CLI available and running
- **Stdin mode**: pipe anything in

Binary is ~2.7 MB, statically optimized. No runtime dependencies.

## Building from Source

```sh
git clone https://github.com/vltamanec/logpulse.git
cd logpulse
cargo build --release
# Binary at ./target/release/logpulse
```

## License

MIT
