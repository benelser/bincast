//! TOML serializer for ReleaserConfig.

use crate::config::ReleaserConfig;

pub fn serialize_config(config: &ReleaserConfig) -> String {
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
    if let Some(ws_pkg) = &config.package.workspace_package {
        out.push_str(&format!("workspace_package = \"{ws_pkg}\"\n"));
    }

    out.push_str("\n[targets]\nplatforms = [\n");
    for target in &config.targets.platforms {
        out.push_str(&format!("  \"{target}\",\n"));
    }
    out.push_str("]\n");

    // Multi-binary support
    if !config.binaries.is_empty() {
        for bin in &config.binaries {
            out.push_str("\n[[binaries]]\n");
            out.push_str(&format!("name = \"{}\"\n", bin.name));
            if let Some(pkg) = &bin.package {
                out.push_str(&format!("package = \"{pkg}\"\n"));
            }
        }
    }

    if let Some(gh) = &config.distribute.github {
        out.push_str(&format!("\n[distribute.github]\nrelease = {}\n", gh.release));
    }
    if let Some(pypi) = &config.distribute.pypi {
        out.push_str(&format!("\n[distribute.pypi]\npackage_name = \"{}\"\n", pypi.package_name));
    }
    if let Some(npm) = &config.distribute.npm {
        out.push_str(&format!("\n[distribute.npm]\nscope = \"{}\"\n", npm.scope));
        if let Some(pkg) = &npm.package_name {
            out.push_str(&format!("package_name = \"{pkg}\"\n"));
        }
    }
    if let Some(homebrew) = &config.distribute.homebrew {
        out.push_str(&format!("\n[distribute.homebrew]\ntap = \"{}\"\n", homebrew.tap));
    }
    if let Some(cargo) = &config.distribute.cargo {
        out.push_str(&format!("\n[distribute.cargo]\ncrate_name = \"{}\"\n", cargo.crate_name));
    }
    if let Some(install) = &config.distribute.install_script {
        out.push_str(&format!("\n[distribute.install_script]\nenabled = {}\n", install.enabled));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;

    #[test]
    fn test_serialize_round_trip_all_channels() {
        let config = ReleaserConfig {
            package: PackageConfig {
                name: "durable".into(),
                binary: "durable".into(),
                description: Some("The SQLite of durable agent execution".into()),
                repository: "https://github.com/benelser/durable".into(),
                license: Some("MIT".into()),
                homepage: Some("https://durable.dev".into()),
                readme: None,
                workspace_package: None,
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
                cargo: Some(CargoConfig { crate_name: "durable-runtime".into() }),
                install_script: Some(InstallScriptConfig { enabled: true }),
            },
            binaries: Vec::new(),
        };

        let toml_str = serialize_config(&config);
        let parsed = crate::config::parse(&toml_str).unwrap();

        assert_eq!(parsed.package.name, "durable");
        assert_eq!(parsed.package.homepage.as_deref(), Some("https://durable.dev"));
        assert_eq!(parsed.targets.platforms.len(), 3);
        assert_eq!(parsed.distribute.pypi.as_ref().unwrap().package_name, "durable");
        assert_eq!(parsed.distribute.npm.as_ref().unwrap().scope, "@durable");
        assert_eq!(parsed.distribute.homebrew.as_ref().unwrap().tap, "benelser/homebrew-durable");
        assert_eq!(parsed.distribute.cargo.as_ref().unwrap().crate_name, "durable-runtime");
        assert!(parsed.distribute.install_script.as_ref().unwrap().enabled);
    }

    #[test]
    fn test_serialize_minimal() {
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

        let toml_str = serialize_config(&config);
        let parsed = crate::config::parse(&toml_str).unwrap();
        assert_eq!(parsed.package.name, "t");
        assert!(parsed.distribute.github.is_some());
    }
}
