# OpenSimTelemetry build recipes

export PATH := env("HOME") + "/.cargo/bin:" + env("PATH")

# Default: list available recipes
default:
    @just --list

# Check compilation (fast, no codegen)
check:
    cargo check --workspace

# Build for the host platform (debug)
build:
    cargo build

# Build for the host platform (release)
release:
    cargo build --release

# Build for Windows (cross-compile from macOS using cargo-xwin)
build-windows:
    cargo xwin build --release --target x86_64-pc-windows-msvc

# Run the server locally (debug)
run:
    cargo run -p ost-server

# Run the server locally (release)
run-release:
    cargo run --release -p ost-server

# Run all tests
test:
    cargo test --workspace

# Run clippy lints
lint:
    cargo clippy --all-targets -- -D warnings

# Format code
fmt:
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Run benchmarks
bench:
    cargo bench --workspace

# Run check + lint + test (CI-equivalent)
ci: check lint test fmt-check

# Run Playwright E2E browser tests (Chromium in Docker)
e2e:
    #!/usr/bin/env bash
    set -euo pipefail
    PW_VERSION=$(node -p "require('./tests/e2e/node_modules/@playwright/test/package.json').version")
    # Kill any stale ost-server from a previous run
    pkill -f 'target/release/ost-server' 2>/dev/null || true
    echo "Starting Chromium browser server in Docker (Playwright v${PW_VERSION})..."
    docker run --rm -d --name ost-pw-chromium \
      -p 3000:3000 \
      --init \
      "mcr.microsoft.com/playwright:v${PW_VERSION}-noble" \
      /bin/bash -c "npx -y @playwright/test@${PW_VERSION} run-server --port 3000 --host 0.0.0.0"
    trap 'docker stop ost-pw-chromium 2>/dev/null || true' EXIT
    echo "Waiting for browser server..."
    for i in $(seq 1 60); do
      if curl -sf http://localhost:3000/json/version > /dev/null 2>&1 || nc -z localhost 3000 2>/dev/null; then break; fi
      sleep 1
    done
    # Extra settle time for run-server initialization
    sleep 3
    echo "Running E2E tests..."
    cd tests/e2e && PLAYWRIGHT_WS_ENDPOINT=ws://localhost:3000 npm test

# Install E2E test dependencies (npm only, browser runs in Docker)
e2e-install:
    cd tests/e2e && npm install

# Clean build artifacts
clean:
    cargo clean

# Install cross-compilation tools (one-time setup, requires rustup-managed Rust)
setup-cross:
    @command -v rustup >/dev/null 2>&1 || { echo "Error: rustup is required for cross-compilation (brew install rustup)"; exit 1; }
    rustup target add x86_64-pc-windows-msvc
    cargo install cargo-xwin
