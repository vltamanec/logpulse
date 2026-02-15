# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [0.1.2] - 2026-02-15

### Added
- Multiline log grouping — stack traces (PHP, Java, Python, Go) auto-grouped with parent error entry, shown as `[+N lines]`
- Regex search (`?` key) with inline yellow highlighting, `n`/`N` navigation (wraps around)
- Pattern highlighting (`*` key) — up to 4 simultaneous colors, empty pattern clears all
- Copy to clipboard (`y` key) — copies selected entry + continuation lines via `pbcopy` / `xclip`
- Export to file (`s` key) — saves all visible (filtered) entries to a file
- Time jump (`g` key) — type a timestamp fragment to jump to that log entry, auto-pauses
- Page Up / Page Down — jump 50 lines at a time
- Home / End — jump to first / last log entry
- Status bar messages — temporary notifications for copy, save, highlight actions

### Fixed
- Pause mode now buffers incoming data (previously lines were lost while paused)
- `selected_index` out-of-bounds crash when filter reduces visible entries
- Drain batch limit (5000/tick) prevents UI freeze on log bursts

### Improved
- `VecDeque` for EPS history (was `Vec` with O(n) remove)
- Only visible window ListItems rendered (not all 10k entries)
- `visible_logs()` called once per frame (was 3+ times)
- Zero-alloc case-insensitive level detection in parser

## [0.1.1] - 2026-02-15

### Added
- SSH proxy flags: `-p` (port), `-i` (key), `-J` (jump host / bastion) — no ssh config required
- Update command shown in `--help` and after install
- Full tldr page with all commands (contrib/logpulse.md)

## [0.1.0] - 2026-02-15

### Added
- Real-time log monitoring with TUI (ratatui + crossterm)
- Smart auto-detection for 6 log formats: JSON, Laravel, Django, Go (slog), Nginx/Apache, Plain text
- Manual format override with `--format` flag
- Multi-file monitoring: `logpulse app.log nginx.log`
- Docker integration with smart prefix match + auto-reconnect (works with Swarm/Compose)
- SSH remote source: `logpulse ssh user@host /path` or `logpulse ssh user@host docker <name>`
- Kubernetes source: `logpulse k8s <pod>` with namespace, container, label selector support
- Docker Compose source: `logpulse compose <service>`
- Stdin pipe support: `docker logs -f app | logpulse`
- Regex-powered filter mode (`/` key)
- Error-only mode (`e` key)
- JSON Explorer detail view (`Enter` key)
- Sparkline activity graph with EPS tracking
- Freeze/pause mode (`Space` key)
- Shell completions for bash, zsh, fish (`--completions`)
- One-liner installer for Linux/macOS
- Cross-platform release builds (Linux x86_64/ARM64, macOS x86_64/ARM64, Windows x86_64)
