# Task runner (https://github.com/casey/just). Architecture §13.

default: test

# Build the kb:// UI (ui/dist is embedded into kb-shell; debug builds read it from disk)
ui:
    cd ui && npm ci && npm run build

# Build the whole workspace
build: ui
    cargo build --workspace

# Run the browser shell
dev: ui
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
