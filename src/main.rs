use std::path::Path;
use std::process;

use bincast::cli::{self, Command};

fn main() {
    let cmd = match cli::parse_args() {
        Ok(cmd) => cmd,
        Err(e) => {
            eprintln!("error: {e}");
            process::exit(1);
        }
    };

    let result = match cmd {
        Command::Init(flags) => bincast::init::run_with_flags(std::path::Path::new("."), flags),
        Command::Generate { config } => {
            let config_path = config.as_deref().unwrap_or("bincast.toml");
            run_generate(config_path)
        }
        Command::Check { config } => {
            let config_path = config.as_deref().unwrap_or("bincast.toml");
            run_check(config_path)
        }
        Command::Publish {
            version,
            dry_run,
            config,
        } => {
            let config_path = config.as_deref().unwrap_or("bincast.toml");
            run_publish(config_path, &version, dry_run)
        }
        Command::Release { dry_run } => {
            bincast::release::run(dry_run)
        }
        Command::Bump { bump } => {
            bincast::version::run(&bump)
        }
        Command::Version => {
            println!("bincast {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::Help => {
            cli::print_help();
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

fn run_generate(config_path: &str) -> Result<(), bincast::error::Error> {
    let config_path = Path::new(config_path);
    let config = bincast::config::load(config_path)?;
    let errors = bincast::config::validate::validate(&config);
    if !errors.is_empty() {
        return Err(bincast::error::Error::Validation(errors));
    }

    // Write output relative to the config file's directory
    let output_dir = config_path
        .parent()
        .unwrap_or(Path::new("."));
    let files = bincast::generate::run(&config, output_dir)
        .map_err(|e| bincast::error::Error::Config(format!("generate failed: {e}")))?;

    for f in &files {
        eprintln!("  ✓ {}", f.path);
    }
    eprintln!("\nGenerated {} files", files.len());
    Ok(())
}

fn run_check(config_path: &str) -> Result<(), bincast::error::Error> {
    let config = bincast::config::load(Path::new(config_path))?;

    // Run the check pipeline
    let pipeline = bincast::check::build_pipeline(&config);
    let mut ctx = bincast::pipeline::Context::with_config(config, false);

    match pipeline.execute(&mut ctx) {
        Ok(report) => {
            report.print_summary();
            Ok(())
        }
        Err(e) => Err(bincast::error::Error::Config(e.to_string())),
    }
}

fn run_publish(config_path: &str, version: &str, dry_run: bool) -> Result<(), bincast::error::Error> {
    let config = bincast::config::load(Path::new(config_path))?;
    let errors = bincast::config::validate::validate(&config);
    if !errors.is_empty() {
        return Err(bincast::error::Error::Validation(errors));
    }

    let pipeline = bincast::publish::build_pipeline(&config);
    let mut ctx = bincast::pipeline::Context::with_config(config, dry_run);
    ctx.version = Some(version.to_string());

    if dry_run {
        eprintln!("dry-run: simulating publish {version}\n");
    }

    match pipeline.execute(&mut ctx) {
        Ok(report) => {
            report.print_summary();
            if dry_run {
                eprintln!("\ndry-run complete — no artifacts were published");
            }
            Ok(())
        }
        Err(e) => Err(bincast::error::Error::Config(e.to_string())),
    }
}
