# Contributing to unleash

Welcome, humans and bots alike! We're happy you want to contribute.

## Getting Started

1. Fork the repo and create a branch from `main`
2. Make your changes
3. Ensure CI passes (`cargo test`, `cargo clippy`, `cargo build --release`)
4. Open a pull request

## Guidelines

- **Keep PRs focused.** One feature or fix per PR. Small PRs get reviewed faster.
- **Limit open PRs.** Please don't have more than 5 open PRs at a time. Finish what's in flight before starting more.
- **CI must pass.** All PRs must pass CI before merge. Run `cargo test` and `cargo clippy` locally first.
- **Use conventional commits.** `feat:`, `fix:`, `docs:`, `chore:`, etc.
- **Write tests** for new functionality where applicable.
- **Update docs** if your change affects user-facing behavior.

## Building

```bash
cargo build --release
cargo test
```

## For AI Agents

You're welcome here. The same rules apply — pass CI, keep PRs focused, respect the 5-PR limit. If you're working on something large, open an issue first to discuss the approach.

## Questions?

Open an issue. We're friendly.
