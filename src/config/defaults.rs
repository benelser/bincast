use crate::cargo::CargoMetadata;
use super::types::*;

/// Derive a ReleaserConfig with sensible defaults from Cargo.toml metadata.
/// Channels are left empty — the user enables them via `releaser init`.
pub fn from_cargo(cargo: &CargoMetadata) -> ReleaserConfig {
    let binary = cargo.binary.clone().unwrap_or_else(|| cargo.name.clone());

    ReleaserConfig {
        package: PackageConfig {
            name: cargo.name.clone(),
            binary,
            description: cargo.description.clone(),
            repository: cargo.repository.clone().unwrap_or_default(),
            license: cargo.license.clone(),
            homepage: cargo.homepage.clone(),
            readme: cargo.readme.clone(),
            workspace_package: cargo.package_flag.clone(),
        },
        targets: TargetsConfig {
            platforms: default_platforms(),
        },
        distribute: DistributeConfig::default(),
        binaries: Vec::new(),
    }
}

/// Default platform targets — the most common set.
fn default_platforms() -> Vec<TargetTriple> {
    [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "aarch64-unknown-linux-gnu",
        "x86_64-unknown-linux-gnu",
        "x86_64-unknown-linux-musl",
        "x86_64-pc-windows-msvc",
    ]
    .iter()
    .map(|t| TargetTriple::new(t).unwrap())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_from_cargo() {
        let cargo = CargoMetadata {
            name: "my-tool".into(),
            version: "0.1.0".into(),
            description: Some("A great tool".into()),
            repository: Some("https://github.com/user/my-tool".into()),
            license: Some("MIT".into()),
            homepage: None,
            readme: None,
            binary: None,
            package_flag: None,
            is_workspace: false,
        };

        let config = from_cargo(&cargo);
        assert_eq!(config.package.name, "my-tool");
        assert_eq!(config.package.binary, "my-tool");
        assert_eq!(config.package.description.as_deref(), Some("A great tool"));
        assert_eq!(config.targets.platforms.len(), 6);
        assert!(config.distribute.github.is_none());
        assert!(config.distribute.pypi.is_none());
    }

    #[test]
    fn test_defaults_uses_explicit_binary() {
        let cargo = CargoMetadata {
            name: "my-tool-lib".into(),
            version: "0.1.0".into(),
            description: None,
            repository: Some("https://github.com/user/my-tool".into()),
            license: None,
            homepage: None,
            readme: None,
            binary: Some("my-tool".into()),
            package_flag: None,
            is_workspace: false,
        };

        let config = from_cargo(&cargo);
        assert_eq!(config.package.name, "my-tool-lib");
        assert_eq!(config.package.binary, "my-tool");
    }
}
