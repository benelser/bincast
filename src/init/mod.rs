//! `bincast init` — the full onboarding orchestrator.
//!
//! Detect → Profile → Configure → Preview → Execute → Secrets → Done
//!
//! Does everything programmatically. Only pauses for human input when
//! we genuinely need it (profile choice, npm scope, token paste).

mod prompts;
mod serialize;
mod stages;

use std::path::Path;

use crate::error::{Error, Result};

/// Run the full init orchestrator.
pub fn run(project_dir: &Path) -> Result<()> {
    let cargo_path = project_dir.join("Cargo.toml");
    if !cargo_path.exists() {
        return Err(Error::Config(
            "no Cargo.toml found in current directory".into(),
        ));
    }

    // Check for existing config
    let config_path = project_dir.join("releaser.toml");
    if config_path.exists() {
        return Err(Error::Config(
            "releaser.toml already exists — delete it first or edit it directly".into(),
        ));
    }

    // Stage 1: DETECT
    let detection = stages::detect(project_dir)?;

    if !prompts::can_prompt() {
        return Err(Error::Config(
            "must run interactively or provide --profile flag when stdin is not a TTY".into(),
        ));
    }

    // Display detection results
    eprintln!();
    eprintln!("  bincast v{} — Ship your Rust binary everywhere", env!("CARGO_PKG_VERSION"));
    eprintln!();
    if detection.is_workspace {
        eprintln!("  Detected workspace, using binary: {}", detection.name);
    } else {
        eprintln!("  Detected: {} v{} (from Cargo.toml)", detection.name, detection.version);
    }
    eprintln!("  Repository: {}", detection.repository);
    if let Some(bin) = &detection.binary
        && bin != &detection.name
    {
        eprintln!("  Binary: {bin}");
    }
    eprintln!();

    // Stage 2: PROFILE
    let profile = stages::ask_profile()?;

    // Stage 3: CONFIGURE (channel-specific inputs)
    let channel_config = stages::configure_channels(profile, &detection)?;

    // Stage 4: Build the config
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

    if !prompts::confirm("  Execute", true)? {
        eprintln!("  Cancelled.");
        return Ok(());
    }
    eprintln!();

    // Stage 6: EXECUTE
    // Write releaser.toml
    std::fs::write(&config_path, &toml_str)?;
    eprintln!("  ✓ Wrote releaser.toml");

    // Generate files
    let output_dir = project_dir;
    let files = crate::generate::run(&config, output_dir)
        .map_err(|e| Error::Config(format!("generate failed: {e}")))?;
    eprintln!("  ✓ Generated {} files", files.len());

    // Create tap/bucket repos via gh
    if let Some(tap) = &channel_config.homebrew_tap {
        stages::create_repo_if_needed(tap, &detection.gh_available);
    }
    if let Some(bucket) = &channel_config.scoop_bucket {
        stages::create_repo_if_needed(bucket, &detection.gh_available);
    }

    // Check name availability (best effort, don't fail)
    stages::check_names(&config);

    // Git commit
    stages::git_commit(project_dir);

    // Stage 7: SECRETS
    eprintln!();
    stages::handle_secrets(&config, &detection);

    // Stage 8: DONE
    eprintln!();
    eprintln!("  Done! Release with:");
    eprintln!();
    eprintln!("    bincast release");
    eprintln!();

    Ok(())
}

// Re-export serialize for tests
pub use serialize::serialize_config;
