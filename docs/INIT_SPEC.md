# bincast init — UX Specification

## Principle

Do everything programmatically. Only pause for human input when we genuinely need it. Detect, execute, report. The golden path is ONE command from zero to fully configured release infrastructure.

## Flow

```
bincast init → detect → ask profile → ask channel config → preview → execute → secrets → done
```

### Stage 1: DETECT (programmatic)

- Read Cargo.toml (or workspace root → find binary crate)
- Extract: name, version, binary, repository, license, description
- Parse git remote for owner/repo
- Check if bincast.toml already exists (offer to reconfigure)
- Check if gh CLI is available (needed for repo creation + secrets)

Output:
```
  bincast v0.1.0 — Ship your Rust binary everywhere

  Detected: my-tool v0.2.0 (from Cargo.toml)
  Repository: https://github.com/user/my-tool
  Binary: my-tool
```

### Stage 2: PROFILE (ask)

```
  Distribution profile:
    1. Maximum Reach — pip, npm, brew, scoop, cargo, curl, irm
    2. Rust Ecosystem — cargo, binstall, curl, irm
    3. Minimal — GitHub Releases + install scripts
    4. Custom

  Choose [1-4]: 
```

Default: 3 (Minimal). Invalid input → use default with message.

### Stage 3: CONFIGURE (ask, conditional)

Only shown for channels that need user input:

- **npm** → `npm scope (e.g., @my-org):`
- **Homebrew** → `Homebrew tap [owner/homebrew-name]:` (smart default, enter accepts)
- **Scoop** → `Scoop bucket [owner/scoop-name]:` (smart default, enter accepts)

Channels without config (GitHub, PyPI, crates.io, install scripts, binstall) are enabled silently.

### Stage 4: PREVIEW + CONFIRM (ask)

Show a summary of everything that will happen:

```
  Ready to set up release infrastructure:

    Write bincast.toml (N channels, M targets)
    Generate .github/workflows/release.yml
    Generate install.sh + install.ps1
    Generate homebrew/name.rb          ← only if homebrew enabled
    Generate scoop/name.json           ← only if scoop enabled
    Create repo owner/homebrew-name    ← only if homebrew enabled
    Create repo owner/scoop-name       ← only if scoop enabled
    Check name availability            ← for enabled registries
    git add + commit

  Execute [Y/n]: 
```

### Stage 5: EXECUTE (programmatic)

Sequential, with progress:

```
  ✓ Wrote bincast.toml
  ✓ Generated 6 files
  ✓ Created owner/homebrew-name (private)
  ✓ Created owner/scoop-name (private)
  ✓ PyPI: 'name' is available
  ✓ npm: '@scope/name' is available
  ✓ crates.io: 'name' is available
  ✓ git add + commit: "Add bincast release infrastructure"
```

If gh CLI not available: skip repo creation, print manual instructions.
If name already taken: warn but continue (user may own it).
If git working tree dirty: warn, still generate files, skip commit.

### Stage 6: SECRETS (detect + guide)

Check what tokens are needed based on enabled channels:

```
  Secrets needed:
    ✓ GITHUB_TOKEN — automatic in GitHub Actions
    ! CARGO_REGISTRY_TOKEN — https://crates.io/settings/tokens
    ! PYPI_TOKEN — https://pypi.org/manage/account/token/
    ! NPM_TOKEN — https://www.npmjs.com/settings/~/tokens
```

For each missing secret:
1. If gh CLI available: offer to set it now
2. Print the exact URL to create the token
3. Use masked password input for paste
4. `gh secret set NAME --body "$token" --repo owner/repo`

```
  Set CARGO_REGISTRY_TOKEN now? [Y/n]: 
  Paste token (hidden): ********
  ✓ Set secret CARGO_REGISTRY_TOKEN for owner/repo
```

### Stage 7: DONE

```
  Done! Release with:

    bincast release
```

## Non-interactive Mode

When stdin is not a TTY, require flags:

```bash
bincast init --profile minimal
bincast init --profile maximum --npm-scope @myorg
bincast init --profile rust
```

Error when flags missing:
```
must provide --profile when not running interactively
```

## Error Patterns

Errors suggest the fix (gh CLI pattern):

```
✗ gh CLI not found — install it: https://cli.github.com/
  (repo creation and secret setup will be skipped)

✗ npm scope must start with '@' — example: @my-org

✗ bincast.toml already exists — delete it or run with --force

✗ no git remote found — push your repo first: git remote add origin ...
```

## Testing

Every path tested with expect scripts (AAA pattern):
- Each profile (1-4)
- Invalid input handling
- Workspace detection
- Full scenario: init → generate → files on disk
- Non-interactive with flags
- Secret setup flow (mocked gh)
- Existing bincast.toml (--force)
- Missing gh CLI (graceful degradation)
