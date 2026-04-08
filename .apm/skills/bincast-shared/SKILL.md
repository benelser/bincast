---
name: bincast-shared
description: "Bincast: shared reference for installation, configuration, and all commands."
metadata:
  version: 0.2.0
  openclaw:
    category: "reference"
    domain: "devtools"
    requires:
      bins:
        - bincast
---

# Bincast Reference

## Installation

Before using any bincast skill, verify it is installed:

```bash
which bincast && bincast version
```

If not found, install it:

| Platform | Command |
|----------|---------|
| macOS | `brew install benelser/bincast/bincast` |
| Linux | `curl -sSL https://raw.githubusercontent.com/benelser/bincast/main/install.sh \| sh` |
| Windows | `irm https://raw.githubusercontent.com/benelser/bincast/main/install.ps1 \| iex` |
| Any (with Rust) | `cargo install bincast` |

Do not build from source in `apm_modules`. Always install from a package manager.

## Commands

| Command | Purpose |
|---------|---------|
| `bincast init` | Set up a project for multi-platform distribution |
| `bincast generate` | Regenerate CI workflow and distribution files from config |
| `bincast check` | Validate config, check registry name availability, verify secrets |
| `bincast version patch\|minor\|major` | Bump version in Cargo.toml and commit |
| `bincast release` | Tag the current version, push, and trigger CI |

## Configuration

All configuration lives in `bincast.toml`. Created by `bincast init`, edited directly to add or change channels.

```toml
[package]
name = "my-tool"
binary = "my-tool"
repository = "https://github.com/owner/repo"
license = "MIT"

[targets]
platforms = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-unknown-linux-gnu",
  "x86_64-pc-windows-msvc",
]

[distribute.github]
release = true

[distribute.install_script]
enabled = true
```

## Channels

| Channel | Config section | Required secret |
|---------|---------------|-----------------|
| GitHub Releases | `[distribute.github]` | None (automatic `GITHUB_TOKEN`) |
| PyPI | `[distribute.pypi]` | None with `auth = "oidc"`, or `PYPI_TOKEN` |
| npm | `[distribute.npm]` | `NPM_TOKEN` |
| Homebrew | `[distribute.homebrew]` | `TAP_GITHUB_TOKEN` |
| crates.io | `[distribute.cargo]` | `CARGO_REGISTRY_TOKEN` |
| Install scripts | `[distribute.install_script]` | None |

## Conventions

- Version source of truth: `Cargo.toml`
- Tag format: `v{version}` (e.g. `v0.2.0`)
- CI triggers on tag push matching `v*`
- `bincast version` bumps and commits. `bincast release` tags and pushes. They compose.
