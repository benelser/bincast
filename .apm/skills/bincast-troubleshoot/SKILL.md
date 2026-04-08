---
name: bincast-troubleshoot
description: "Bincast: Diagnose and fix CI release failures."
metadata:
  version: 0.2.0
  openclaw:
    category: "recipe"
    domain: "devtools"
    requires:
      bins:
        - gh
      skills:
        - bincast-shared
---

# Troubleshoot CI Failures

> **Context:** A release was tagged but CI failed.

## Step 1: Get failure details

```bash
gh run view --log-failed
```

## Common failures

| Failure | Cause | Fix |
|---------|-------|-----|
| `Repository not found` on checkout | Private repo + missing permissions | Add `permissions: contents: write` to the job. Regenerate: `bincast generate` |
| `linker aarch64-linux-gnu-gcc not found` | Maturin container missing cross toolchain | Update to bincast >= 0.1.10 and regenerate: `bincast generate` |
| `shasum: command not found` (Windows) | Windows runners lack `shasum` | Regenerate: `bincast generate` (template handles this) |
| `A verified email address is required` | crates.io email not verified | Verify at https://crates.io/settings/profile |
| `File already exists` (PyPI) | Wheel version already uploaded | `bincast version patch` and release again |
| Homebrew SHA mismatch | Formula has wrong checksums | Re-dispatch: `gh api repos/OWNER/homebrew-REPO/dispatches -f event_type=update-formula -f 'client_payload[version]=vX.Y.Z'` |
| Smoke test fails on cross binary | ARM binary can't run on x86 runner | Regenerate: `bincast generate` (template skips cross-arch smoke tests) |

## Step 2: Fix and re-release

```bash
# Fix the issue (edit config, regenerate, etc.)
bincast generate
git add -A && git commit -m "fix CI"
git push

# Delete the failed tag and re-release
git tag -d vX.Y.Z
git push origin :refs/tags/vX.Y.Z
bincast release
```
