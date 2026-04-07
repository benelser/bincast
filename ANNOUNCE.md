# Releaser: Ship Your Rust Binary to Every Package Manager with One Command

Every week, another team builds something great in Rust and then spends two weeks figuring out how to get it into people's hands.

The Rust ecosystem produces the best CLI tools in the world — ruff, uv, ripgrep, bat, delta, biome, turbopack. But shipping them? That's still a mess. Every one of these projects maintains hundreds of lines of custom CI, bespoke Python scripts for wheel packaging, hand-rolled npm platform packages, and manually updated Homebrew formulas. They all solve the same problem independently, and they all hate it.

Today we're releasing **releaser** — a single command that makes your Rust binary installable from every package manager.

```bash
releaser publish v0.1.0
```

That's it. Your binary is now available via:

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

All with correct platform detection, SHA-256 checksums, SLSA build provenance, and native packaging for each ecosystem.

---

## The Problem

A pattern is taking over developer tooling: build the kernel in Rust, distribute it everywhere. ruff replaced flake8 and black. uv replaced pip. Biome replaced prettier and eslint. The Rust binary is the product. The package managers are delivery vehicles.

But Rust has no unified distribution story. To ship a binary to PyPI, npm, Homebrew, Scoop, and GitHub Releases today, you need:

- **maturin** to build Python wheels
- **cargo-dist** or hand-rolled CI for GitHub Releases
- **Custom npm scaffolding** for platform-specific packages
- **A separate Homebrew tap repo** with a manually updated formula
- **A separate Scoop bucket repo** with a manually updated manifest
- **Install scripts** you write yourself
- **500+ lines of GitHub Actions YAML** stitching it all together

This is what the ruff team maintains. And the uv team. And the Codex CLI team. And every other Rust project that distributes beyond `cargo install`. Each one independently built the same infrastructure.

We looked at five of the most successful projects doing this — ruff, uv, Microsoft APM, OpenAI Codex CLI, and Claude Code — and extracted every pattern worth adopting. Then we built the tool that makes all of it a commodity operation.

---

## How It Works

### 1. Initialize

```bash
releaser init
```

Reads your `Cargo.toml`, asks which distribution channels you want, and writes a `releaser.toml`:

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
releaser generate
```

Produces everything you need — ready to commit:

```
Generated:
  .github/workflows/release.yml     CI workflow (actions pinned to SHAs)
  install.sh                         macOS/Linux installer
  install.ps1                        Windows installer
  homebrew/my-tool.rb                Homebrew formula
  scoop/my-tool.json                 Scoop manifest
```

The generated CI workflow handles:
- Cross-compilation for all configured targets
- Dual output per build — one compilation produces both a maturin wheel and a standalone archive
- SHA-256 checksums as sidecar files on every artifact
- Smoke testing every artifact before publishing (install it, run `--help`)
- Build provenance attestation (SLSA)
- PyPI publishing via OIDC trusted publishing (no API tokens)
- crates.io publishing via OIDC
- npm platform package publishing
- GitHub Release creation with all assets
- Repository-dispatch to auto-update your Homebrew tap and Scoop bucket
- README transformation for PyPI compatibility

### 3. Publish

Tag and push. The generated workflow does everything else.

```bash
git tag v0.1.0
git push --tags
```

Or run locally:

```bash
releaser publish v0.1.0
```

---

## What Makes This Different

### Not GoReleaser for Rust

GoReleaser is a great tool for Go projects. Its Rust support shells out to `cargo-zigbuild` and wraps the binary. It doesn't understand Cargo workspaces, can't generate maturin wheels, can't publish to PyPI, and npm support is locked behind a paid Pro tier. For a project where `pip install my-tool` is the primary install path, GoReleaser can't help.

Releaser is built for the polyglot binary distribution pattern. PyPI and npm aren't afterthoughts — they're tier 1 distribution channels, because that's where your users are. A Python developer will never run `cargo install`. They need `pip install` to just work.

### Not cargo-dist

cargo-dist generates CI and install scripts, and it's a good tool. But when the ruff and uv teams adopted it, they ended up overriding most of its build jobs with custom ones. They use cargo-dist for installer generation and release management while maintaining full custom control of cross-platform builds and PyPI publishing. At that point, you're maintaining custom CI anyway.

Releaser owns the entire pipeline. No overrides, no escape hatches, no stitching together three tools that don't share configuration.

### Fully Open Source

The tool that builds and publishes your releases should not be closed source. GoReleaser Pro is a closed-source binary that compiles your code and publishes your artifacts — any compromise of the maintainer is a potential compromise of your users. We think that's the wrong tradeoff.

Releaser is open source, top to bottom. Audit it, fork it, vendor it. Your supply chain is your problem, and you should be able to verify every link in it.

### Proper Dry Run

GoReleaser's `--snapshot` mode has been insufficient for five years (issue #2355, still open). It can't validate your config against real APIs, can't detect errors that only surface during actual publishing, and can't fake versions for CI testing.

```bash
releaser check
```

This validates everything: config syntax, name availability on PyPI/npm/crates.io, API token permissions, target triple validity, Cargo.toml metadata. It tells you what will break before you tag.

---

## Channels

### Tier 1 — Available Today

| Channel | What releaser produces |
|---------|----------------------|
| **GitHub Releases** | Archives (tar.gz/zip) + SHA-256 checksums + SLSA attestation |
| **PyPI** | maturin wheels with `bindings = "bin"`, OIDC trusted publishing |
| **npm** | Platform-specific packages (esbuild pattern), OIDC publishing |
| **Homebrew** | Formula in your tap repo, auto-updated via repository-dispatch |
| **Scoop** | Manifest in your bucket repo, auto-updated via repository-dispatch |
| **crates.io** | `cargo publish` with OIDC auth |
| **cargo-binstall** | Metadata for pre-built binary installs |
| **Install scripts** | `curl -sSL url \| sh` (unix) + `irm url \| iex` (windows) |

### Tier 2 — Coming Soon

| Channel | Status |
|---------|--------|
| **winget** | Auto-PR to microsoft/winget-pkgs |
| **deb/rpm** | via cargo-deb, hosted on GitHub Releases or your own repo |
| **AUR** | PKGBUILD generation and publishing |

---

## The Manifest

Everything is driven by `releaser.toml`. No YAML. No Go templates with evaluation ordering bugs. No separate config files for each channel. One file, one source of truth, with sane defaults derived from your `Cargo.toml`.

If you don't configure a section, it doesn't run. If you configure it, it works. The entire config for most projects is under 30 lines.

---

## Who This Is For

You've written a tool in Rust. It works. It's good. Now you need to ship it.

You don't want to spend two weeks writing CI workflows. You don't want to maintain a Homebrew tap repo. You don't want to learn maturin's wheel packaging. You don't want to scaffold five npm platform packages. You don't want to write install scripts. You don't want to set up OIDC trusted publishing with three different registries.

You want to run one command and have your binary show up in every package manager your users already use.

That's releaser.

```bash
cargo install releaser
releaser init
releaser generate
git add . && git commit -m "add release infrastructure"
git tag v0.1.0 && git push --tags
```

Five commands. Your tool is now installable by every developer on every platform through whatever package manager they already have.

---

## Get Started

```bash
cargo install releaser
```

- [Documentation](https://github.com/benelser/releaser)
- [GitHub](https://github.com/benelser/releaser)
- [releaser.toml reference](https://github.com/benelser/releaser/blob/main/docs/config.md)

Releaser is MIT licensed. Contributions welcome.
