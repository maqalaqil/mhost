# Contributing to mhost

First off, thank you for considering contributing to mhost! Every contribution matters, whether it's a bug report, feature suggestion, documentation improvement, or code change.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Project Architecture](#project-architecture)
- [Making Changes](#making-changes)
- [Testing](#testing)
- [Pull Request Process](#pull-request-process)
- [Commit Messages](#commit-messages)
- [Issue Guidelines](#issue-guidelines)
- [Good First Issues](#good-first-issues)

## Code of Conduct

This project follows the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## Getting Started

1. **Fork** the repository on GitHub
2. **Clone** your fork locally
3. **Create a branch** for your changes
4. **Make changes** and add tests
5. **Submit a PR** against `main`

## Development Setup

### Prerequisites

- Rust 1.82+ (install via [rustup](https://rustup.rs))
- Git

### Build & Test

```bash
git clone https://github.com/YOUR_USERNAME/mhost
cd mhost

# Build all crates
cargo build

# Run all 774 tests
cargo test --workspace

# Run lints
cargo fmt --all --check
cargo clippy --workspace

# Build release binaries
cargo build --release

# Test the CLI
./target/release/mhost --version
./target/release/mhost --help
```

### Quick Test Cycle

```bash
# Test a specific crate
cargo test -p mhost-core

# Test with output
cargo test -p mhost-core -- --nocapture

# Run a specific test
cargo test -p mhost-core test_valid_transitions
```

## Project Architecture

mhost is a Rust workspace with **15 crates**. Each crate has one clear responsibility:

```
                ┌─────────┐
                │ mhost-cli│  ← CLI binary (user-facing)
                └────┬─────┘
                     │
    ┌────────────────┼────────────────┐
    │                │                │
┌───┴───┐    ┌──────┴──────┐   ┌────┴────┐
│mhost- │    │   mhost-    │   │ mhost-  │
│ tui   │    │   daemon    │   │  bot    │
└───────┘    └──────┬──────┘   └─────────┘
                    │
    ┌───────┬───────┼───────┬────────┐
    │       │       │       │        │
┌───┴──┐┌──┴──┐┌──┴──┐┌──┴───┐┌──┴───┐
│health││logs ││proxy││deploy││cloud │
└──────┘└─────┘└─────┘└──────┘└──────┘
    │       │       │       │        │
    └───────┴───────┴───┬───┴────────┘
                        │
          ┌─────────────┼─────────────┐
          │             │             │
    ┌─────┴──┐   ┌─────┴──┐   ┌─────┴──┐
    │ notify │   │ metrics│   │   ai   │
    └────────┘   └────────┘   └────────┘
          │             │             │
          └─────────────┼─────────────┘
                        │
                 ┌──────┴──────┐
                 │  mhost-core │  ← Shared types
                 │  mhost-config│  ← Config parsing
                 │  mhost-ipc  │  ← IPC transport
                 └─────────────┘
```

### Key Design Principles

- **One crate, one responsibility** — each crate does one thing well
- **Immutable data** — `ProcessInfo::transition_to()` returns a new struct, never mutates
- **Shared types in core** — `ProcessConfig`, `ProcessInfo`, `ProcessStatus` used everywhere
- **IPC via JSON-RPC** — CLI talks to daemon over Unix socket
- **Tests alongside code** — `#[cfg(test)] mod tests` in every source file

## Making Changes

### Adding a New Feature

1. **Types** — Add shared types to `mhost-core` if they're used across crates
2. **Config** — Add config parsing to `mhost-config/src/ecosystem.rs`
3. **Implementation** — Build in the appropriate crate
4. **Handler** — Wire into `mhost-daemon/src/handler.rs` if it needs daemon access
5. **CLI** — Add command in `mhost-cli/src/commands/` + `cli.rs` + `main.rs`
6. **Tests** — Add unit tests in the same file, integration tests in `tests/`
7. **Docs** — Update README.md

### Adding a Notification Channel

1. Create `crates/mhost-notify/src/channels/mychannel.rs`
2. Implement the `NotifyChannel` trait
3. Add to `channels/mod.rs`
4. Add config variant to `mhost-config`

### Adding a Cloud Provider

1. Create `crates/mhost-cloud/src/providers/myprovider.rs`
2. Implement the `CloudProvider` trait
3. Add to `providers/mod.rs` factory

## Testing

### Test Requirements

- **All PRs must pass `cargo test --workspace`**
- **New features must have tests** — aim for 80%+ coverage
- **Bug fixes should include a regression test**

### Test Organization

```
# Unit tests — inside source files
crates/mhost-core/src/process.rs   → #[cfg(test)] mod tests { ... }

# Integration tests
crates/mhost-cli/tests/cli_test.rs → E2E binary tests
```

### Writing Good Tests

```rust
#[test]
fn test_what_it_does_not_how() {
    // Arrange
    let config = ProcessConfig { name: "api".into(), ..Default::default() };

    // Act
    let info = ProcessInfo::new(config, 0);

    // Assert
    assert_eq!(info.status, ProcessStatus::Stopped);
}
```

## Pull Request Process

1. Ensure your branch is up to date with `main`
2. All CI checks must pass (fmt, clippy, tests)
3. Fill out the PR template completely
4. Request review from maintainers
5. Address any feedback
6. PRs are squash-merged into `main`

### PR Size Guidelines

- **Small** (< 100 lines): Quick review, merged fast
- **Medium** (100-500 lines): Detailed review
- **Large** (> 500 lines): Consider splitting into smaller PRs

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

Types: feat, fix, docs, test, chore, refactor, perf, ci
Scopes: core, config, ipc, logs, health, notify, metrics,
        proxy, deploy, ai, cloud, bot, tui, daemon, cli
```

### Examples

```
feat(notify): add LINE messaging channel
fix(daemon): handle graceful shutdown on SIGTERM
docs: add cloud fleet examples to README
test(proxy): add load balancing round-robin tests
chore: update tokio to 1.38
perf(logs): optimize FTS5 indexing batch size
ci: add ARM64 Linux build target
```

## Issue Guidelines

### Bug Reports

- Use the bug report template
- Include mhost version, OS, and installation method
- Provide steps to reproduce
- Paste relevant logs or error output
- Include your config file (remove secrets)

### Feature Requests

- Use the feature request template
- Or discuss ideas on [Discord #feature-requests](https://discord.gg/UKgZDUw9) first
- Explain the problem you're solving
- Show how you'd use it (CLI examples)
- Consider alternatives

## Good First Issues

Look for issues labeled [`good first issue`](https://github.com/maqalaqil/mhost/labels/good%20first%20issue). These are:

- Well-defined scope
- Clear acceptance criteria
- Helpful for learning the codebase
- Mentoring available

## Community

- **Discord** -- [discord.gg/UKgZDUw9](https://discord.gg/UKgZDUw9) -- chat, help, and discussions
- **GitHub Issues** -- bug reports and feature requests
- **GitHub Discussions** -- longer-form conversations

## License

By contributing to mhost, you agree that your contributions will be licensed under the [MIT License](LICENSE).
