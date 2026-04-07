---
name: bincast-init
description: "Bincast: Set up a new project for multi-platform distribution."
metadata:
  version: 0.1.1
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

> **PREREQUISITE:** Read `../bincast-shared/SKILL.md` for installation and config format.

Guide the user through setting up bincast for their Rust project.

## Pre-checks

1. Verify `Cargo.toml` exists in the current directory
2. Verify `bincast.toml` does NOT exist (if it does, ask if they want to reconfigure)
3. Verify the project has a git remote (needed for repository URL)

## Steps

1. Run `bincast init` — this is interactive and handles everything:
   - Detects project type (single crate or workspace)
   - Asks which distribution profile (Maximum Reach, Rust Ecosystem, Minimal, Custom)
   - Generates `bincast.toml`, CI workflow, install scripts, Homebrew formula, Scoop manifest
   - Creates Homebrew tap / Scoop bucket repos via `gh` if available
   - Checks name availability on registries
   - Commits generated files
   - Guides through secret setup (tokens for crates.io, PyPI, npm, etc.)

2. After init completes, verify with `bincast check`

## Tips

- For private repos, install `gh` CLI first — bincast uses it for authenticated downloads and repo creation.
- The Minimal profile (option 3) is the safest starting point — GitHub Releases + install scripts.
- You can add more channels later by editing `bincast.toml` and running `bincast generate`.

> [!CAUTION]
> `bincast init` creates repositories (Homebrew tap, Scoop bucket) if `gh` is available. Confirm with the user before running.

## See Also

- `../bincast-shared/SKILL.md` — config reference
- `../bincast-release/SKILL.md` — releasing after setup
