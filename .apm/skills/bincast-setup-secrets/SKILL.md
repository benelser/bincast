---
name: bincast-setup-secrets
description: "Bincast: Set up registry tokens and GitHub secrets for release publishing."
metadata:
  version: 0.2.0
  openclaw:
    category: "recipe"
    domain: "devtools"
    requires:
      bins:
        - bincast
        - gh
      skills:
        - bincast-shared
      mcp:
        - microsoft/playwright-mcp
---

# Set Up Secrets

> **Prerequisite:** Project must have `bincast.toml` (run `bincast init` first).

> **Security:** Never read token values from the browser into agent context. The agent navigates to the right page and provides instructions. The user copies the token and pastes it into `gh secret set` themselves.

## Determine required secrets

Read `bincast.toml` and check which channels are enabled:

| Channel | Secret | Create at |
|---------|--------|-----------|
| `[distribute.cargo]` | `CARGO_REGISTRY_TOKEN` | https://crates.io/settings/tokens |
| `[distribute.pypi]` with `auth = "token"` | `PYPI_TOKEN` | https://pypi.org/manage/account/token/ |
| `[distribute.pypi]` with `auth = "oidc"` | None | Configure trusted publisher on PyPI |
| `[distribute.npm]` | `NPM_TOKEN` | https://www.npmjs.com/settings/~/tokens/granular-access-tokens/new |
| `[distribute.homebrew]` | `TAP_GITHUB_TOKEN` | https://github.com/settings/personal-access-tokens/new |
| `[distribute.github]` | `GITHUB_TOKEN` | Automatic, no setup needed |

Check which are already set:

```bash
gh secret list --repo owner/repo
```

Only set up missing secrets.

## PyPI OIDC trusted publishing

If `auth = "oidc"` is configured, no `PYPI_TOKEN` is needed. Two setup steps are required:

### Step 1: Create a GitHub environment

The generated workflow uses `environment: name: pypi`. This environment must exist in the GitHub repo:

1. Go to `https://github.com/OWNER/REPO/settings/environments`
2. Click **New environment**
3. Name it `pypi` (must match exactly)
4. Save — no additional protection rules needed

Or via CLI:

```bash
gh api repos/OWNER/REPO/environments/pypi -X PUT
```

### Step 2: Add a trusted publisher on PyPI

**First release (project doesn't exist on PyPI yet):**

Use a pending publisher — PyPI lets you register trust before the project exists:

1. Go to `https://pypi.org/manage/account/publishing/`
2. Under **Add a new pending publisher**, fill in:
   - **Project name:** your PyPI package name (from `bincast.toml`)
   - **Owner:** GitHub username or org
   - **Repository:** repo name (without owner prefix)
   - **Workflow name:** `release.yml`
   - **Environment name:** `pypi`
3. Save

The first publish from CI will automatically create the project on PyPI and claim the name.

**Existing project (already published to PyPI):**

1. Go to `https://pypi.org/manage/project/PACKAGE_NAME/settings/publishing/`
2. Select **GitHub Actions** as the publisher
3. Fill in the same fields: owner, repository, `release.yml`, `pypi`
4. Save

The environment name on PyPI must match the GitHub environment name. bincast uses `pypi` for both.

The CI workflow already has `id-token: write` permission and uses `pypa/gh-action-pypi-publish` for automatic OIDC token exchange.

Docs: https://docs.pypi.org/trusted-publishers/

## Browser-assisted flow (with Playwright MCP)

Use `@playwright/mcp` tools (prefixed with `browser_`). If only chrome-devtools tools are available, use the manual flow below.

For each missing secret:

**1. Navigate** to the token creation page using `browser_navigate`.

**2. Wait** for the user to log in if needed.

**3. Guide** the user through token creation:

| Secret | Instructions |
|--------|-------------|
| `CARGO_REGISTRY_TOKEN` | New Token, name: `bincast-release`, scopes: publish-new + publish-update |
| `PYPI_TOKEN` | Token name: `bincast-release`, scope: entire account or project-scoped |
| `NPM_TOKEN` | Granular Access Token, name: `bincast-release`, expiration: 90 days, packages: read and write, organizations: no access. Granular tokens automatically satisfy 2FA requirements for CI publishing. npm does not support OIDC trusted publishing — a token is always required. |
| `TAP_GITHUB_TOKEN` | Fine-grained PAT, name: `bincast-tap`, repository access: tap repo only, permissions: Contents read/write |

**4. User sets the secret** (agent never sees the token):

```bash
gh secret set SECRET_NAME --repo owner/repo
```

**5. Verify:**

```bash
gh secret list --repo owner/repo
```

## Manual flow (no Playwright MCP)

Same steps, but instead of navigating the browser, tell the user:

```
Please open [URL] in your browser, then follow these steps...
```

## After all secrets are set

```bash
bincast check
```

If everything passes, proceed to the first release (see `bincast-release`).
