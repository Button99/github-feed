# GitHub Feed

A Rust CLI for reading received GitHub activity, with repository filtering and a local account-specific cache. Uses your existing GitHub CLI login.

## Run

Requires Rust/Cargo and `gh`:

```bash
gh auth login --hostname github.com
cargo run -- check
cargo run -- check --limit 20
cargo run -- check --repo rust-lang/rust --limit 10
cargo run -- check --refresh
```

Results retain GitHub's feed order. The default limit is 10. Repository filtering happens before limiting.

Activity is cached for five minutes under `${XDG_CACHE_HOME:-$HOME/.cache}/github-feed/`. Failed activity requests fall back to the previous saved snapshot, with a warning on stderr. Cached results retain their original save time. Account lookup uses GitHub CLI's 24-hour HTTP cache; offline fallback requires that lookup to succeed. Cache files can include private repository activity and are created with private permissions on Unix.

This reads the received-events API rather than every recommendation on GitHub's website. Available history is bounded, and activity can be delayed. The `publish` command is a placeholder and does not publish anything.

## Omarchy widget

See [omarchy-github-feed](https://github.com/Button99/omarchy-github-feed) for the separate desktop plugin.

## Checks

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CLI integration tests use a fake `gh` executable and require no GitHub credentials or network access. Those integration tests currently target Unix.
