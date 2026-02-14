# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [0.1.0] - 2025-02-15

### Added
- Real-time log monitoring with TUI (ratatui + crossterm)
- Smart auto-detection for 6 log formats: JSON, Laravel, Django, Go (slog), Nginx/Apache, Plain text
- Manual format override with `--format` flag
- Multi-file monitoring: `logpulse app.log nginx.log`
- Docker integration: `logpulse docker <container> [file]`
- Stdin pipe support: `docker logs -f app | logpulse`
- Regex-powered filter mode (`/` key)
- Error-only mode (`e` key)
- JSON Explorer detail view (`Enter` key)
- Sparkline activity graph with EPS tracking
- Freeze/pause mode (`Space` key)
- Shell completions for bash, zsh, fish (`--completions`)
- One-liner installer for Linux/macOS
- Cross-platform release builds (Linux x86_64/ARM64, macOS x86_64/ARM64, Windows x86_64)
