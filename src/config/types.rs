use crate::error::Error;
use crate::toml_parser::Value;

/// Root configuration parsed from releaser.toml.
#[derive(Debug, Clone)]
pub struct ReleaserConfig {
    pub package: PackageConfig,
    pub targets: TargetsConfig,
    pub distribute: DistributeConfig,
    /// Multiple binaries from a workspace. If empty, uses package.binary.
    pub binaries: Vec<BinaryConfig>,
}

/// A binary to release from a workspace.
#[derive(Debug, Clone)]
pub struct BinaryConfig {
    /// Binary name (what gets installed)
    pub name: String,
    /// Cargo package name (-p flag). If None, uses name.
    pub package: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PackageConfig {
    pub name: String,
    pub binary: String,
    pub description: Option<String>,
    pub repository: String,
    pub license: Option<String>,
    pub homepage: Option<String>,
    pub readme: Option<String>,
    /// For workspace projects: the -p flag value for cargo build/publish.
    pub workspace_package: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TargetsConfig {
    pub platforms: Vec<TargetTriple>,
}

/// A Rust target triple. Accepts any valid triple — known targets get
/// smart defaults for runner/extension mapping, unknown targets are
/// accepted with best-effort inference from the triple string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TargetTriple(String);

impl TargetTriple {
    /// Well-known targets with verified runner mappings.
    /// Based on ruff/uv's 18-target matrix.
    const KNOWN: &[&str] = &[
        // macOS
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        // Linux glibc
        "aarch64-unknown-linux-gnu",
        "x86_64-unknown-linux-gnu",
        "i686-unknown-linux-gnu",
        "armv7-unknown-linux-gnueabihf",
        "s390x-unknown-linux-gnu",
        "powerpc64le-unknown-linux-gnu",
        "riscv64gc-unknown-linux-gnu",
        // Linux musl
        "aarch64-unknown-linux-musl",
        "x86_64-unknown-linux-musl",
        "i686-unknown-linux-musl",
        "armv7-unknown-linux-musleabihf",
        // Windows
        "x86_64-pc-windows-msvc",
        "aarch64-pc-windows-msvc",
        "i686-pc-windows-msvc",
        // FreeBSD
        "x86_64-unknown-freebsd",
        // Android
        "aarch64-linux-android",
    ];

    /// Accept any target triple. Known triples get smart defaults,
    /// unknown triples are accepted with best-effort inference.
    pub fn new(triple: &str) -> Result<Self, String> {
        if triple.is_empty() {
            return Err("target triple must not be empty".into());
        }
        // Basic sanity: must contain at least one hyphen
        if !triple.contains('-') {
            return Err(format!("invalid target triple format: '{triple}' (expected arch-vendor-os or arch-os)"));
        }
        if !Self::KNOWN.contains(&triple) {
            eprintln!("  ! unknown target '{triple}' — will use best-effort runner/extension mapping");
        }
        Ok(TargetTriple(triple.to_string()))
    }

    pub fn is_known(&self) -> bool {
        Self::KNOWN.contains(&self.0.as_str())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn os(&self) -> &str {
        if self.0.contains("apple-darwin") {
            "macos"
        } else if self.0.contains("linux") {
            "linux"
        } else if self.0.contains("windows") {
            "windows"
        } else if self.0.contains("freebsd") {
            "freebsd"
        } else if self.0.contains("android") {
            "android"
        } else {
            "unknown"
        }
    }

    pub fn arch(&self) -> &str {
        // Extract arch from the first component of the triple
        self.0.split('-').next().unwrap_or("unknown")
    }

    pub fn github_runner(&self) -> &str {
        match self.os() {
            "macos" => "macos-latest",
            "windows" => "windows-latest",
            "linux" | "freebsd" | "android" => "ubuntu-latest",
            _ => "ubuntu-latest",
        }
    }

    pub fn archive_extension(&self) -> &str {
        if self.os() == "windows" { ".zip" } else { ".tar.gz" }
    }

    pub fn binary_extension(&self) -> &str {
        if self.os() == "windows" { ".exe" } else { "" }
    }

    pub fn npm_os(&self) -> &str {
        match self.os() {
            "macos" => "darwin",
            "linux" => "linux",
            "windows" => "win32",
            other => other,
        }
    }

    pub fn npm_cpu(&self) -> &str {
        match self.arch() {
            "aarch64" => "arm64",
            "x86_64" => "x64",
            "i686" => "ia32",
            "armv7" => "arm",
            other => other,
        }
    }

    pub fn is_musl(&self) -> bool {
        self.0.contains("musl")
    }
}

impl std::fmt::Display for TargetTriple {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct DistributeConfig {
    pub github: Option<GitHubConfig>,
    pub pypi: Option<PyPIConfig>,
    pub npm: Option<NpmConfig>,
    pub homebrew: Option<HomebrewConfig>,
    pub scoop: Option<ScoopConfig>,
    pub cargo: Option<CargoConfig>,
    pub install_script: Option<InstallScriptConfig>,
}

#[derive(Debug, Clone)]
pub struct GitHubConfig {
    pub release: bool,
}

#[derive(Debug, Clone)]
pub struct PyPIConfig {
    pub package_name: String,
}

#[derive(Debug, Clone)]
pub struct NpmConfig {
    pub scope: String,
    pub package_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HomebrewConfig {
    pub tap: String,
}

#[derive(Debug, Clone)]
pub struct ScoopConfig {
    pub bucket: String,
}

#[derive(Debug, Clone)]
pub struct CargoConfig {
    pub crate_name: String,
}

#[derive(Debug, Clone)]
pub struct InstallScriptConfig {
    pub enabled: bool,
}

// --- Parsing from TOML Value ---

impl ReleaserConfig {
    pub fn from_toml(val: &Value) -> Result<Self, Error> {
        let package = PackageConfig::from_toml(
            val.get("package")
                .ok_or_else(|| Error::Config("missing [package] section".into()))?,
        )?;

        let targets = TargetsConfig::from_toml(
            val.get("targets")
                .ok_or_else(|| Error::Config("missing [targets] section".into()))?,
        )?;

        let distribute = match val.get("distribute") {
            Some(d) => DistributeConfig::from_toml(d)?,
            None => DistributeConfig::default(),
        };

        // Parse [[binaries]] if present
        let binaries = val
            .get("binaries")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|b| {
                        let name = b.get_str("name")?.to_string();
                        let package = b.get_str("package").map(|s| s.to_string());
                        Some(BinaryConfig { name, package })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(ReleaserConfig {
            package,
            targets,
            distribute,
            binaries,
        })
    }

    /// Get the list of binaries to release.
    /// If [[binaries]] is configured, returns those.
    /// Otherwise, returns a single binary from [package].
    pub fn effective_binaries(&self) -> Vec<BinaryConfig> {
        if self.binaries.is_empty() {
            vec![BinaryConfig {
                name: self.package.binary.clone(),
                package: self.package.workspace_package.clone(),
            }]
        } else {
            self.binaries.clone()
        }
    }
}

impl PackageConfig {
    fn from_toml(val: &Value) -> Result<Self, Error> {
        let name = val
            .get_str("name")
            .ok_or_else(|| Error::Config("package.name is required".into()))?
            .to_string();

        let binary = val
            .get_str("binary")
            .unwrap_or_else(|| val.get_str("name").unwrap_or(""))
            .to_string();

        let repository = val
            .get_str("repository")
            .ok_or_else(|| Error::Config("package.repository is required".into()))?
            .to_string();

        Ok(PackageConfig {
            name,
            binary,
            description: val.get_str("description").map(|s| s.to_string()),
            repository,
            license: val.get_str("license").map(|s| s.to_string()),
            homepage: val.get_str("homepage").map(|s| s.to_string()),
            readme: val.get_str("readme").map(|s| s.to_string()),
            workspace_package: val.get_str("workspace_package").map(|s| s.to_string()),
        })
    }
}

impl TargetsConfig {
    fn from_toml(val: &Value) -> Result<Self, Error> {
        let platform_strs = val
            .get_string_array("platforms")
            .ok_or_else(|| Error::Config("targets.platforms must be an array of strings".into()))?;

        let mut platforms = Vec::new();
        for s in platform_strs {
            platforms.push(
                TargetTriple::new(s).map_err(Error::Config)?,
            );
        }

        if platforms.is_empty() {
            return Err(Error::Config("targets.platforms must not be empty".into()));
        }

        Ok(TargetsConfig { platforms })
    }
}

impl DistributeConfig {
    fn from_toml(val: &Value) -> Result<Self, Error> {
        let github = val.get("github").map(|v| {
            GitHubConfig {
                release: v.get_path("release").and_then(|v| v.as_bool()).unwrap_or(true),
            }
        });

        let pypi = val.get("pypi").map(|v| -> Result<PyPIConfig, Error> {
            Ok(PyPIConfig {
                package_name: v
                    .get_str("package_name")
                    .ok_or_else(|| Error::Config("distribute.pypi.package_name is required".into()))?
                    .to_string(),
            })
        }).transpose()?;

        let npm = val.get("npm").map(|v| -> Result<NpmConfig, Error> {
            Ok(NpmConfig {
                scope: v
                    .get_str("scope")
                    .ok_or_else(|| Error::Config("distribute.npm.scope is required".into()))?
                    .to_string(),
                package_name: v.get_str("package_name").map(|s| s.to_string()),
            })
        }).transpose()?;

        let homebrew = val.get("homebrew").map(|v| -> Result<HomebrewConfig, Error> {
            Ok(HomebrewConfig {
                tap: v
                    .get_str("tap")
                    .ok_or_else(|| Error::Config("distribute.homebrew.tap is required".into()))?
                    .to_string(),
            })
        }).transpose()?;

        let scoop = val.get("scoop").map(|v| -> Result<ScoopConfig, Error> {
            Ok(ScoopConfig {
                bucket: v
                    .get_str("bucket")
                    .ok_or_else(|| Error::Config("distribute.scoop.bucket is required".into()))?
                    .to_string(),
            })
        }).transpose()?;

        let cargo = val.get("cargo").map(|v| -> Result<CargoConfig, Error> {
            Ok(CargoConfig {
                crate_name: v
                    .get_str("crate_name")
                    .ok_or_else(|| Error::Config("distribute.cargo.crate_name is required".into()))?
                    .to_string(),
            })
        }).transpose()?;

        let install_script = val.get("install_script").map(|v| {
            InstallScriptConfig {
                enabled: v.get_path("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
            }
        });

        Ok(DistributeConfig {
            github,
            pypi,
            npm,
            homebrew,
            scoop,
            cargo,
            install_script,
        })
    }
}
