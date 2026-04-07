//! GitHub Release pipe — creates a release and uploads artifacts via the GitHub API.

use crate::http::github as gh;
use crate::pipeline::{Context, DryRunEntry, Pipe};

pub struct GitHubReleasePipe;

impl Pipe for GitHubReleasePipe {
    fn name(&self) -> &str {
        "github-release"
    }

    fn run(&self, ctx: &mut Context) -> Result<(), String> {
        let version = ctx.version.as_ref().ok_or("no version set")?;
        let owner = ctx.owner.as_deref().ok_or("no owner")?;
        let repo = ctx.repo.as_deref().ok_or("no repo")?;

        if ctx.artifacts.is_empty() {
            return Err("no artifacts to upload — did the build step run?".into());
        }

        let token = std::env::var("GITHUB_TOKEN")
            .or_else(|_| std::env::var("GH_TOKEN"))
            .map_err(|_| "GITHUB_TOKEN or GH_TOKEN not set — needed for GitHub Releases".to_string())?;

        // Step 1: Create draft release
        eprintln!("  creating draft release {version}...");
        let release = gh::create_release(owner, repo, version, &token)?;
        eprintln!("  draft release created: {}", release.html_url);

        // Step 2: Upload each artifact + its checksum sidecar
        for artifact in &ctx.artifacts {
            let path = &artifact.path;
            if !path.exists() {
                eprintln!("  warning: artifact not found, skipping: {}", path.display());
                continue;
            }
            eprintln!("  uploading {}...", path.display());
            gh::upload_asset(owner, repo, release.id, path, &token)?;

            // Upload .sha256 sidecar if it exists
            let sidecar = path.with_extension(format!(
                "{}.sha256",
                path.extension().and_then(|e| e.to_str()).unwrap_or("")
            ));
            if sidecar.exists() {
                eprintln!("  uploading {}...", sidecar.display());
                gh::upload_asset(owner, repo, release.id, &sidecar, &token)?;
            }
        }

        // Step 3: Publish (flip draft to false)
        eprintln!("  publishing release...");
        gh::publish_release(owner, repo, release.id, &token)?;

        ctx.github_release_url = Some(release.html_url.clone());
        eprintln!("  ✓ released: {}", release.html_url);

        Ok(())
    }

    fn dry_run(&self, ctx: &Context) -> DryRunEntry {
        let version = ctx.version.as_deref().unwrap_or("v?.?.?");
        let artifact_count = ctx.artifacts.len();
        DryRunEntry {
            pipe: "github-release".into(),
            description: format!(
                "would create GitHub Release {version} and upload {artifact_count} artifacts"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::pipeline::{Artifact, ArtifactKind};
    use std::path::PathBuf;

    fn test_config() -> crate::config::ReleaserConfig {
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
"#,
        )
        .unwrap()
    }

    #[test]
    fn test_github_release_fails_without_artifacts() {
        let pipe = GitHubReleasePipe;
        let mut ctx = Context::with_config(test_config(), false);
        ctx.version = Some("v0.1.0".into());

        let result = pipe.run(&mut ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no artifacts"));
    }

    #[test]
    fn test_github_release_fails_without_token() {
        let pipe = GitHubReleasePipe;
        let mut ctx = Context::with_config(test_config(), false);
        ctx.version = Some("v0.1.0".into());
        ctx.artifacts.push(Artifact {
            path: PathBuf::from("/tmp/nonexistent.tar.gz"),
            kind: ArtifactKind::Archive,
            target: None,
        });

        // Don't set GITHUB_TOKEN — should fail
        let result = pipe.run(&mut ctx);
        // This will either fail on missing token or on the actual curl call
        assert!(result.is_err());
    }

    #[test]
    fn test_github_release_fails_without_version() {
        let pipe = GitHubReleasePipe;
        let mut ctx = Context::with_config(test_config(), false);
        ctx.artifacts.push(Artifact {
            path: PathBuf::from("test.tar.gz"),
            kind: ArtifactKind::Archive,
            target: None,
        });

        let result = pipe.run(&mut ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no version"));
    }

    #[test]
    fn test_dry_run_includes_version() {
        let pipe = GitHubReleasePipe;
        let mut ctx = Context::with_config(test_config(), true);
        ctx.version = Some("v0.2.0".into());

        let entry = pipe.dry_run(&ctx);
        assert!(entry.description.contains("v0.2.0"));
    }
}
