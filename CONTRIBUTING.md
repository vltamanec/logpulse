# Contributing to LogPulse

Thanks for your interest in contributing!

## Quick Start

```bash
git clone https://github.com/vltamanec/logpulse.git
cd logpulse
cargo build
cargo test
```

## Development

- **Rust edition**: 2021
- **Formatter**: `cargo fmt` (required — CI checks it)
- **Linter**: `cargo clippy -- -D warnings` (required — CI checks it)
- **Tests**: `cargo test` (required before PR)

## Adding a New Log Parser

1. Create a struct implementing `LogParser` trait in `src/parser.rs`
2. Add a static `LazyLock<Regex>` for your format pattern
3. Register it in `detect_parser()` and the `--format` CLI flag
4. Add unit tests in the `#[cfg(test)]` module
5. Update `README.md` supported formats table

## Pull Request Process

1. Fork the repo and create your branch from `main`
2. Make your changes with tests
3. Run `cargo fmt && cargo clippy && cargo test`
4. Open a PR with a clear description of what and why

## Reporting Bugs

Open an issue with:
- LogPulse version (`logpulse --version`)
- OS and terminal
- Sample log line that causes the issue (sanitized)
- Expected vs actual behavior

## Code Style

- Keep it simple — no over-engineering
- Follow existing patterns in the codebase
- Prefer `&str` over `String` where possible
- Use `LazyLock<Regex>` for compiled regex patterns
