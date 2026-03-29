# Contributing to mhost

Thanks for your interest in contributing! Here's how to get started.

## Development Setup

```bash
# Clone the repo
git clone https://github.com/maheralaqil/mhost
cd mhost

# Build
cargo build

# Run tests
cargo test --workspace

# Run lints
cargo fmt --check
cargo clippy --workspace
```

## Project Structure

mhost is a Rust workspace with 15 crates:

```
crates/
├── mhost-core       Core types and protocol
├── mhost-config     Config parsing (TOML/YAML/JSON)
├── mhost-ipc        IPC layer (Unix socket / named pipe)
├── mhost-logs       Log capture, FTS5 search, sinks
├── mhost-health     Health probes (HTTP/TCP/script)
├── mhost-notify     Notification channels (8 channels)
├── mhost-metrics    Metrics, Prometheus, alerts
├── mhost-proxy      Reverse proxy, TLS, ACME
├── mhost-deploy     Deploy engine, git, rollback
├── mhost-ai         LLM intelligence (OpenAI/Claude)
├── mhost-cloud      Remote fleet management
├── mhost-bot        Chat bot (Telegram/Discord)
├── mhost-tui        Terminal dashboard
├── mhost-daemon     Daemon binary (mhostd)
└── mhost-cli        CLI binary (mhost)
```

## Pull Request Process

1. Fork the repo and create a feature branch
2. Write tests for your changes
3. Ensure `cargo test --workspace` passes
4. Ensure `cargo clippy --workspace` is clean
5. Submit a PR with a clear description

## Commit Messages

Follow conventional commits:

```
feat(core): add new process state
fix(cli): handle empty process list
docs: update README installation section
test(health): add TCP probe timeout test
chore: update dependencies
```

## Code Style

- Run `cargo fmt` before committing
- Keep files under 800 lines
- Write tests for all new functionality
- Handle errors explicitly (no `.unwrap()` in library code)

## Adding a New Feature

1. Add types to `mhost-core` if they're shared
2. Implement in the appropriate crate
3. Wire into `mhost-daemon` handler if it needs daemon access
4. Add CLI command in `mhost-cli`
5. Update README

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
