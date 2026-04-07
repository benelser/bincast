//! Build, archive, and checksum pipes — the real execution versions.

use std::path::PathBuf;
use crate::build;
use crate::package::{archive, checksum};
use crate::pipeline::{Artifact, ArtifactKind, Context, DryRunEntry, Pipe};

/// Detect the current platform's Rust target triple.
fn native_target() -> String {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;

    let rust_arch = match arch {
        "aarch64" => "aarch64",
        "x86_64" => "x86_64",
        "x86" => "i686",
        other => other,
    };

    let rust_os = match os {
        "macos" => "apple-darwin",
        "linux" => "unknown-linux-gnu",
        "windows" => "pc-windows-msvc",
        other => other,
    };

    format!("{rust_arch}-{rust_os}")
}

/// Pipe that builds the binary for the native platform.
pub struct BuildPipe;

impl Pipe for BuildPipe {
    fn name(&self) -> &str {
        "build"
    }

    fn run(&self, ctx: &mut Context) -> Result<(), String> {
        let config = ctx.config.as_ref().ok_or("no config")?;
        let binary = &config.package.binary;
        let target = native_target();

        eprintln!("  building {binary} for {target}...");

        let cmd = build::cargo_build_command(binary, &crate::config::TargetTriple::new(&target)
            .map_err(|e| format!("unsupported native target {target}: {e}"))?, true);

        let mut command = std::process::Command::new(&cmd.program);
        command.args(&cmd.args);
        // Add -p flag for workspace builds
        if let Some(pkg) = &config.package.workspace_package {
            command.args(["-p", pkg]);
        }
        for (k, v) in &cmd.env {
            command.env(k, v);
        }
        command.current_dir(&ctx.work_dir);

        let output = command.output().map_err(|e| format!("failed to run cargo: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("cargo build failed:\n{stderr}"));
        }

        let bin_path = ctx.work_dir.join(build::binary_path(binary, &crate::config::TargetTriple::new(&target).unwrap(), true));

        if !bin_path.exists() {
            return Err(format!("binary not found after build: {}", bin_path.display()));
        }

        ctx.artifacts.push(Artifact {
            path: bin_path,
            kind: ArtifactKind::Binary,
            target: Some(target),
        });

        eprintln!("  ✓ build complete");
        Ok(())
    }

    fn dry_run(&self, _ctx: &Context) -> DryRunEntry {
        DryRunEntry {
            pipe: "build".into(),
            description: format!("would run cargo build --release --target {}", native_target()),
        }
    }
}

/// Pipe that creates archives from built binaries.
pub struct ArchivePipe;

impl Pipe for ArchivePipe {
    fn name(&self) -> &str {
        "archive"
    }

    fn run(&self, ctx: &mut Context) -> Result<(), String> {
        let config = ctx.config.as_ref().ok_or("no config")?;
        let name = &config.package.name;

        let binaries: Vec<Artifact> = ctx.artifacts
            .iter()
            .filter(|a| a.kind == ArtifactKind::Binary)
            .cloned()
            .collect();

        if binaries.is_empty() {
            return Err("no binary artifacts to archive".into());
        }

        let dist_dir = ctx.work_dir.join("dist");
        std::fs::create_dir_all(&dist_dir)
            .map_err(|e| format!("failed to create dist/: {e}"))?;

        for bin in &binaries {
            let target = bin.target.as_deref().unwrap_or("unknown");
            let is_windows = target.contains("windows");

            eprintln!("  archiving {name}-{target}...");
            let archive_path = archive::create_archive(
                &bin.path,
                &dist_dir,
                name,
                target,
                is_windows,
            )?;

            ctx.artifacts.push(Artifact {
                path: archive_path,
                kind: ArtifactKind::Archive,
                target: Some(target.to_string()),
            });
        }

        eprintln!("  ✓ archives created");
        Ok(())
    }

    fn dry_run(&self, ctx: &Context) -> DryRunEntry {
        let count = ctx.artifacts.iter().filter(|a| a.kind == ArtifactKind::Binary).count();
        DryRunEntry {
            pipe: "archive".into(),
            description: format!("would create {count} archive(s) in dist/"),
        }
    }
}

/// Pipe that computes SHA-256 checksums for all archives.
pub struct ChecksumPipe;

impl Pipe for ChecksumPipe {
    fn name(&self) -> &str {
        "checksum"
    }

    fn run(&self, ctx: &mut Context) -> Result<(), String> {
        let archives: Vec<PathBuf> = ctx.artifacts
            .iter()
            .filter(|a| a.kind == ArtifactKind::Archive)
            .map(|a| a.path.clone())
            .collect();

        if archives.is_empty() {
            return Err("no archives to checksum".into());
        }

        for archive_path in &archives {
            eprintln!("  computing SHA-256 for {}...", archive_path.display());
            let hash = checksum::sha256_file(archive_path)?;
            let sidecar = checksum::write_checksum_file(archive_path)?;

            ctx.checksums.insert(archive_path.clone(), hash);
            ctx.artifacts.push(Artifact {
                path: sidecar,
                kind: ArtifactKind::Checksum,
                target: None,
            });
        }

        eprintln!("  ✓ checksums computed");
        Ok(())
    }

    fn dry_run(&self, ctx: &Context) -> DryRunEntry {
        let count = ctx.artifacts.iter().filter(|a| a.kind == ArtifactKind::Archive).count();
        DryRunEntry {
            pipe: "checksum".into(),
            description: format!("would compute SHA-256 for {count} archive(s)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_target_is_valid() {
        let target = native_target();
        assert!(target.contains('-'), "target should have dashes: {target}");
        // Should be a valid-looking triple
        let parts: Vec<&str> = target.split('-').collect();
        assert!(parts.len() >= 2, "target should have at least 2 parts: {target}");
    }

    #[test]
    fn test_build_pipe_dry_run() {
        let pipe = BuildPipe;
        let ctx = Context::new_dry_run(true);
        let entry = pipe.dry_run(&ctx);
        assert!(entry.description.contains("cargo build"));
    }

    #[test]
    fn test_archive_pipe_dry_run() {
        let pipe = ArchivePipe;
        let ctx = Context::new_dry_run(true);
        let entry = pipe.dry_run(&ctx);
        assert!(entry.description.contains("archive"));
    }

    #[test]
    fn test_checksum_pipe_dry_run() {
        let pipe = ChecksumPipe;
        let ctx = Context::new_dry_run(true);
        let entry = pipe.dry_run(&ctx);
        assert!(entry.description.contains("SHA-256"));
    }
}
