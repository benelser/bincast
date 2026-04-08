//! Init stages — each stage is a function that does one thing.

use std::path::Path;
use std::process::Command;

use crate::cargo::{self, CargoMetadata};
use crate::config::*;
use crate::config::defaults;
use crate::error::Result;
use super::prompts;

// --- Detection ---

pub struct Detection {
    pub name: String,
    pub version: String,
    pub _binary: Option<String>,
    pub repository: String,
    pub owner: String,
    pub repo_name: String,
    pub is_workspace: bool,
    pub cargo_meta: CargoMetadata,
    pub gh_available: bool,
    /// All binary crates found in workspace (empty for single crate)
    pub all_binaries: Vec<(String, Option<String>)>, // (binary_name, package_name)
}

pub fn detect(project_dir: &Path) -> Result<Detection> {
    let project = cargo::read_project(project_dir)?;

    let meta = match &project {
        cargo::ProjectKind::SingleCrate(m) => m.clone(),
        cargo::ProjectKind::Workspace { root_meta, members } => {
            cargo::resolve_workspace_binary(project_dir, root_meta, members)?
        }
    };

    let is_workspace = matches!(&project, cargo::ProjectKind::Workspace { .. });

    let all_binaries: Vec<(String, Option<String>)> = match &project {
        cargo::ProjectKind::Workspace { members, .. } => {
            cargo::workspace_binaries(members)
                .iter()
                .map(|m| {
                    let bin_name = m.binary_name.clone().unwrap_or_else(|| m.name.clone());
                    (bin_name, Some(m.name.clone()))
                })
                .collect()
        }
        _ => Vec::new(),
    };

    // Detect repository URL: Cargo.toml → git remote → empty
    let repository = meta.repository.clone()
        .filter(|r| !r.is_empty())
        .or_else(|| {
            git_remote_owner_repo()
                .map(|(o, r)| format!("https://github.com/{o}/{r}"))
        })
        .unwrap_or_default();

    let (owner, repo_name) = cargo::parse_github_url(&repository)
        .map(|(o, r)| (o.to_string(), r.to_string()))
        .unwrap_or_else(|| ("user".into(), meta.name.clone()));

    let gh_available = Command::new("gh").arg("--version").output().is_ok();

    Ok(Detection {
        name: meta.name.clone(),
        version: meta.version.clone(),
        _binary: meta.binary.clone(),
        repository,
        owner,
        repo_name,
        is_workspace,
        cargo_meta: meta,
        gh_available,
        all_binaries,
    })
}

fn git_remote_owner_repo() -> Option<(String, String)> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    let url = String::from_utf8_lossy(&output.stdout);
    let url = url.trim();
    // Handle SSH: git@github.com:owner/repo.git
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let rest = rest.trim_end_matches(".git");
        let (owner, repo) = rest.split_once('/')?;
        return Some((owner.to_string(), repo.to_string()));
    }
    // Handle HTTPS
    cargo::parse_github_url(url).map(|(o, r)| (o.to_string(), r.to_string()))
}

// --- Profile ---

#[derive(Debug, Clone, Copy)]
pub enum Profile {
    MaximumReach,
    RustEcosystem,
    Minimal,
    Custom,
}

pub fn ask_profile() -> Result<Profile> {
    eprintln!("  How should people install your tool?");
    let choice = prompts::select(
        "  Choose [1-4]",
        3,
        &[
            ("Rust developers", "cargo install, cargo binstall, curl, irm"),
            ("GitHub Releases only", "download from releases page + install scripts"),
            ("Pick channels", "choose exactly which package managers"),
        ],
    )?;

    Ok(match choice {
        1 => Profile::MaximumReach,
        2 => Profile::RustEcosystem,
        3 => Profile::Minimal,
        4 => Profile::Custom,
        _ => Profile::Minimal,
    })
}

// --- Channel Configuration ---

pub struct ChannelConfig {
    pub install_scripts: bool,
    pub pypi_name: Option<String>,
    pub npm_scope: Option<String>,
    pub homebrew_tap: Option<String>,
    pub cargo_crate: Option<String>,
}

pub fn configure_channels(profile: Profile, det: &Detection) -> Result<ChannelConfig> {
    eprintln!();
    match profile {
        Profile::MaximumReach => configure_maximum(det),
        Profile::RustEcosystem => Ok(configure_rust(det)),
        Profile::Minimal => Ok(configure_minimal()),
        Profile::Custom => configure_custom(det),
    }
}

fn configure_maximum(det: &Detection) -> Result<ChannelConfig> {
    let npm_scope = prompts::input("npm scope (e.g., @my-org)")?;
    let default_tap = format!("{}/homebrew-{}", det.owner, det.repo_name);
    let tap = prompts::input_default("Homebrew tap", &default_tap)?;

    Ok(ChannelConfig {
        install_scripts: true,
        pypi_name: Some(det.name.clone()),
        npm_scope: Some(npm_scope),
        homebrew_tap: Some(tap),
        cargo_crate: Some(det.name.clone()),
    })
}

fn configure_rust(det: &Detection) -> ChannelConfig {
    ChannelConfig {
        install_scripts: true,
        pypi_name: None,
        npm_scope: None,
        homebrew_tap: None,
        cargo_crate: Some(det.name.clone()),
    }
}

fn configure_minimal() -> ChannelConfig {
    ChannelConfig {
        install_scripts: true,
        pypi_name: None,
        npm_scope: None,
        homebrew_tap: None,
        cargo_crate: None,
    }
}

fn configure_custom(det: &Detection) -> Result<ChannelConfig> {
    eprintln!("  Select channels:");
    eprintln!();
    eprintln!("    GitHub Releases        [always on]");

    let install = prompts::ask_yn("Install scripts (curl|sh + irm|iex)", true)?;

    let pypi = if prompts::ask_yn("PyPI (pip install)", true)? {
        Some(det.name.clone())
    } else {
        None
    };

    let npm = if prompts::ask_yn("npm (npm install)", true)? {
        Some(prompts::input("npm scope (e.g., @my-org)")?)
    } else {
        None
    };

    let homebrew = if prompts::ask_yn("Homebrew tap", true)? {
        let default = format!("{}/homebrew-{}", det.owner, det.repo_name);
        Some(prompts::input_default("  tap repo", &default)?)
    } else {
        None
    };

    let cargo = if prompts::ask_yn("crates.io (cargo install)", true)? {
        Some(det.name.clone())
    } else {
        None
    };

    eprintln!("    cargo-binstall         [always on]");

    Ok(ChannelConfig {
        install_scripts: install,
        pypi_name: pypi,
        npm_scope: npm,
        homebrew_tap: homebrew,
        cargo_crate: cargo,
    })
}

// --- Build Config ---

pub fn build_config(det: &Detection, ch: &ChannelConfig) -> ReleaserConfig {
    let mut config = defaults::from_cargo(&det.cargo_meta);

    // Override repository with detected URL (git remote fallback)
    if config.package.repository.is_empty() && !det.repository.is_empty() {
        config.package.repository = det.repository.clone();
    }

    config.distribute.github = Some(GitHubConfig { release: true });

    // Populate binaries for multi-binary workspaces
    if det.all_binaries.len() > 1 {
        config.binaries = det.all_binaries.iter().map(|(name, pkg)| {
            crate::config::BinaryConfig {
                name: name.clone(),
                package: pkg.clone(),
            }
        }).collect();
    }

    if ch.install_scripts {
        config.distribute.install_script = Some(InstallScriptConfig { enabled: true });
    }
    if let Some(name) = &ch.pypi_name {
        config.distribute.pypi = Some(PyPIConfig { package_name: name.clone() });
    }
    if let Some(scope) = &ch.npm_scope {
        config.distribute.npm = Some(NpmConfig { scope: scope.clone(), package_name: None });
    }
    if let Some(tap) = &ch.homebrew_tap {
        config.distribute.homebrew = Some(HomebrewConfig { tap: tap.clone() });
    }
    if let Some(crate_name) = &ch.cargo_crate {
        config.distribute.cargo = Some(CargoConfig { crate_name: crate_name.clone() });
    }

    config
}

// --- Plan Actions ---

pub fn plan_actions(config: &ReleaserConfig, det: &Detection) -> Vec<String> {
    let mut actions = Vec::new();
    let channels = count_channels(config);
    let targets = config.targets.platforms.len();

    actions.push(format!("Write bincast.toml ({channels} channels, {targets} targets)"));
    actions.push("Generate .github/workflows/release.yml".into());

    if config.distribute.install_script.as_ref().is_some_and(|s| s.enabled) {
        actions.push("Generate install.sh + install.ps1".into());
    }
    if config.distribute.homebrew.is_some() {
        actions.push(format!("Generate homebrew/{}.rb", config.package.name));
    }
    if let Some(hb) = &config.distribute.homebrew
        && det.gh_available
    {
        actions.push(format!("Create repo {} (private)", hb.tap));
    }
    actions.push("Check name availability".into());
    actions.push("git add + commit".into());

    actions
}

fn count_channels(config: &ReleaserConfig) -> usize {
    let mut n = 1; // GitHub always
    if config.distribute.install_script.as_ref().is_some_and(|s| s.enabled) { n += 1; }
    if config.distribute.pypi.is_some() { n += 1; }
    if config.distribute.npm.is_some() { n += 1; }
    if config.distribute.homebrew.is_some() { n += 1; }
    if config.distribute.cargo.is_some() { n += 1; }
    n += 1; // binstall always
    n
}

// --- Execute Helpers ---

pub fn create_repo_if_needed(repo: &str, gh_available: &bool) {
    if !gh_available {
        eprintln!("  ! gh CLI not found — create {repo} manually on GitHub");
        return;
    }

    // Check if repo exists
    let exists = Command::new("gh")
        .args(["repo", "view", repo, "--json", "name"])
        .output()
        .is_ok_and(|o| o.status.success());

    if exists {
        eprintln!("  ✓ Repo {repo} already exists");
        return;
    }

    let output = Command::new("gh")
        .args(["repo", "create", repo, "--private", "--description", "Managed by bincast"])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            eprintln!("  ✓ Created {repo} (private)");
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            eprintln!("  ! Failed to create {repo}: {stderr}");
        }
        Err(e) => {
            eprintln!("  ! Failed to create {repo}: {e}");
        }
    }
}

pub fn check_names(config: &ReleaserConfig) {
    use crate::http::{self, Registry};

    if let Some(pypi) = &config.distribute.pypi {
        match http::check_name(Registry::PyPI, &pypi.package_name) {
            Ok(true) => eprintln!("  ✓ PyPI: '{}' is available", pypi.package_name),
            Ok(false) => eprintln!("  ! PyPI: '{}' is already taken", pypi.package_name),
            Err(e) => eprintln!("  ! PyPI check failed: {e}"),
        }
    }
    if let Some(npm) = &config.distribute.npm {
        let full = format!("{}/{}", npm.scope, config.package.name);
        match http::check_name(Registry::Npm, &full) {
            Ok(true) => eprintln!("  ✓ npm: '{full}' is available"),
            Ok(false) => eprintln!("  ! npm: '{full}' is already taken"),
            Err(e) => eprintln!("  ! npm check failed: {e}"),
        }
    }
    if let Some(cargo) = &config.distribute.cargo {
        match http::check_name(Registry::CratesIo, &cargo.crate_name) {
            Ok(true) => eprintln!("  ✓ crates.io: '{}' is available", cargo.crate_name),
            Ok(false) => eprintln!("  ! crates.io: '{}' is already taken", cargo.crate_name),
            Err(e) => eprintln!("  ! crates.io check failed: {e}"),
        }
    }
}

pub fn git_commit(project_dir: &Path) {
    // Check if we're in a git repo
    let in_git = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(project_dir)
        .output()
        .is_ok_and(|o| o.status.success());

    if !in_git {
        eprintln!("  ! Not a git repo — skipping commit");
        return;
    }

    let _ = Command::new("git")
        .args(["add", "bincast.toml", ".github/", "install.sh", "install.ps1", "binstall.toml", "homebrew/"])
        .current_dir(project_dir)
        .output();

    let output = Command::new("git")
        .args(["commit", "-m", "Add bincast release infrastructure"])
        .current_dir(project_dir)
        .output();

    match output {
        Ok(o) if o.status.success() => {
            eprintln!("  ✓ git commit: \"Add bincast release infrastructure\"");
        }
        _ => {
            eprintln!("  ! git commit skipped (nothing to commit or working tree issue)");
        }
    }
}

// --- Secrets ---

struct SecretInfo {
    name: &'static str,
    url: &'static str,
    instructions: String,
}

pub fn handle_secrets(config: &ReleaserConfig, det: &Detection) {
    let mut secrets_needed: Vec<SecretInfo> = Vec::new();

    eprintln!("  Secrets:");
    eprintln!("    ✓ GITHUB_TOKEN — automatic in GitHub Actions");

    if config.distribute.cargo.is_some() {
        secrets_needed.push(SecretInfo {
            name: "CARGO_REGISTRY_TOKEN",
            url: "https://crates.io/settings/tokens",
            instructions: "    1. Verify your email at: https://crates.io/settings/profile\n    2. Create token at: https://crates.io/settings/tokens\n       Scopes: publish-new, publish-update".into(),
        });
    }
    if config.distribute.pypi.is_some() {
        secrets_needed.push(SecretInfo {
            name: "PYPI_TOKEN",
            url: "https://pypi.org/manage/account/token/",
            instructions: "    Create at: https://pypi.org/manage/account/token/\n    Scope: Entire account (or project-scoped)".into(),
        });
    }
    if config.distribute.npm.is_some() {
        secrets_needed.push(SecretInfo {
            name: "NPM_TOKEN",
            url: "https://www.npmjs.com/settings/~/tokens",
            instructions: "    Create at: https://www.npmjs.com/settings/~/tokens/granular-access-tokens/new\n    Type: Granular Access Token\n    Packages: Read and write\n    Organizations: No access\n    Note: npm recommends Trusted Publishing (OIDC) for CI — see npm docs".into(),
        });
    }
    if let Some(hb) = &config.distribute.homebrew {
        secrets_needed.push(SecretInfo {
            name: "TAP_GITHUB_TOKEN",
            url: "https://github.com/settings/personal-access-tokens/new",
            instructions: format!(
                "    Create at: https://github.com/settings/personal-access-tokens/new\n\n    Fine-grained personal access token:\n      Repository access: Only select repositories -> {}\n      Permissions:\n        Contents: Read and write\n        Metadata: Read-only (auto-selected)",
                hb.tap
            ),
        });
    }

    if secrets_needed.is_empty() {
        eprintln!("    No additional secrets needed.");
        return;
    }

    for secret in &secrets_needed {
        eprintln!("    ! {} — {}", secret.name, secret.url);
    }

    if !det.gh_available {
        eprintln!();
        eprintln!("  Set secrets manually at: https://github.com/{}/{}/settings/secrets/actions",
            det.owner, det.repo_name);
        return;
    }

    eprintln!();
    let repo = format!("{}/{}", det.owner, det.repo_name);

    for secret in &secrets_needed {
        let name = secret.name;
        let _url = secret.url;
        if !prompts::can_prompt() {
            break;
        }

        eprintln!("  {name}:");
        eprintln!("{}", secret.instructions);
        eprintln!();

        let set_now = prompts::confirm(
            &format!("  Set {name} now?"),
            false,
        );

        match set_now {
            Ok(true) => {
                match prompts::password(&format!("Paste {name} (hidden)")) {
                    Ok(token) if !token.is_empty() => {
                        let output = Command::new("gh")
                            .args(["secret", "set", name, "--body", &token, "--repo", &repo])
                            .output();

                        match output {
                            Ok(o) if o.status.success() => {
                                eprintln!("  ✓ Set secret {name} for {repo}");
                            }
                            _ => {
                                eprintln!("  ! Failed to set {name} — set it manually in repo settings");
                            }
                        }
                    }
                    _ => {
                        eprintln!("  ! Skipped {name}");
                    }
                }
            }
            _ => {
                eprintln!("  Skipped {name} — set it later at:");
                eprintln!("    gh secret set {name} --repo {repo}");
            }
        }
    }
}
