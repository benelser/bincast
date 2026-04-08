---
name: bincast-release
description: "Bincast: Bump version and release a new version of your project."
metadata:
  version: 0.2.0
  openclaw:
    category: "recipe"
    domain: "devtools"
    requires:
      bins:
        - bincast
        - git
      skills:
        - bincast-shared
---

# Release a New Version

> **Prerequisite:** Project has `bincast.toml` and secrets are configured.

## Pre-checks

1. Working tree must be clean (`git status`)
2. Must be on `main` or `master` branch (for `bincast release`)
3. `bincast check` should pass

## Solo developer flow

```bash
bincast check                # validate setup
bincast version patch        # bump, commit (or minor / major)
bincast release              # tag, push — CI handles the rest
```

## Team flow (branch protection)

```bash
# On a feature branch:
bincast version patch
# Open PR, get review, merge

# After merge, on main:
git checkout main && git pull
bincast release
```

## What each command does

**`bincast version patch|minor|major`**
- Reads the current version from `Cargo.toml`
- Bumps it according to semver
- Updates `Cargo.toml` (and `workspace.package.version` if applicable)
- Commits with message `release v{version}`

**`bincast release`**
- Reads version from `Cargo.toml`
- Checks: on main/master, clean tree, tag doesn't already exist, CI workflow present
- Creates tag `v{version}`, pushes commit and tag
- CI builds for all targets and publishes to configured channels

## Recovery

| Problem | Fix |
|---------|-----|
| Tag already exists | `bincast version patch` to bump, then retry |
| CI fails | See `bincast-troubleshoot` |
| Wrong version tagged | `git tag -d vX.Y.Z && git push origin :refs/tags/vX.Y.Z`, then re-release |

> **Caution:** `bincast release` pushes a tag that triggers CI publishing to registries. Always confirm the version with the user before running.
