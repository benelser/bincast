use std::path::Path;
use std::process;

use releaser::cli::{self, Command};

fn main() {
    let cmd = match cli::parse_args() {
        Ok(cmd) => cmd,
        Err(e) => {
            eprintln!("error: {e}");
            process::exit(1);
        }
    };

    let result = match cmd {
        Command::Init => run_init(),
        Command::Generate { config } => {
            let config_path = config.as_deref().unwrap_or("releaser.toml");
            run_generate(config_path)
        }
        Command::Check { config } => {
            let config_path = config.as_deref().unwrap_or("releaser.toml");
            run_check(config_path)
        }
        Command::Publish {
            version,
            dry_run,
            config,
        } => {
            let config_path = config.as_deref().unwrap_or("releaser.toml");
            run_publish(config_path, &version, dry_run)
        }
        Command::Version => {
            println!("releaser {}", env!("CARGO_PKG_VERSION"));
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

fn run_init() -> Result<(), releaser::error::Error> {
    let toml_str = releaser::init::run(Path::new("."))?;
    let out_path = Path::new("releaser.toml");
    if out_path.exists() {
        return Err(releaser::error::Error::Config(
            "releaser.toml already exists — delete it first or edit it directly".into(),
        ));
    }
    std::fs::write(out_path, &toml_str)?;
    eprintln!("  ✓ created releaser.toml");
    Ok(())
}

fn run_generate(config_path: &str) -> Result<(), releaser::error::Error> {
    let config_path = Path::new(config_path);
    let config = releaser::config::load(config_path)?;
    let errors = releaser::config::validate::validate(&config);
    if !errors.is_empty() {
        return Err(releaser::error::Error::Validation(errors));
    }

    // Write output relative to the config file's directory
    let output_dir = config_path
        .parent()
        .unwrap_or(Path::new("."));
    let files = releaser::generate::run(&config, output_dir)
        .map_err(|e| releaser::error::Error::Config(format!("generate failed: {e}")))?;

    for f in &files {
        eprintln!("  ✓ {}", f.path);
    }
    eprintln!("\nGenerated {} files", files.len());
    Ok(())
}

fn run_check(config_path: &str) -> Result<(), releaser::error::Error> {
    let config = releaser::config::load(Path::new(config_path))?;

    // Run the check pipeline
    let pipeline = releaser::check::build_pipeline(&config);
    let mut ctx = releaser::pipeline::Context::with_config(config, false);

    match pipeline.execute(&mut ctx) {
        Ok(report) => {
            report.print_summary();
            Ok(())
        }
        Err(e) => Err(releaser::error::Error::Config(e.to_string())),
    }
}

fn run_publish(config_path: &str, version: &str, dry_run: bool) -> Result<(), releaser::error::Error> {
    let config = releaser::config::load(Path::new(config_path))?;
    let errors = releaser::config::validate::validate(&config);
    if !errors.is_empty() {
        return Err(releaser::error::Error::Validation(errors));
    }

    let pipeline = releaser::publish::build_pipeline(&config);
    let mut ctx = releaser::pipeline::Context::with_config(config, dry_run);
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
        Err(e) => Err(releaser::error::Error::Config(e.to_string())),
    }
}
