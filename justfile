# Task runner (https://github.com/casey/just). Architecture §13.

default: test

# Build the whole workspace
build:
    cargo build --workspace

# Run the browser shell
dev:
    cargo run -p kb-shell --bin kendime-brow

# Tests, formatting and lints (same as CI)
test:
    cargo test --workspace

check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo deny check licenses bans sources

# Secret scan over the full history
scan:
    gitleaks git --redact --no-banner --config .gitleaks.toml .
