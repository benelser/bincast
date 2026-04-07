//! The `releaser check` command — validates config and checks name availability.

use crate::config::ReleaserConfig;
use crate::http::{self, Registry};
use crate::pipeline::{Context, DryRunEntry, Pipe, Pipeline};

/// Build the check pipeline for a given config.
pub fn build_pipeline(config: &ReleaserConfig) -> Pipeline {
    Pipeline::new()
        .push(Box::new(ConfigValidationPipe))
        .push_if(
            config.distribute.pypi.is_some(),
            Box::new(NameCheckPipe {
                registry: Registry::PyPI,
                name: config
                    .distribute
                    .pypi
                    .as_ref()
                    .map(|p| p.package_name.clone())
                    .unwrap_or_default(),
            }),
        )
        .push_if(
            config.distribute.npm.is_some(),
            Box::new(NameCheckPipe {
                registry: Registry::Npm,
                name: config
                    .distribute
                    .npm
                    .as_ref()
                    .map(|n| format!("{}/{}", n.scope, n.package_name.as_deref().unwrap_or(&config.package.name)))
                    .unwrap_or_default(),
            }),
        )
        .push_if(
            config.distribute.cargo.is_some(),
            Box::new(NameCheckPipe {
                registry: Registry::CratesIo,
                name: config
                    .distribute
                    .cargo
                    .as_ref()
                    .map(|c| c.crate_name.clone())
                    .unwrap_or_default(),
            }),
        )
}

/// Pipe that validates the config.
struct ConfigValidationPipe;

impl Pipe for ConfigValidationPipe {
    fn name(&self) -> &str {
        "config-validation"
    }

    fn run(&self, ctx: &mut Context) -> Result<(), String> {
        let config = ctx
            .config
            .as_ref()
            .ok_or_else(|| "no config loaded".to_string())?;

        let errors = crate::config::validate::validate(config);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn dry_run(&self, _ctx: &Context) -> DryRunEntry {
        DryRunEntry {
            pipe: "config-validation".into(),
            description: "would validate releaser.toml".into(),
        }
    }
}

/// Pipe that checks name availability on a registry.
struct NameCheckPipe {
    registry: Registry,
    name: String,
}

impl Pipe for NameCheckPipe {
    fn name(&self) -> &str {
        match self.registry {
            Registry::PyPI => "pypi-name-check",
            Registry::Npm => "npm-name-check",
            Registry::CratesIo => "crates-name-check",
        }
    }

    fn run(&self, _ctx: &mut Context) -> Result<(), String> {
        let registry_name = self.registry.display_name();
        match http::check_name(self.registry, &self.name) {
            Ok(true) => {
                eprintln!("  ✓ {registry_name}: '{}' is available", self.name);
                Ok(())
            }
            Ok(false) => {
                Err(format!(
                    "{registry_name}: '{}' is already taken",
                    self.name
                ))
            }
            Err(e) => {
                Err(format!("{registry_name}: check failed — {e}"))
            }
        }
    }

    fn dry_run(&self, _ctx: &Context) -> DryRunEntry {
        DryRunEntry {
            pipe: self.name().to_string(),
            description: format!(
                "would check if '{}' is available on {}",
                self.name,
                self.registry.display_name()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    fn full_config() -> ReleaserConfig {
        config::parse(
            r#"
[package]
name = "my-tool"
binary = "my-tool"
repository = "https://github.com/user/my-tool"

[targets]
platforms = ["x86_64-unknown-linux-gnu"]

[distribute.github]
release = true

[distribute.pypi]
package_name = "my-tool"

[distribute.npm]
scope = "@my-org"

[distribute.cargo]
crate_name = "my-tool-rs"
"#,
        )
        .unwrap()
    }

    #[test]
    fn test_build_pipeline_includes_all_checks() {
        let config = full_config();
        let pipeline = build_pipeline(&config);

        // Dry-run the pipeline to see what it would do
        let mut ctx = Context::with_config(config, true);
        let report = pipeline.execute(&mut ctx).unwrap();

        assert_eq!(report.dry_run_entries.len(), 4); // config + pypi + npm + crates
        assert!(report.dry_run_entries[0].pipe.contains("config"));
        assert!(report.dry_run_entries[1].pipe.contains("pypi"));
        assert!(report.dry_run_entries[2].pipe.contains("npm"));
        assert!(report.dry_run_entries[3].pipe.contains("crates"));
    }

    #[test]
    fn test_build_pipeline_skips_unconfigured() {
        let config = config::parse(
            r#"
[package]
name = "simple"
binary = "simple"
repository = "https://github.com/user/simple"

[targets]
platforms = ["x86_64-unknown-linux-gnu"]

[distribute.github]
release = true
"#,
        )
        .unwrap();

        let pipeline = build_pipeline(&config);
        let mut ctx = Context::with_config(config, true);
        let report = pipeline.execute(&mut ctx).unwrap();

        // Only config validation — no registry checks
        assert_eq!(report.dry_run_entries.len(), 1);
        assert!(report.dry_run_entries[0].pipe.contains("config"));
    }

    #[test]
    fn test_config_validation_pipe_catches_errors() {
        let mut config = full_config();
        config.package.repository = "https://gitlab.com/bad".into();

        let pipe = ConfigValidationPipe;
        let mut ctx = Context::with_config(config, false);
        let result = pipe.run(&mut ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be a GitHub URL"));
    }
}
