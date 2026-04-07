//! `bincast init` — the full onboarding orchestrator.
//!
//! Two modes:
//! - Interactive: wizard with profile selection (human at terminal)
//! - Non-interactive: --channels flag (agent-driven or CI)
//!
//! Does everything programmatically. Only pauses for human input when
//! we genuinely need it.

mod prompts;
mod serialize;
mod stages;

use std::path::Path;

use crate::cli::InitFlags;
use crate::error::{Error, Result};

/// Entry point — dispatches to interactive or flag-driven flow.
pub fn run_with_flags(project_dir: &Path, flags: InitFlags) -> Result<()> {
    let cargo_path = project_dir.join("Cargo.toml");
    if !cargo_path.exists() {
        return Err(Error::Config(
            "no Cargo.toml found in current directory".into(),
        ));
    }

    let config_path = project_dir.join("bincast.toml");
    if config_path.exists() {
        return Err(Error::Config(
            "bincast.toml already exists — delete it first or edit it directly".into(),
        ));
    }

    // Stage 1: DETECT
    let detection = stages::detect(project_dir)?;

    // Display detection
    eprintln!();
    eprintln!("  bincast v{} — Ship your Rust binary everywhere", env!("CARGO_PKG_VERSION"));
    eprintln!();
    if detection.is_workspace {
        if detection.all_binaries.len() > 1 {
            eprintln!("  Detected workspace with {} binaries:", detection.all_binaries.len());
            for (bin, pkg) in &detection.all_binaries {
                let pkg_info = pkg.as_deref().unwrap_or(bin);
                eprintln!("    + {bin} (package: {pkg_info})");
            }
        } else {
            eprintln!("  Detected workspace, binary: {}", detection.name);
        }
    } else {
        eprintln!("  Detected: {} v{} (from Cargo.toml)", detection.name, detection.version);
    }
    eprintln!("  Repository: {}", detection.repository);
    eprintln!();

    // Stage 2+3: CHANNEL SELECTION
    let channel_config = if flags.channels.is_some() {
        // Non-interactive: channels from flags
        channels_from_flags(&flags, &detection)?
    } else if prompts::can_prompt() {
        // Interactive: wizard
        let profile = stages::ask_profile()?;
        stages::configure_channels(profile, &detection)?
    } else {
        return Err(Error::Config(
            "must run interactively or provide --channels flag\n\n  \
             Examples:\n    \
             bincast init --channels github,cargo,install-scripts\n    \
             bincast init --channels github,pypi,homebrew --npm-scope @myorg\n    \
             bincast init --channels github,pypi,npm,homebrew,scoop,cargo,install-scripts --npm-scope @myorg --yes".into(),
        ));
    };

    // Stage 4: Build config
    let config = stages::build_config(&detection, &channel_config);
    let toml_str = serialize::serialize_config(&config);

    // Stage 5: PREVIEW + CONFIRM
    let actions = stages::plan_actions(&config, &detection);
    eprintln!();
    eprintln!("  Ready to set up release infrastructure:");
    eprintln!();
    for action in &actions {
        eprintln!("    {action}");
    }
    eprintln!();

    if !flags.yes {
        if prompts::can_prompt() {
            if !prompts::confirm("  Execute", true)? {
                eprintln!("  Cancelled.");
                return Ok(());
            }
        } else {
            return Err(Error::Config(
                "use --yes to confirm non-interactively".into(),
            ));
        }
    }
    eprintln!();

    // Stage 6: EXECUTE
    std::fs::write(&config_path, &toml_str)?;
    eprintln!("  ✓ Wrote bincast.toml");

    let output_dir = project_dir;
    let files = crate::generate::run(&config, output_dir)
        .map_err(|e| Error::Config(format!("generate failed: {e}")))?;
    eprintln!("  ✓ Generated {} files", files.len());

    if let Some(tap) = &channel_config.homebrew_tap {
        stages::create_repo_if_needed(tap, &detection.gh_available);
    }
    if let Some(bucket) = &channel_config.scoop_bucket {
        stages::create_repo_if_needed(bucket, &detection.gh_available);
    }

    stages::check_names(&config);
    stages::git_commit(project_dir);

    // Stage 7: SECRETS
    eprintln!();
    stages::handle_secrets(&config, &detection);

    // Stage 8: DONE
    eprintln!();
    eprintln!("  Done! Release with:");
    eprintln!();
    eprintln!("    bincast version patch");
    eprintln!("    bincast release");
    eprintln!();

    Ok(())
}

/// Build channel config from CLI flags.
fn channels_from_flags(flags: &InitFlags, det: &stages::Detection) -> Result<stages::ChannelConfig> {
    let channels_str = flags.channels.as_deref().unwrap_or("github,install-scripts");
    let channels: Vec<&str> = channels_str.split(',').map(|s| s.trim()).collect();

    let owner = &det.owner;
    let name = &det.name;

    let npm_scope = if channels.contains(&"npm") {
        Some(flags.npm_scope.clone().ok_or_else(|| {
            Error::Config("--npm-scope is required when npm channel is enabled\n\n  Example: bincast init --channels github,npm --npm-scope @myorg".into())
        })?)
    } else {
        None
    };

    let homebrew_tap = if channels.contains(&"homebrew") || channels.contains(&"brew") {
        Some(flags.tap.clone().unwrap_or_else(|| format!("{owner}/homebrew-{name}")))
    } else {
        None
    };

    let scoop_bucket = if channels.contains(&"scoop") {
        Some(flags.bucket.clone().unwrap_or_else(|| format!("{owner}/scoop-{name}")))
    } else {
        None
    };

    Ok(stages::ChannelConfig {
        install_scripts: channels.contains(&"install-scripts") || channels.contains(&"curl"),
        pypi_name: if channels.contains(&"pypi") || channels.contains(&"pip") { Some(name.clone()) } else { None },
        npm_scope,
        homebrew_tap,
        scoop_bucket,
        cargo_crate: if channels.contains(&"cargo") { Some(name.clone()) } else { None },
    })
}

// Re-export serialize for tests
pub use serialize::serialize_config;
