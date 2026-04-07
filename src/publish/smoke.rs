//! Smoke test pipe — verifies built artifacts are functional before publishing.

use crate::pipeline::{Context, DryRunEntry, Pipe};

pub struct SmokeTestPipe;

impl Pipe for SmokeTestPipe {
    fn name(&self) -> &str {
        "smoke-test"
    }

    fn skip(&self, ctx: &Context) -> bool {
        // Skip if no artifacts to test (but not during dry-run)
        !ctx.dry_run && ctx.artifacts.is_empty()
    }

    fn run(&self, ctx: &mut Context) -> Result<(), String> {
        let config = ctx.config.as_ref().ok_or("no config")?;
        let binary = &config.package.binary;

        // Find binary artifacts and run --help on each
        let binary_artifacts: Vec<_> = ctx
            .artifacts
            .iter()
            .filter(|a| a.kind == crate::pipeline::ArtifactKind::Binary)
            .collect();

        if binary_artifacts.is_empty() {
            // No binary artifacts yet — might be archives only at this stage
            eprintln!("  ○ smoke test: no binary artifacts to test");
            return Ok(());
        }

        for artifact in binary_artifacts {
            let path = &artifact.path;
            if !path.exists() {
                return Err(format!("binary not found: {}", path.display()));
            }

            // Only test binaries for the current platform
            let current_target = current_platform_target();
            if let Some(target) = &artifact.target
                && !target.contains(&current_target)
            {
                eprintln!("  ○ skipping smoke test for {target} (cross-platform)");
                continue;
            }

            let output = std::process::Command::new(path)
                .arg("--help")
                .output()
                .map_err(|e| format!("failed to run {binary} --help: {e}"))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!(
                    "{binary} --help failed (exit {}): {stderr}",
                    output.status
                ));
            }

            eprintln!("  ✓ smoke test: {binary} --help passed");
        }

        Ok(())
    }

    fn dry_run(&self, ctx: &Context) -> DryRunEntry {
        let count = ctx.artifacts.len();
        DryRunEntry {
            pipe: "smoke-test".into(),
            description: format!(
                "would run --help on {count} artifact(s) to verify they execute correctly"
            ),
        }
    }
}

/// Detect the current platform's Rust target triple (simplified).
fn current_platform_target() -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let rust_os = match os {
        "macos" => "apple-darwin",
        "linux" => "unknown-linux",
        "windows" => "pc-windows",
        _ => "unknown",
    };

    let rust_arch = match arch {
        "aarch64" => "aarch64",
        "x86_64" => "x86_64",
        _ => "unknown",
    };

    format!("{rust_arch}-{rust_os}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::ArtifactKind;

    #[test]
    fn test_smoke_skips_when_no_artifacts() {
        let ctx = Context::new_dry_run(false);
        let pipe = SmokeTestPipe;
        assert!(pipe.skip(&ctx));
    }

    #[test]
    fn test_smoke_dry_run_reports_count() {
        let mut ctx = Context::new_dry_run(true);
        ctx.artifacts.push(crate::pipeline::Artifact {
            path: std::path::PathBuf::from("test"),
            kind: ArtifactKind::Archive,
            target: None,
        });
        let pipe = SmokeTestPipe;
        let entry = pipe.dry_run(&ctx);
        assert!(entry.description.contains("1 artifact"));
    }

    #[test]
    fn test_current_platform_target_is_nonempty() {
        let target = current_platform_target();
        assert!(!target.is_empty());
        assert!(target.contains('-'));
    }
}
