//! The `releaser init` command — reads Cargo.toml and generates releaser.toml.

use std::path::Path;

use crate::cargo;
use crate::config::defaults;
use crate::error::Result;

/// Run init: read Cargo.toml from the given directory, generate releaser.toml.
/// In non-interactive mode, enables GitHub Releases by default.
pub fn run(project_dir: &Path) -> Result<String> {
    let cargo_path = project_dir.join("Cargo.toml");
    if !cargo_path.exists() {
        return Err(crate::error::Error::Config(
            "no Cargo.toml found in current directory".into(),
        ));
    }

    let meta = cargo::read(&cargo_path)?;
    let mut config = defaults::from_cargo(&meta);

    // Enable GitHub Releases by default
    config.distribute.github = Some(crate::config::GitHubConfig { release: true });

    // Enable install scripts by default
    config.distribute.install_script = Some(crate::config::InstallScriptConfig { enabled: true });

    let toml = serialize_config(&config);
    Ok(toml)
}

/// Serialize a ReleaserConfig to TOML format.
fn serialize_config(config: &crate::config::ReleaserConfig) -> String {
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
        let dir = std::env::temp_dir().join(format!("releaser-init-test-{ts}"));
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

        // The generated TOML should be parseable back
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

        // Parse the generated TOML
        let config = crate::config::parse(&toml_str).unwrap();

        // Re-serialize
        let toml_str2 = serialize_config(&config);

        // Parse again
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
        let dir = std::env::temp_dir().join("releaser-init-empty");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let result = run(&dir);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&dir);
    }
}
