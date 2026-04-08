# bincast

Ship your Rust binary to every package manager with one command.

## Get Started

The fastest way to set up bincast is with an AI agent. Install the skill package and ask your agent to set up releases:

```bash
apm install benelser/bincast
```

Your agent reads the skills, installs bincast, asks how people should install your tool, and runs `bincast init` non-interactively. Works with Claude Code, Copilot, Cursor, and Codex.

### Manual setup

```bash
brew install benelser/bincast/bincast   # or: cargo install bincast
bincast init                            # interactive wizard
bincast version patch                   # bump version
bincast release                         # tag, push, CI does the rest
```

Your binary is now available via:

```bash
brew install you/tap/your-tool  # Homebrew
cargo install your-tool         # crates.io
pip install your-tool           # PyPI
npm install @org/your-tool      # npm
cargo binstall your-tool        # pre-built binary
curl -sSL url | sh              # macOS/Linux
irm url | iex                   # Windows
```

## How It Works

### Initialize

```bash
bincast init
```

Detects your project, asks how people should install it, and generates everything: config, CI workflow, install scripts, Homebrew formula, and cargo-binstall metadata. Creates Homebrew tap repos via `gh` and guides you through secret setup.

Non-interactive mode:

```bash
bincast init --channels github,pypi,cargo,homebrew,install-scripts --npm-scope @myorg --yes
```

### Version

```bash
bincast version patch    # bumps Cargo.toml, commits
bincast version minor
bincast version major
```

### Release

```bash
bincast release          # tags current version, pushes — CI builds and publishes
```

`version` decides what to release. `release` executes it.

For teams with branch protection:
1. Developer runs `bincast version patch` on a branch, opens a PR
2. After merge, release lead runs `bincast release` on main

### Validate

```bash
bincast check
```

Validates config, checks name availability on registries, and verifies secrets are configured.

## Channels

| Channel | What bincast generates |
|---------|----------------------|
| **GitHub Releases** | Archives (tar.gz/zip) with SHA-256 checksums |
| **PyPI** | Maturin wheels with OIDC trusted publishing |
| **npm** | Platform-specific packages (esbuild pattern) |
| **Homebrew** | Formula in your tap repo, auto-updated on release |
| **crates.io** | Standard `cargo publish` |
| **cargo-binstall** | Metadata for pre-built binary installs |
| **Install scripts** | `curl \| sh` for unix, `irm \| iex` for Windows |

## Configuration

All configuration lives in `bincast.toml`:

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
auth = "oidc"

[distribute.npm]
scope = "@my-org"

[distribute.homebrew]
tap = "you/homebrew-my-tool"

[distribute.cargo]
crate_name = "my-tool"

[distribute.install_script]
enabled = true
```

Supports Cargo workspaces, multiple binaries, and custom target triples.

## Install

```bash
brew install benelser/bincast/bincast
```

```bash
cargo install bincast
```

```bash
curl -sSL https://raw.githubusercontent.com/benelser/bincast/main/install.sh | sh
```

## License

MIT
