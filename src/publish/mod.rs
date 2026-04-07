//! Publish pipeline — pipes for each distribution channel.
//! This is the real execution pipeline, not stubs.

pub mod build_pipe;
pub mod github;
pub mod readme;
pub mod smoke;

use crate::config::ReleaserConfig;
use crate::http::github as gh;
use crate::pipeline::{Context, DryRunEntry, Pipe, Pipeline};

/// Build the full publish pipeline for a release.
pub fn build_pipeline(config: &ReleaserConfig) -> Pipeline {
    Pipeline::new()
        // Phase 1: Build
        .push(Box::new(build_pipe::BuildPipe))
        // Phase 2: Package
        .push(Box::new(build_pipe::ArchivePipe))
        .push(Box::new(build_pipe::ChecksumPipe))
        // Phase 3: Verify
        .push(Box::new(smoke::SmokeTestPipe))
        // Phase 4: Publish (GitHub first — downstream needs release URLs)
        .push(Box::new(github::GitHubReleasePipe))
        .push_if(
            config.distribute.pypi.is_some(),
            Box::new(PyPIPublishPipe),
        )
        .push_if(
            config.distribute.npm.is_some(),
            Box::new(NpmPublishPipe),
        )
        .push_if(
            config.distribute.cargo.is_some(),
            Box::new(CratesPublishPipe),
        )
        .push_if(
            config.distribute.homebrew.is_some(),
            Box::new(HomebrewDispatchPipe),
        )
        .push_if(
            config.distribute.scoop.is_some(),
            Box::new(ScoopDispatchPipe),
        )
}

// --- Real publisher pipes ---

struct PyPIPublishPipe;
impl Pipe for PyPIPublishPipe {
    fn name(&self) -> &str { "publish-pypi" }
    fn run(&self, ctx: &mut Context) -> Result<(), String> {
        let config = ctx.config.as_ref().ok_or("no config")?;
        let pypi = config.distribute.pypi.as_ref().ok_or("pypi not configured")?;
        let token = std::env::var("PYPI_TOKEN")
            .map_err(|_| "PYPI_TOKEN not set — needed for PyPI publishing")?;

        // Find wheel artifacts or build them
        let wheels: Vec<_> = ctx.artifacts.iter()
            .filter(|a| a.kind == crate::pipeline::ArtifactKind::Wheel)
            .collect();

        if wheels.is_empty() {
            // Try building with maturin
            eprintln!("  building wheel with maturin...");
            let output = std::process::Command::new("maturin")
                .args(["build", "--release", "--bindings", "bin", "--strip"])
                .current_dir(&ctx.work_dir)
                .output()
                .map_err(|e| format!("maturin not found: {e}. Install with: pip install maturin"))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("maturin build failed:\n{stderr}"));
            }

            // Find the built wheel(s) in target/wheels/
            let wheels_dir = ctx.work_dir.join("target/wheels");
            if wheels_dir.exists() {
                for entry in std::fs::read_dir(&wheels_dir).map_err(|e| e.to_string())? {
                    let entry = entry.map_err(|e| e.to_string())?;
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("whl") {
                        ctx.artifacts.push(crate::pipeline::Artifact {
                            path: path.clone(),
                            kind: crate::pipeline::ArtifactKind::Wheel,
                            target: None,
                        });
                    }
                }
            }
        }

        let wheels: Vec<_> = ctx.artifacts.iter()
            .filter(|a| a.kind == crate::pipeline::ArtifactKind::Wheel)
            .collect();

        for wheel in &wheels {
            let path = wheel.path.to_str().ok_or("non-utf8 wheel path")?;
            eprintln!("  uploading {} to PyPI...", wheel.path.display());

            let output = std::process::Command::new("curl")
                .args([
                    "-s",
                    "-X", "POST",
                    "-u", &format!("__token__:{token}"),
                    "-F", ":action=file_upload",
                    "-F", "protocol_version=1",
                    &format!("-F content=@{path}"),
                    &std::env::var("RELEASER_PYPI_URL")
                        .unwrap_or_else(|_| "https://upload.pypi.org/legacy/".to_string()),
                ])
                .output()
                .map_err(|e| format!("curl failed: {e}"))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("PyPI upload failed: {stderr}"));
            }
        }

        eprintln!("  ✓ published to PyPI as '{}'", pypi.package_name);
        Ok(())
    }
    fn dry_run(&self, ctx: &Context) -> DryRunEntry {
        let name = ctx.config.as_ref()
            .and_then(|c| c.distribute.pypi.as_ref())
            .map(|p| p.package_name.as_str())
            .unwrap_or("unknown");
        DryRunEntry {
            pipe: "publish-pypi".into(),
            description: format!("would build wheel with maturin and publish '{name}' to PyPI"),
        }
    }
}

struct NpmPublishPipe;
impl Pipe for NpmPublishPipe {
    fn name(&self) -> &str { "publish-npm" }
    fn run(&self, ctx: &mut Context) -> Result<(), String> {
        let config = ctx.config.as_ref().ok_or("no config")?;
        let npm = config.distribute.npm.as_ref().ok_or("npm not configured")?;
        let version = ctx.version.as_ref().ok_or("no version")?;
        let version = version.strip_prefix('v').unwrap_or(version);
        let binary = &config.package.binary;

        // Check npm is available
        std::process::Command::new("npm").arg("--version")
            .output()
            .map_err(|_| "npm not found — install Node.js to publish npm packages")?;

        // Find binary artifact
        let bin_artifact = ctx.artifacts.iter()
            .find(|a| a.kind == crate::pipeline::ArtifactKind::Binary)
            .ok_or("no binary artifact — build step must run first")?;

        let target = bin_artifact.target.as_deref().unwrap_or("unknown");

        // Determine npm os/cpu from target
        let (npm_os, npm_cpu) = parse_npm_platform(target);

        // Create platform package directory
        let pkg_dir = ctx.work_dir.join(format!("dist/npm-{npm_os}-{npm_cpu}"));
        let bin_dir = pkg_dir.join("bin");
        std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;

        // Copy binary
        std::fs::copy(&bin_artifact.path, bin_dir.join(binary))
            .map_err(|e| format!("failed to copy binary: {e}"))?;

        // Write package.json
        let pkg_json = crate::package::npm::platform_package_json(
            &npm.scope, binary,
            &crate::config::TargetTriple::new(target).map_err(|e| e.to_string())?,
            version,
        );
        std::fs::write(pkg_dir.join("package.json"), pkg_json)
            .map_err(|e| e.to_string())?;

        // Publish
        eprintln!("  publishing {}/{binary}-{npm_os}-{npm_cpu}@{version}...", npm.scope);
        let output = std::process::Command::new("npm")
            .args(["publish", "--access", "public"])
            .current_dir(&pkg_dir)
            .output()
            .map_err(|e| format!("npm publish failed: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("npm publish failed:\n{stderr}"));
        }

        eprintln!("  ✓ published to npm");
        Ok(())
    }
    fn dry_run(&self, ctx: &Context) -> DryRunEntry {
        let scope = ctx.config.as_ref()
            .and_then(|c| c.distribute.npm.as_ref())
            .map(|n| n.scope.as_str())
            .unwrap_or("unknown");
        DryRunEntry {
            pipe: "publish-npm".into(),
            description: format!("would publish platform packages to npm under '{scope}'"),
        }
    }
}

struct CratesPublishPipe;
impl Pipe for CratesPublishPipe {
    fn name(&self) -> &str { "publish-crates" }
    fn run(&self, ctx: &mut Context) -> Result<(), String> {
        let config = ctx.config.as_ref().ok_or("no config")?;
        let cargo_config = config.distribute.cargo.as_ref().ok_or("cargo not configured")?;

        eprintln!("  publishing to crates.io as '{}'...", cargo_config.crate_name);
        let mut cmd = std::process::Command::new("cargo");
        cmd.args(["publish", "--no-verify"]);
        if let Some(pkg) = &config.package.workspace_package {
            cmd.args(["-p", pkg]);
        }
        let output = cmd.current_dir(&ctx.work_dir)
            .output()
            .map_err(|e| format!("cargo publish failed: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("cargo publish failed:\n{stderr}"));
        }

        eprintln!("  ✓ published to crates.io");
        Ok(())
    }
    fn dry_run(&self, ctx: &Context) -> DryRunEntry {
        let name = ctx.config.as_ref()
            .and_then(|c| c.distribute.cargo.as_ref())
            .map(|c| c.crate_name.as_str())
            .unwrap_or("unknown");
        DryRunEntry {
            pipe: "publish-crates".into(),
            description: format!("would run cargo publish for '{name}'"),
        }
    }
}

struct HomebrewDispatchPipe;
impl Pipe for HomebrewDispatchPipe {
    fn name(&self) -> &str { "dispatch-homebrew" }
    fn run(&self, ctx: &mut Context) -> Result<(), String> {
        let config = ctx.config.as_ref().ok_or("no config")?;
        let homebrew = config.distribute.homebrew.as_ref().ok_or("homebrew not configured")?;
        let version = ctx.version.as_ref().ok_or("no version")?;

        let token = std::env::var("TAP_GITHUB_TOKEN")
            .or_else(|_| std::env::var("GITHUB_TOKEN"))
            .or_else(|_| std::env::var("GH_TOKEN"))
            .map_err(|_| "TAP_GITHUB_TOKEN not set — needed for Homebrew dispatch")?;

        let tap_url = format!("https://github.com/{}", homebrew.tap);
        let (owner, repo) = crate::cargo::parse_github_url(&tap_url)
            .ok_or_else(|| format!("invalid tap format: {}", homebrew.tap))?;

        eprintln!("  dispatching to {owner}/{repo}...");
        gh::repository_dispatch(owner, repo, "update-formula", version, &token)?;
        eprintln!("  ✓ Homebrew tap update dispatched");
        Ok(())
    }
    fn dry_run(&self, ctx: &Context) -> DryRunEntry {
        let tap = ctx.config.as_ref()
            .and_then(|c| c.distribute.homebrew.as_ref())
            .map(|h| h.tap.as_str())
            .unwrap_or("unknown");
        DryRunEntry {
            pipe: "dispatch-homebrew".into(),
            description: format!("would dispatch repository event to '{tap}' with version + checksums"),
        }
    }
}

struct ScoopDispatchPipe;
impl Pipe for ScoopDispatchPipe {
    fn name(&self) -> &str { "dispatch-scoop" }
    fn run(&self, ctx: &mut Context) -> Result<(), String> {
        let config = ctx.config.as_ref().ok_or("no config")?;
        let scoop = config.distribute.scoop.as_ref().ok_or("scoop not configured")?;
        let version = ctx.version.as_ref().ok_or("no version")?;

        let token = std::env::var("BUCKET_GITHUB_TOKEN")
            .or_else(|_| std::env::var("GITHUB_TOKEN"))
            .or_else(|_| std::env::var("GH_TOKEN"))
            .map_err(|_| "BUCKET_GITHUB_TOKEN not set — needed for Scoop dispatch")?;

        let bucket_url = format!("https://github.com/{}", scoop.bucket);
        let (owner, repo) = crate::cargo::parse_github_url(&bucket_url)
            .ok_or_else(|| format!("invalid bucket format: {}", scoop.bucket))?;

        eprintln!("  dispatching to {owner}/{repo}...");
        gh::repository_dispatch(owner, repo, "update-manifest", version, &token)?;
        eprintln!("  ✓ Scoop bucket update dispatched");
        Ok(())
    }
    fn dry_run(&self, ctx: &Context) -> DryRunEntry {
        let bucket = ctx.config.as_ref()
            .and_then(|c| c.distribute.scoop.as_ref())
            .map(|s| s.bucket.as_str())
            .unwrap_or("unknown");
        DryRunEntry {
            pipe: "dispatch-scoop".into(),
            description: format!("would dispatch repository event to '{bucket}' with version + checksums"),
        }
    }
}

fn parse_npm_platform(target: &str) -> (&str, &str) {
    let os = if target.contains("apple-darwin") {
        "darwin"
    } else if target.contains("linux") {
        "linux"
    } else if target.contains("windows") {
        "win32"
    } else {
        "unknown"
    };

    let cpu = if target.starts_with("aarch64") {
        "arm64"
    } else if target.starts_with("x86_64") {
        "x64"
    } else {
        "unknown"
    };

    (os, cpu)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    fn full_config() -> ReleaserConfig {
        config::parse(r#"
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

[distribute.homebrew]
tap = "user/homebrew-my-tool"

[distribute.scoop]
bucket = "user/scoop-my-tool"
"#).unwrap()
    }

    #[test]
    fn test_publish_pipeline_dry_run_all_channels() {
        let config = full_config();
        let pipeline = build_pipeline(&config);
        let mut ctx = Context::with_config(config, true);
        let report = pipeline.execute(&mut ctx).unwrap();

        // build + archive + checksum + smoke + github + pypi + npm + crates + homebrew + scoop = 10
        assert_eq!(report.dry_run_entries.len(), 10);

        let names: Vec<&str> = report.dry_run_entries.iter().map(|e| e.pipe.as_str()).collect();
        assert!(names.contains(&"build"));
        assert!(names.contains(&"archive"));
        assert!(names.contains(&"checksum"));
        assert!(names.contains(&"smoke-test"));
        assert!(names.contains(&"github-release"));
        assert!(names.contains(&"publish-pypi"));
        assert!(names.contains(&"publish-npm"));
        assert!(names.contains(&"publish-crates"));
        assert!(names.contains(&"dispatch-homebrew"));
        assert!(names.contains(&"dispatch-scoop"));
    }

    #[test]
    fn test_publish_pipeline_github_only() {
        let config = config::parse(r#"
[package]
name = "simple"
binary = "simple"
repository = "https://github.com/user/simple"

[targets]
platforms = ["x86_64-unknown-linux-gnu"]

[distribute.github]
release = true
"#).unwrap();

        let pipeline = build_pipeline(&config);
        let mut ctx = Context::with_config(config, true);
        let report = pipeline.execute(&mut ctx).unwrap();

        // build + archive + checksum + smoke + github = 5
        assert_eq!(report.dry_run_entries.len(), 5);
    }

    #[test]
    fn test_dry_run_descriptions_are_real() {
        let config = full_config();
        let pipeline = build_pipeline(&config);
        let mut ctx = Context::with_config(config, true);
        let report = pipeline.execute(&mut ctx).unwrap();

        let build = report.dry_run_entries.iter().find(|e| e.pipe == "build").unwrap();
        assert!(build.description.contains("cargo build"));

        let pypi = report.dry_run_entries.iter().find(|e| e.pipe == "publish-pypi").unwrap();
        assert!(pypi.description.contains("maturin"));

        let crates = report.dry_run_entries.iter().find(|e| e.pipe == "publish-crates").unwrap();
        assert!(crates.description.contains("cargo publish"));
    }

    #[test]
    fn test_parse_npm_platform() {
        assert_eq!(parse_npm_platform("aarch64-apple-darwin"), ("darwin", "arm64"));
        assert_eq!(parse_npm_platform("x86_64-unknown-linux-gnu"), ("linux", "x64"));
        assert_eq!(parse_npm_platform("x86_64-pc-windows-msvc"), ("win32", "x64"));
    }
}
