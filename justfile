# OpenSimTelemetry build recipes

# Default: list available recipes
default:
    @just --list

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
    cargo test

# Run clippy lints
lint:
    cargo clippy --all-targets -- -D warnings

# Format code
fmt:
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Clean build artifacts
clean:
    cargo clean

# Install cross-compilation tools (one-time setup, requires rustup-managed Rust)
setup-cross:
    @command -v rustup >/dev/null 2>&1 || { echo "Error: rustup is required for cross-compilation (brew install rustup)"; exit 1; }
    rustup target add x86_64-pc-windows-msvc
    cargo install cargo-xwin
