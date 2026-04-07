/// Minimal CLI argument parser — no dependencies.
/// Supports: releaser <subcommand> [options]

#[derive(Debug)]
pub enum Command {
    Init,
    Generate {
        /// Override config file path (default: releaser.toml)
        config: Option<String>,
    },
    Check {
        config: Option<String>,
    },
    Publish {
        version: String,
        dry_run: bool,
        config: Option<String>,
    },
    Version,
    Help,
}

pub fn parse_args() -> Result<Command, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    parse_from(&args)
}

pub fn parse_from(args: &[String]) -> Result<Command, String> {
    if args.is_empty() {
        return Ok(Command::Help);
    }

    match args[0].as_str() {
        "init" => Ok(Command::Init),
        "generate" => {
            let config = extract_option(args, "--config");
            Ok(Command::Generate { config })
        }
        "check" => {
            let config = extract_option(args, "--config");
            Ok(Command::Check { config })
        }
        "publish" => {
            let version = args
                .get(1)
                .filter(|a| !a.starts_with('-'))
                .ok_or_else(|| "usage: releaser publish <version> [--dry-run]".to_string())?
                .clone();
            let dry_run = args.iter().any(|a| a == "--dry-run");
            let config = extract_option(args, "--config");
            Ok(Command::Publish {
                version,
                dry_run,
                config,
            })
        }
        "--version" | "-V" | "version" => Ok(Command::Version),
        "--help" | "-h" | "help" => Ok(Command::Help),
        other => Err(format!(
            "unknown command: '{other}'\n\nUsage: releaser <command>\n\nCommands:\n  init       Initialize releaser.toml from Cargo.toml\n  generate   Generate CI workflows, install scripts, and package manifests\n  check      Validate config and check name availability\n  publish    Build, package, and publish to all channels\n  help       Show this help message"
        )),
    }
}

fn extract_option(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].clone())
}

pub fn print_help() {
    eprintln!(
        "releaser {}
Ship your Rust binary to every package manager with one command.

USAGE:
    releaser <COMMAND>

COMMANDS:
    init       Initialize releaser.toml from Cargo.toml
    generate   Generate CI workflows, install scripts, and package manifests
    check      Validate config and check name availability
    publish    Build, package, and publish to all channels

OPTIONS:
    --config <PATH>    Path to releaser.toml (default: ./releaser.toml)
    --dry-run          Publish: simulate without publishing
    -V, --version      Print version
    -h, --help         Print help",
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn test_no_args_shows_help() {
        assert!(matches!(parse_from(&[]).unwrap(), Command::Help));
    }

    #[test]
    fn test_init() {
        assert!(matches!(parse_from(&args("init")).unwrap(), Command::Init));
    }

    #[test]
    fn test_generate() {
        assert!(matches!(
            parse_from(&args("generate")).unwrap(),
            Command::Generate { config: None }
        ));
    }

    #[test]
    fn test_generate_with_config() {
        let cmd = parse_from(&args("generate --config custom.toml")).unwrap();
        match cmd {
            Command::Generate { config } => assert_eq!(config.as_deref(), Some("custom.toml")),
            _ => panic!("expected Generate"),
        }
    }

    #[test]
    fn test_check() {
        assert!(matches!(
            parse_from(&args("check")).unwrap(),
            Command::Check { config: None }
        ));
    }

    #[test]
    fn test_publish() {
        let cmd = parse_from(&args("publish v0.1.0")).unwrap();
        match cmd {
            Command::Publish {
                version,
                dry_run,
                config,
            } => {
                assert_eq!(version, "v0.1.0");
                assert!(!dry_run);
                assert!(config.is_none());
            }
            _ => panic!("expected Publish"),
        }
    }

    #[test]
    fn test_publish_dry_run() {
        let cmd = parse_from(&args("publish v0.2.0 --dry-run")).unwrap();
        match cmd {
            Command::Publish { version, dry_run, .. } => {
                assert_eq!(version, "v0.2.0");
                assert!(dry_run);
            }
            _ => panic!("expected Publish"),
        }
    }

    #[test]
    fn test_publish_missing_version() {
        assert!(parse_from(&args("publish")).is_err());
    }

    #[test]
    fn test_version_flag() {
        assert!(matches!(
            parse_from(&args("--version")).unwrap(),
            Command::Version
        ));
        assert!(matches!(
            parse_from(&args("-V")).unwrap(),
            Command::Version
        ));
    }

    #[test]
    fn test_help_flag() {
        assert!(matches!(
            parse_from(&args("--help")).unwrap(),
            Command::Help
        ));
    }

    #[test]
    fn test_unknown_command() {
        assert!(parse_from(&args("deploy")).is_err());
    }
}
