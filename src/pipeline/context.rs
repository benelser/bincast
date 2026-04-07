use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::ReleaserConfig;

/// Shared state flowing through the pipeline.
pub struct Context {
    /// The parsed bincast.toml config.
    pub config: Option<ReleaserConfig>,
    /// The version being released.
    pub version: Option<String>,
    /// Artifacts produced by build/package pipes.
    pub artifacts: Vec<Artifact>,
    /// SHA-256 checksums keyed by artifact path.
    pub checksums: HashMap<PathBuf, String>,
    /// GitHub Release URL (set by GitHubReleasePipe, used by downstream).
    pub github_release_url: Option<String>,
    /// Owner extracted from repository URL.
    pub owner: Option<String>,
    /// Repo name extracted from repository URL.
    pub repo: Option<String>,
    /// Whether this is a dry-run (no side effects).
    pub dry_run: bool,
    /// Working directory for build operations.
    pub work_dir: PathBuf,
}

impl Context {
    pub fn new_dry_run(dry_run: bool) -> Self {
        Context {
            config: None,
            version: None,
            artifacts: Vec::new(),
            checksums: HashMap::new(),
            github_release_url: None,
            owner: None,
            repo: None,
            dry_run,
            work_dir: PathBuf::from("."),
        }
    }

    pub fn with_config(config: ReleaserConfig, dry_run: bool) -> Self {
        let (owner, repo) = crate::cargo::parse_github_url(&config.package.repository)
            .map(|(o, r)| (Some(o.to_string()), Some(r.to_string())))
            .unwrap_or((None, None));

        Context {
            config: Some(config),
            version: None,
            artifacts: Vec::new(),
            checksums: HashMap::new(),
            github_release_url: None,
            owner,
            repo,
            dry_run,
            work_dir: PathBuf::from("."),
        }
    }
}

/// An artifact produced during the pipeline.
#[derive(Debug, Clone)]
pub struct Artifact {
    pub path: PathBuf,
    pub kind: ArtifactKind,
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArtifactKind {
    Binary,
    Archive,
    Wheel,
    Checksum,
    NpmPackage,
}
