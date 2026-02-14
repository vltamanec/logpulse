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
  Works with local files, stdin, Docker, SSH, Kubernetes, and Compose.
</p>

---

## Install

**One-liner** (Linux / macOS):

```sh
curl -fsSL https://raw.githubusercontent.com/vltamanec/logpulse/main/install.sh | sh
```

**Update** to the latest version — the same command:

```sh
curl -fsSL https://raw.githubusercontent.com/vltamanec/logpulse/main/install.sh | sh
```

**Cargo** (from source):

```sh
cargo install logpulse

# Update
cargo install logpulse --force
```

**Manual download**: grab a binary from [Releases](https://github.com/vltamanec/logpulse/releases/latest).

| Platform | Binary |
|----------|--------|
| Linux x86_64 | `logpulse-vX.X.X-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `logpulse-vX.X.X-aarch64-unknown-linux-gnu.tar.gz` |
| macOS x86_64 | `logpulse-vX.X.X-x86_64-apple-darwin.tar.gz` |
| macOS ARM64 (Apple Silicon) | `logpulse-vX.X.X-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `logpulse-vX.X.X-x86_64-pc-windows-msvc.zip` |

**Shell completions:**

```sh
# Bash
logpulse --completions bash > ~/.local/share/bash-completion/completions/logpulse

# Zsh
logpulse --completions zsh > ~/.zfunc/_logpulse

# Fish
logpulse --completions fish > ~/.config/fish/completions/logpulse.fish
```

## Quick Start

```sh
# Local log file
logpulse /var/log/syslog

# Multiple files at once
logpulse app.log nginx.log error.log

# Force a specific parser
logpulse --format laravel storage/logs/laravel.log

# Pipe from Docker (auto-detects stdin)
docker logs -f my-app 2>&1 | logpulse
```

## Remote Sources

### Docker (smart prefix match + auto-reconnect)

```sh
# Container stdout — matches myapi.1.abc123 in Swarm
logpulse docker myapi

# Log file inside container
logpulse docker myapi /var/log/app.log
```

Finds containers by name prefix (`docker ps --filter name=<prefix>`). Works with Docker Swarm and Compose — no manager access needed. Auto-reconnects when a container restarts or redeploys (tries for 5 minutes).

### SSH (remote files & remote Docker)

```sh
# Remote log file
logpulse ssh user@host /var/log/app.log

# Remote Docker container (same smart matching)
logpulse ssh user@host docker myapi

# Log file inside remote container
logpulse ssh user@host docker myapi /var/log/app.log
```

**Proxy / bastion / jump host** — pass directly, no config needed:

```sh
# Via jump host (corporate proxy, bastion)
logpulse ssh user@host -J bastion.corp.com docker myapi

# Custom port + key
logpulse ssh user@host -p 2222 -i ~/.ssh/id_ed25519 /var/log/app.log

# All together
logpulse ssh deploy@10.0.1.50 -J bastion.corp.com -p 2222 docker myapi
```

Also respects `~/.ssh/config` for keys, ports, `ProxyJump`, `ProxyCommand` — use whichever is more convenient.

### Kubernetes

```sh
# Pod stdout
logpulse k8s my-pod

# Specific namespace
logpulse k8s my-pod -n staging

# Specific container in multi-container pod
logpulse k8s my-pod -c sidecar

# Find pod by label
logpulse k8s -l app=api -n prod

# Log file inside pod
logpulse k8s my-pod /var/log/app.log
```

### Docker Compose

```sh
# Service logs
logpulse compose api

# Custom compose file
logpulse compose api -f docker-compose.prod.yml
```

## Hotkeys

| Key | Action |
|-----|--------|
| `q` | Quit |
| `Space` | Pause / Resume (freeze mode) |
| `/` | Filter — regex search, Enter to apply, Esc to cancel |
| `e` | Toggle error-only mode |
| `Enter` | Detail view (JSON pretty-print / stacktrace) |
| `c` | Clear screen buffer |
| `j` / `k` or `Up` / `Down` | Navigate log lines |
| `Esc` | Close detail view / cancel filter |
| `Ctrl+C` | Force quit |

## Supported Formats

LogPulse auto-detects the log format from the first lines. No configuration needed.
Use `--format` to override: `logpulse --format nginx access.log`

| Format | Flag | Example |
|--------|------|---------|
| **JSON** | `--format json` | `{"level":"error","msg":"failed","service":"api"}` |
| **Laravel** | `--format laravel` | `[2024-01-15 10:30:01] production.ERROR: Connection refused` |
| **Django** | `--format django` | `[15/Jan/2024 10:30:11] ERROR [django.request] Internal Server Error` |
| **Go (slog)** | `--format go` | `time=2024-01-15T10:30:09Z level=ERROR msg="panic recovered"` |
| **Nginx/Apache** | `--format nginx` | `192.168.1.1 - - [15/Jan/2024:10:30:07] "GET /api" 500 89` |
| **Plain text** | `--format plain` | Anything else — level detected by keywords |

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

- **Local / stdin mode**: just the binary
- **Docker mode**: `docker` CLI available and running
- **SSH mode**: `ssh` CLI with key-based auth configured
- **Kubernetes mode**: `kubectl` with cluster access configured
- **Compose mode**: `docker compose` (v2) available

Binary is ~2.7 MB, statically optimized. No runtime dependencies.

## Building from Source

```sh
git clone https://github.com/vltamanec/logpulse.git
cd logpulse
cargo build --release
cargo test
# Binary at ./target/release/logpulse
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

[MIT](LICENSE)
