mod ci;
mod homebrew;
mod install;
pub mod receivers;
pub mod validate;

use std::fs;
use std::path::Path;

use crate::config::ReleaserConfig;
use crate::cargo;
use crate::error::Result;

/// Run the generate command: emit all distribution artifacts.
pub fn run(config: &ReleaserConfig, output_dir: &Path) -> Result<Vec<GeneratedFile>> {
    let mut files = Vec::new();

    let (owner, repo) = cargo::parse_github_url(&config.package.repository)
        .ok_or_else(|| crate::error::Error::Config(
            format!("cannot parse GitHub owner/repo from '{}'", config.package.repository)
        ))?;

    let ctx = GenerateContext {
        config,
        owner: owner.to_string(),
        repo: repo.to_string(),
    };

    // Always generate CI workflow
    let ci_content = ci::render(&ctx)?;
    files.push(GeneratedFile {
        path: ".github/workflows/release.yml".into(),
        content: ci_content,
    });

    // Install scripts
    if config.distribute.install_script.as_ref().is_some_and(|s| s.enabled) {
        let sh = install::render_sh(&ctx)?;
        files.push(GeneratedFile {
            path: "install.sh".into(),
            content: sh,
        });

        let ps1 = install::render_ps1(&ctx)?;
        files.push(GeneratedFile {
            path: "install.ps1".into(),
            content: ps1,
        });
    }

    // Homebrew formula
    if config.distribute.homebrew.is_some() {
        let formula = homebrew::render(&ctx)?;
        files.push(GeneratedFile {
            path: format!("homebrew/{}.rb", config.package.name),
            content: formula,
        });
    }

    // cargo-binstall metadata snippet
    let binstall = crate::package::binstall::binstall_metadata(
        &ctx.owner,
        &ctx.repo,
        &config.package.binary,
    );
    files.push(GeneratedFile {
        path: "binstall.toml".into(),
        content: format!(
            "# Add this to your Cargo.toml for cargo-binstall support:\n\n{binstall}"
        ),
    });

    // Write files to disk
    for file in &files {
        let full_path = output_dir.join(&file.path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&full_path, &file.content)?;
    }

    Ok(files)
}

pub struct GenerateContext<'a> {
    pub config: &'a ReleaserConfig,
    pub owner: String,
    pub repo: String,
}

pub struct GeneratedFile {
    pub path: String,
    pub content: String,
}
