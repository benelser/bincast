---
name: bincast-init
description: "Bincast: Set up a Rust project for multi-platform binary distribution."
metadata:
  version: 0.2.0
  openclaw:
    category: "recipe"
    domain: "devtools"
    requires:
      bins:
        - bincast
      skills:
        - bincast-shared
---

# Initialize a Project

> **Prerequisite:** Verify bincast is installed (see `bincast-shared`).

## Pre-checks

Use file reading tools (Read, Glob) for checks. Do not use shell `test -f` commands.

1. Read `Cargo.toml` to confirm it exists and has a `[package]` section
2. Confirm `bincast.toml` does not already exist
3. Run `git remote -v` to verify a git remote is configured

## Non-interactive flow (recommended for agents)

Ask the user how they want people to install their tool. Map their answer to channels:

| User intent | Channels |
|-------------|----------|
| "pip install" | `github,pypi,install-scripts` |
| "npm install" | `github,npm,install-scripts` (requires `--npm-scope`) |
| "brew install" | `github,homebrew,install-scripts` |
| "cargo install" | `github,cargo,install-scripts` |
| "everything" | `github,pypi,npm,homebrew,cargo,install-scripts` |
| "just GitHub releases" | `github,install-scripts` |

Confirm the plan with the user before running:

```
I'll set up bincast with these channels:
  - GitHub Releases (archives + checksums)
  - Homebrew (brew install owner/tap/my-tool)
  - crates.io (cargo install my-tool)
  - Install scripts (curl | sh)

This creates bincast.toml, a CI workflow, and install scripts. Proceed?
```

Then run:

```bash
bincast init --channels github,homebrew,cargo,install-scripts --yes
```

### Channel-specific flags

| Flag | When required |
|------|---------------|
| `--npm-scope @org` | npm channel is included |
| `--tap owner/homebrew-name` | Override default tap repo name |

## Interactive flow

If the user prefers to run it themselves:

```bash
! bincast init
```

The `!` prefix runs the command in the current session so the interactive prompts work.

## After init

1. Run `bincast check` to validate the setup
2. Set up secrets for the enabled channels (see `bincast-setup-secrets`)
3. Make a first release: `bincast version patch && bincast release`

> **Important:** Do not skip secret setup. The first release will fail if required tokens are missing.
