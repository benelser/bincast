//! The `bincast init` command — reads Cargo.toml and generates releaser.toml.
//! Profile-based wizard with smart defaults.

use std::io::{self, BufRead, Write};
use std::path::Path;

use crate::cargo::{self, CargoMetadata};
use crate::config::defaults;
use crate::error::Result;

// --- Distribution profiles ---

#[derive(Debug, Clone, Copy)]
enum Profile {
    MaximumReach,
    RustEcosystem,
    Minimal,
    Custom,
}

/// Run init: read Cargo.toml, present wizard, generate releaser.toml.
pub fn run(project_dir: &Path) -> Result<String> {
    let cargo_path = project_dir.join("Cargo.toml");
    if !cargo_path.exists() {
        return Err(crate::error::Error::Config(
            "no Cargo.toml found in current directory".into(),
        ));
    }

    let meta = cargo::read(&cargo_path)?;
    let mut config = defaults::from_cargo(&meta);

    // Always enabled
    config.distribute.github = Some(crate::config::GitHubConfig { release: true });

    if is_interactive() {
        wizard(&mut config, &meta)?;
    } else {
        // Non-interactive: minimal profile
        config.distribute.install_script = Some(crate::config::InstallScriptConfig { enabled: true });
    }

    let toml = serialize_config(&config);
    Ok(toml)
}

fn wizard(config: &mut crate::config::ReleaserConfig, meta: &CargoMetadata) -> Result<()> {
    let stdin = io::stdin();
    let mut r = stdin.lock();
    let name = &meta.name;

    let owner = cargo::parse_github_url(&config.package.repository)
        .map(|(o, _)| o.to_string())
        .unwrap_or_else(|| "user".to_string());

    eprintln!();
    eprintln!("  bincast v{} — Ship your Rust binary everywhere", env!("CARGO_PKG_VERSION"));
    eprintln!();
    eprintln!("  Detected: {name} v{} (from Cargo.toml)", meta.version);
    eprintln!("  Repository: {}", config.package.repository);
    eprintln!();

    // Step 1: Choose profile
    eprintln!("  Distribution profile:");
    eprintln!("    1. Maximum Reach — pip, npm, brew, scoop, cargo, curl, irm");
    eprintln!("    2. Rust Ecosystem — cargo, binstall, curl, irm");
    eprintln!("    3. Minimal — GitHub Releases + install scripts");
    eprintln!("    4. Custom");
    eprintln!();

    let profile = ask_choice(&mut r, "  Choose [1-4]", 3, &[1, 2, 3, 4])?;
    let profile = match profile {
        1 => Profile::MaximumReach,
        2 => Profile::RustEcosystem,
        3 => Profile::Minimal,
        4 => Profile::Custom,
        _ => Profile::Minimal,
    };

    eprintln!();

    match profile {
        Profile::MaximumReach => apply_maximum_reach(config, &mut r, name, &owner)?,
        Profile::RustEcosystem => apply_rust_ecosystem(config, name),
        Profile::Minimal => apply_minimal(config),
        Profile::Custom => apply_custom(config, &mut r, name, &owner)?,
    }

    // Step 2: Target customization
    eprintln!();
    eprintln!("  Targets:");
    for t in &config.targets.platforms {
        eprintln!("    + {t}");
    }
    eprint!("  [enter to accept, c to customize]: ");
    io::stderr().flush().ok();
    let mut line = String::new();
    r.read_line(&mut line).map_err(crate::error::Error::Io)?;
    if line.trim() == "c" {
        customize_targets(config, &mut r)?;
    }

    // Summary
    let channel_count = count_channels(config);
    let target_count = config.targets.platforms.len();
    eprintln!();
    eprintln!("  ✓ Created releaser.toml ({channel_count} channels, {target_count} targets)");
    eprintln!();
    eprintln!("  Next steps:");
    eprintln!("    bincast generate    # create CI workflow + install scripts");
    eprintln!("    bincast check       # validate everything");
    eprintln!();

    Ok(())
}

fn apply_maximum_reach(
    config: &mut crate::config::ReleaserConfig,
    r: &mut impl BufRead,
    name: &str,
    owner: &str,
) -> Result<()> {
    // All channels enabled — just need channel-specific config
    config.distribute.install_script = Some(crate::config::InstallScriptConfig { enabled: true });

    config.distribute.pypi = Some(crate::config::PyPIConfig {
        package_name: name.to_string(),
    });

    let scope = ask_value(r, "  npm scope (e.g., @my-org)")?;
    config.distribute.npm = Some(crate::config::NpmConfig {
        scope,
        package_name: None,
    });

    let default_tap = format!("{owner}/homebrew-{name}");
    let tap = ask_value_default(r, "  Homebrew tap", &default_tap)?;
    config.distribute.homebrew = Some(crate::config::HomebrewConfig { tap });

    let default_bucket = format!("{owner}/scoop-{name}");
    let bucket = ask_value_default(r, "  Scoop bucket", &default_bucket)?;
    config.distribute.scoop = Some(crate::config::ScoopConfig { bucket });

    config.distribute.cargo = Some(crate::config::CargoConfig {
        crate_name: name.to_string(),
    });

    Ok(())
}

fn apply_rust_ecosystem(config: &mut crate::config::ReleaserConfig, name: &str) {
    config.distribute.install_script = Some(crate::config::InstallScriptConfig { enabled: true });
    config.distribute.cargo = Some(crate::config::CargoConfig {
        crate_name: name.to_string(),
    });
}

fn apply_minimal(config: &mut crate::config::ReleaserConfig) {
    config.distribute.install_script = Some(crate::config::InstallScriptConfig { enabled: true });
}

fn apply_custom(
    config: &mut crate::config::ReleaserConfig,
    r: &mut impl BufRead,
    name: &str,
    owner: &str,
) -> Result<()> {
    eprintln!("  Select channels:");
    eprintln!();
    eprintln!("    GitHub Releases        [always on]");

    // Install scripts
    let install = ask_yn(r, "    Install scripts        ", true)?;
    config.distribute.install_script = Some(crate::config::InstallScriptConfig { enabled: install });

    // PyPI
    if ask_yn(r, "    PyPI (pip install)      ", true)? {
        config.distribute.pypi = Some(crate::config::PyPIConfig {
            package_name: name.to_string(),
        });
    }

    // npm
    if ask_yn(r, "    npm (npm install)       ", true)? {
        let scope = ask_value(r, "      npm scope")?;
        config.distribute.npm = Some(crate::config::NpmConfig {
            scope,
            package_name: None,
        });
    }

    // Homebrew
    if ask_yn(r, "    Homebrew tap            ", true)? {
        let default_tap = format!("{owner}/homebrew-{name}");
        let tap = ask_value_default(r, "      tap repo", &default_tap)?;
        config.distribute.homebrew = Some(crate::config::HomebrewConfig { tap });
    }

    // Scoop
    if ask_yn(r, "    Scoop bucket           ", true)? {
        let default_bucket = format!("{owner}/scoop-{name}");
        let bucket = ask_value_default(r, "      bucket repo", &default_bucket)?;
        config.distribute.scoop = Some(crate::config::ScoopConfig { bucket });
    }

    // crates.io
    if ask_yn(r, "    crates.io              ", true)? {
        config.distribute.cargo = Some(crate::config::CargoConfig {
            crate_name: name.to_string(),
        });
    }

    eprintln!("    cargo-binstall         [always on]");

    Ok(())
}

fn customize_targets(
    config: &mut crate::config::ReleaserConfig,
    r: &mut impl BufRead,
) -> Result<()> {
    let all_targets = [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "aarch64-unknown-linux-gnu",
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-musl",
        "x86_64-unknown-linux-musl",
        "x86_64-pc-windows-msvc",
        "aarch64-pc-windows-msvc",
        "i686-unknown-linux-gnu",
        "i686-pc-windows-msvc",
        "armv7-unknown-linux-gnueabihf",
    ];

    let current: Vec<String> = config.targets.platforms.iter().map(|t| t.to_string()).collect();
    let mut selected = Vec::new();

    eprintln!();
    for target in &all_targets {
        let is_default = current.iter().any(|t| t == *target);
        if ask_yn(r, &format!("    {target}"), is_default)? {
            selected.push(crate::config::TargetTriple::new(target).unwrap());
        }
    }

    if selected.is_empty() {
        eprintln!("  ! No targets selected — keeping defaults");
    } else {
        config.targets.platforms = selected;
    }

    Ok(())
}

fn count_channels(config: &crate::config::ReleaserConfig) -> usize {
    let mut n = 1; // GitHub Releases always
    if config.distribute.install_script.as_ref().is_some_and(|s| s.enabled) { n += 1; }
    if config.distribute.pypi.is_some() { n += 1; }
    if config.distribute.npm.is_some() { n += 1; }
    if config.distribute.homebrew.is_some() { n += 1; }
    if config.distribute.scoop.is_some() { n += 1; }
    if config.distribute.cargo.is_some() { n += 1; }
    n += 1; // cargo-binstall always
    n
}

// --- Prompt helpers ---

fn ask_choice(r: &mut impl BufRead, prompt: &str, default: u32, valid: &[u32]) -> Result<u32> {
    eprint!("{prompt}: ");
    io::stderr().flush().ok();
    let mut line = String::new();
    r.read_line(&mut line).map_err(crate::error::Error::Io)?;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(default);
    }
    match trimmed.parse::<u32>() {
        Ok(n) if valid.contains(&n) => Ok(n),
        _ => {
            eprintln!("  Invalid choice, using default ({default})");
            Ok(default)
        }
    }
}

fn ask_yn(r: &mut impl BufRead, label: &str, default: bool) -> Result<bool> {
    let hint = if default { "[Y/n]" } else { "[y/N]" };
    eprint!("{label}{hint}: ");
    io::stderr().flush().ok();
    let mut line = String::new();
    r.read_line(&mut line).map_err(crate::error::Error::Io)?;
    let answer = line.trim().to_lowercase();
    if answer.is_empty() {
        Ok(default)
    } else {
        Ok(answer == "y" || answer == "yes")
    }
}

fn ask_value(r: &mut impl BufRead, prompt: &str) -> Result<String> {
    eprint!("{prompt}: ");
    io::stderr().flush().ok();
    let mut line = String::new();
    r.read_line(&mut line).map_err(crate::error::Error::Io)?;
    Ok(line.trim().to_string())
}

fn ask_value_default(r: &mut impl BufRead, prompt: &str, default: &str) -> Result<String> {
    eprint!("{prompt} [{default}]: ");
    io::stderr().flush().ok();
    let mut line = String::new();
    r.read_line(&mut line).map_err(crate::error::Error::Io)?;
    let value = line.trim();
    if value.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(value.to_string())
    }
}

fn is_interactive() -> bool {
    atty_stdin()
}

#[cfg(unix)]
fn atty_stdin() -> bool {
    unsafe { libc_isatty(0) != 0 }
}

#[cfg(unix)]
unsafe fn libc_isatty(fd: i32) -> i32 {
    unsafe extern "C" {
        safe fn isatty(fd: i32) -> i32;
    }
    isatty(fd)
}

#[cfg(not(unix))]
fn atty_stdin() -> bool {
    false
}

// --- Serialization ---

/// Serialize a ReleaserConfig to TOML format.
pub fn serialize_config(config: &crate::config::ReleaserConfig) -> String {
    let mut out = String::new();

    out.push_str("[package]\n");
    out.push_str(&format!("name = \"{}\"\n", config.package.name));
    out.push_str(&format!("binary = \"{}\"\n", config.package.binary));
    if let Some(desc) = &config.package.description {
        out.push_str(&format!("description = \"{desc}\"\n"));
    }
    out.push_str(&format!("repository = \"{}\"\n", config.package.repository));
    if let Some(license) = &config.package.license {
        out.push_str(&format!("license = \"{license}\"\n"));
    }
    if let Some(homepage) = &config.package.homepage {
        out.push_str(&format!("homepage = \"{homepage}\"\n"));
    }

    out.push_str("\n[targets]\nplatforms = [\n");
    for target in &config.targets.platforms {
        out.push_str(&format!("  \"{target}\",\n"));
    }
    out.push_str("]\n");

    if let Some(gh) = &config.distribute.github {
        out.push_str(&format!("\n[distribute.github]\nrelease = {}\n", gh.release));
    }

    if let Some(pypi) = &config.distribute.pypi {
        out.push_str(&format!(
            "\n[distribute.pypi]\npackage_name = \"{}\"\n",
            pypi.package_name
        ));
    }

    if let Some(npm) = &config.distribute.npm {
        out.push_str(&format!(
            "\n[distribute.npm]\nscope = \"{}\"\n",
            npm.scope
        ));
        if let Some(pkg) = &npm.package_name {
            out.push_str(&format!("package_name = \"{pkg}\"\n"));
        }
    }

    if let Some(homebrew) = &config.distribute.homebrew {
        out.push_str(&format!(
            "\n[distribute.homebrew]\ntap = \"{}\"\n",
            homebrew.tap
        ));
    }

    if let Some(scoop) = &config.distribute.scoop {
        out.push_str(&format!(
            "\n[distribute.scoop]\nbucket = \"{}\"\n",
            scoop.bucket
        ));
    }

    if let Some(cargo) = &config.distribute.cargo {
        out.push_str(&format!(
            "\n[distribute.cargo]\ncrate_name = \"{}\"\n",
            cargo.crate_name
        ));
    }

    if let Some(install) = &config.distribute.install_script {
        out.push_str(&format!(
            "\n[distribute.install_script]\nenabled = {}\n",
            install.enabled
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_fixture_project() -> std::path::PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("bincast-init-test-{ts}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            r#"
[package]
name = "my-tool"
version = "0.1.0"
edition = "2024"
description = "A cool tool"
license = "MIT"
repository = "https://github.com/user/my-tool"
"#,
        )
        .unwrap();
        dir
    }

    #[test]
    fn test_init_generates_valid_toml() {
        let dir = create_fixture_project();
        let toml_str = run(&dir).unwrap();

        let config = crate::config::parse(&toml_str).unwrap();
        assert_eq!(config.package.name, "my-tool");
        assert_eq!(config.package.binary, "my-tool");
        assert_eq!(config.package.description.as_deref(), Some("A cool tool"));
        assert_eq!(config.package.license.as_deref(), Some("MIT"));
        assert_eq!(config.targets.platforms.len(), 6);
        assert!(config.distribute.github.is_some());
        assert!(config.distribute.install_script.is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_init_round_trips() {
        let dir = create_fixture_project();
        let toml_str = run(&dir).unwrap();
        let config = crate::config::parse(&toml_str).unwrap();
        let toml_str2 = serialize_config(&config);
        let config2 = crate::config::parse(&toml_str2).unwrap();

        assert_eq!(config.package.name, config2.package.name);
        assert_eq!(config.package.binary, config2.package.binary);
        assert_eq!(config.targets.platforms.len(), config2.targets.platforms.len());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_serialize_round_trip_all_channels() {
        use crate::config::*;

        let config = ReleaserConfig {
            package: PackageConfig {
                name: "durable".into(),
                binary: "durable".into(),
                description: Some("The SQLite of durable agent execution".into()),
                repository: "https://github.com/benelser/durable".into(),
                license: Some("MIT".into()),
                homepage: Some("https://durable.dev".into()),
                readme: None,
            },
            targets: TargetsConfig {
                platforms: vec![
                    TargetTriple::new("aarch64-apple-darwin").unwrap(),
                    TargetTriple::new("x86_64-unknown-linux-gnu").unwrap(),
                    TargetTriple::new("x86_64-pc-windows-msvc").unwrap(),
                ],
            },
            distribute: DistributeConfig {
                github: Some(GitHubConfig { release: true }),
                pypi: Some(PyPIConfig { package_name: "durable".into() }),
                npm: Some(NpmConfig { scope: "@durable".into(), package_name: Some("cli".into()) }),
                homebrew: Some(HomebrewConfig { tap: "benelser/homebrew-durable".into() }),
                scoop: Some(ScoopConfig { bucket: "benelser/scoop-durable".into() }),
                cargo: Some(CargoConfig { crate_name: "durable-runtime".into() }),
                install_script: Some(InstallScriptConfig { enabled: true }),
            },
        };

        let toml_str = serialize_config(&config);
        let parsed = crate::config::parse(&toml_str).unwrap();

        assert_eq!(parsed.package.name, "durable");
        assert_eq!(parsed.package.homepage.as_deref(), Some("https://durable.dev"));
        assert_eq!(parsed.targets.platforms.len(), 3);
        assert_eq!(parsed.distribute.pypi.as_ref().unwrap().package_name, "durable");
        assert_eq!(parsed.distribute.npm.as_ref().unwrap().scope, "@durable");
        assert_eq!(parsed.distribute.npm.as_ref().unwrap().package_name.as_deref(), Some("cli"));
        assert_eq!(parsed.distribute.homebrew.as_ref().unwrap().tap, "benelser/homebrew-durable");
        assert_eq!(parsed.distribute.scoop.as_ref().unwrap().bucket, "benelser/scoop-durable");
        assert_eq!(parsed.distribute.cargo.as_ref().unwrap().crate_name, "durable-runtime");
        assert!(parsed.distribute.install_script.as_ref().unwrap().enabled);
    }

    #[test]
    fn test_init_no_cargo_toml_errors() {
        let dir = std::env::temp_dir().join("bincast-init-empty");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let result = run(&dir);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_count_channels_minimal() {
        let config = crate::config::parse(r#"
[package]
name = "t"
binary = "t"
repository = "https://github.com/u/t"
[targets]
platforms = ["x86_64-unknown-linux-gnu"]
[distribute.github]
release = true
[distribute.install_script]
enabled = true
"#).unwrap();
        assert_eq!(count_channels(&config), 3); // github + install_script + binstall
    }

    #[test]
    fn test_count_channels_maximum() {
        let config = crate::config::parse(r#"
[package]
name = "t"
binary = "t"
repository = "https://github.com/u/t"
[targets]
platforms = ["x86_64-unknown-linux-gnu"]
[distribute.github]
release = true
[distribute.pypi]
package_name = "t"
[distribute.npm]
scope = "@t"
[distribute.homebrew]
tap = "u/homebrew-t"
[distribute.scoop]
bucket = "u/scoop-t"
[distribute.cargo]
crate_name = "t"
[distribute.install_script]
enabled = true
"#).unwrap();
        assert_eq!(count_channels(&config), 8); // all 8
    }
}
