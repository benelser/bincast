# bincast

Ship your Rust binary to every package manager with one command.

## Install

```bash
brew install benelser/bincast/bincast
# or
cargo install bincast
# or
curl -sSL https://raw.githubusercontent.com/benelser/bincast/main/install.sh | sh
```

## Quick Start

```bash
bincast init                    # set up distribution channels
bincast version patch           # bump version
bincast release                 # tag, push, CI does the rest
```

That's it. Your binary is now available via:

```bash
pip install your-tool           # PyPI
npm install @org/your-tool      # npm
brew install you/tap/your-tool  # Homebrew
cargo install your-tool         # crates.io
cargo binstall your-tool        # pre-built binary
curl -sSL url | sh              # macOS/Linux
irm url | iex                   # Windows
```

## How It Works

### Initialize

```bash
bincast init
```

Interactive wizard that detects your project, asks how people should install it, and generates everything — config, CI workflow, install scripts, Homebrew formula, cargo-binstall metadata. Creates Homebrew tap repos automatically via `gh`. Guides you through setting up secrets.

Or non-interactive for AI agents:

```bash
bincast init --channels github,pypi,cargo,homebrew,install-scripts --npm-scope @myorg --yes
```

### Release

Two composable commands:

```bash
bincast version patch    # bumps Cargo.toml, commits
bincast release          # tags current version, pushes, CI builds + publishes
```

`version` decides WHAT to release. `release` executes it. They compose.

For teams with branch protection:
- Developer runs `bincast version patch` on a feature branch, includes it in PR
- After merge, release lead runs `bincast release` on main

### Validate

```bash
bincast check
```

Validates config, checks name availability on registries, verifies tokens are set.

## Channels

| Channel | What bincast produces |
|---------|----------------------|
| **GitHub Releases** | Archives (tar.gz/zip) + SHA-256 checksums |
| **PyPI** | maturin wheels, OIDC trusted publishing (no token needed) |
| **npm** | Platform-specific packages (esbuild pattern) |
| **Homebrew** | Formula in your tap repo, auto-updated via repository-dispatch |
| **crates.io** | `cargo publish` |
| **cargo-binstall** | Metadata for pre-built binary installs |
| **Install scripts** | `curl -sSL url \| sh` (unix) + `irm url \| iex` (windows) |

## Configuration

Everything is driven by `bincast.toml`:

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
auth = "oidc"                    # no token needed

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

## AI Agent Integration

bincast ships as an [APM](https://github.com/microsoft/apm) package with skills that teach AI agents (Claude Code, Copilot, Cursor, Codex) how to set up and release your project.

```bash
apm install benelser/bincast
```

This installs 6 skills:

| Skill | What the agent learns |
|-------|----------------------|
| `bincast-shared` | Installation, config format, all commands |
| `bincast-init` | Project setup with `--channels` flags |
| `bincast-release` | Version bump + tag (solo and team flows) |
| `bincast-add-channel` | Add distribution channels step-by-step |
| `bincast-setup-secrets` | Create tokens and set GitHub secrets (browser-assisted) |
| `bincast-troubleshoot` | Diagnose CI failures with fixes |

The agent reads the skills, installs bincast if needed, asks you how people should install your tool, and runs `bincast init --channels ... --yes` non-interactively.

## License

MIT
