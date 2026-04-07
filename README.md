# bincast

Ship your Rust binary to every package manager with one command.

```bash
bincast publish v0.1.0
```

Your binary is now available via:

```bash
pip install your-tool         # PyPI
npm install your-tool         # npm
brew install your-tool        # Homebrew
scoop install your-tool       # Scoop
cargo install your-tool       # crates.io
cargo binstall your-tool      # pre-built binary
curl -sSL url | sh            # macOS/Linux
irm url | iex                 # Windows
```

## Quick Start

```bash
cargo install bincast
bincast init
bincast generate
git add . && git commit -m "add release infrastructure"
git tag v0.1.0 && git push --tags
```

## How It Works

### 1. Initialize

```bash
bincast init
```

Reads your `Cargo.toml` and writes a `bincast.toml`:

```toml
[package]
name = "my-tool"
binary = "my-tool"
repository = "https://github.com/you/my-tool"
license = "MIT"

[targets]
platforms = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "aarch64-unknown-linux-gnu",
  "x86_64-unknown-linux-gnu",
  "x86_64-unknown-linux-musl",
  "x86_64-pc-windows-msvc",
]

[distribute.github]
release = true

[distribute.pypi]
package_name = "my-tool"

[distribute.npm]
scope = "@my-org"

[distribute.homebrew]
tap = "you/homebrew-my-tool"

[distribute.scoop]
bucket = "you/scoop-my-tool"

[distribute.cargo]
crate_name = "my-tool"

[distribute.install_script]
enabled = true
```

### 2. Generate

```bash
bincast generate
```

Produces everything you need, ready to commit:

```
Generated:
  .github/workflows/release.yml     CI workflow (actions pinned to SHAs)
  install.sh                         macOS/Linux installer
  install.ps1                        Windows installer
  homebrew/my-tool.rb                Homebrew formula
  scoop/my-tool.json                 Scoop manifest
  binstall.toml                      cargo-binstall metadata
```

The generated CI workflow handles cross-compilation, maturin wheel building, SHA-256 checksums, smoke testing, SLSA attestation, OIDC trusted publishing to PyPI, npm platform package publishing, GitHub Release creation, and repository-dispatch to auto-update your Homebrew tap and Scoop bucket.

### 3. Publish

Tag and push. The generated workflow does everything else.

```bash
git tag v0.1.0
git push --tags
```

Or publish locally:

```bash
bincast publish v0.1.0
```

This builds the binary, creates archives, computes checksums, uploads to GitHub Releases, publishes to PyPI/npm/crates.io, and dispatches updates to your Homebrew tap and Scoop bucket.

### 4. Validate

```bash
bincast check
```

Validates config syntax, checks name availability on PyPI/npm/crates.io, and verifies your setup before you tag.

## Channels

| Channel | What bincast produces |
|---------|----------------------|
| **GitHub Releases** | Archives (tar.gz/zip) + SHA-256 checksums + SLSA attestation |
| **PyPI** | maturin wheels with `bindings = "bin"`, OIDC trusted publishing |
| **npm** | Platform-specific packages (esbuild pattern) |
| **Homebrew** | Formula in your tap repo, auto-updated via repository-dispatch |
| **Scoop** | Manifest in your bucket repo, auto-updated via repository-dispatch |
| **crates.io** | `cargo publish` |
| **cargo-binstall** | Metadata for pre-built binary installs |
| **Install scripts** | `curl -sSL url \| sh` (unix) + `irm url \| iex` (windows) |

## Performance

| Metric | Value |
|---|---|
| Binary size (stripped) | 599 KB |
| Dependencies | **0** |
| Startup time | <1ms |
| Memory usage | 1.5 MB |
| Generate (all channels) | 6ms |
| Test suite (215 tests) | 0.5s |

Zero dependencies means zero supply chain surface. The tool that secures your release pipeline has nothing to audit but its own code.

## License

MIT
