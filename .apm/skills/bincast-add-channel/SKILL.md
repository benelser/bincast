---
name: bincast-add-channel
description: "Bincast: Add a distribution channel to an existing project."
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

# Add a Distribution Channel

> **Prerequisite:** Project must have `bincast.toml` (run `bincast init` first).

## Steps

1. **Ask** which channel to add

2. **Edit `bincast.toml`** to add the channel section:

| Channel | Config to add |
|---------|---------------|
| PyPI | `[distribute.pypi]` with `package_name` and optionally `auth = "oidc"` |
| npm | `[distribute.npm]` with `scope` |
| Homebrew | `[distribute.homebrew]` with `tap` |
| crates.io | `[distribute.cargo]` with `crate_name` |
| Install scripts | `[distribute.install_script]` with `enabled = true` |

3. **Regenerate** CI and distribution files:

```bash
bincast generate
```

4. **Create supporting infrastructure** if needed:

```bash
# Homebrew: create the tap repo
gh repo create owner/homebrew-my-tool --public

# crates.io: verify email at https://crates.io/settings/profile
```

5. **Set up secrets** for the new channel (see `bincast-setup-secrets`)

6. **Commit** the changes:

```bash
git add bincast.toml .github/workflows/release.yml
git commit -m "add [channel] distribution"
```

7. **Verify:** `bincast check`

## Notes

- Always run `bincast generate` after editing `bincast.toml` to regenerate the CI workflow.
- PyPI with `auth = "oidc"` uses trusted publishing and requires no token. Configure the trusted publisher on pypi.org instead.
- Homebrew taps require a fine-grained GitHub PAT with `Contents: Read and write` on the tap repo only.
