# GitHub Actions Workflows

This project uses GitHub Actions for continuous integration and automated releases.

## CI Workflow (`ci.yml`)

Runs on every push and pull request to `main` or `develop` branches.

### Jobs

1. **Check** - Verifies code compiles
2. **Test Suite** - Runs all tests on Windows, macOS, and Linux
3. **Rustfmt** - Ensures code is properly formatted
4. **Clippy** - Runs Rust linter for common issues
5. **Build** - Creates release builds and smoke tests the server

### Running Locally

```bash
# Check formatting
cargo fmt --check

# Run clippy
cargo clippy --all-features -- -D warnings

# Run tests
cargo test --all-features
```

## Release Workflow (`release.yml`)

Automatically creates multi-platform releases when you push a version tag.

### Supported Platforms

- **Windows x64** (`x86_64-pc-windows-msvc`) - Full iRacing support
- **macOS Intel** (`x86_64-apple-darwin`)
- **macOS Apple Silicon** (`aarch64-apple-darwin`)
- **Linux x64** (`x86_64-unknown-linux-gnu`)

### Creating a Release

1. Update version in `Cargo.toml` files
2. Commit changes:
   ```bash
   git add .
   git commit -m "Bump version to 0.1.0"
   ```

3. Create and push a tag:
   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```

4. GitHub Actions will:
   - Build binaries for all platforms
   - Run tests
   - Create a GitHub Release
   - Attach binary archives (`.tar.gz` for Unix, `.zip` for Windows)

### Manual Trigger

You can also trigger the release workflow manually from the GitHub Actions tab without creating a tag. This is useful for testing the build process.

## Artifacts

Build artifacts are:
- Cached between runs for faster builds
- Available for download for 90 days after each workflow run
- Automatically attached to GitHub Releases when tags are pushed

## Notes

- The Windows build includes the full iRacing adapter with shared memory support
- macOS and Linux builds include a stub iRacing adapter (detection always returns false)
- All builds include the demo adapter for testing
- Binary names:
  - Windows: `ost-server.exe`
  - macOS/Linux: `ost-server`
