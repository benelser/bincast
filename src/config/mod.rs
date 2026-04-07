pub mod types;
pub mod validate;
pub mod defaults;

pub use types::*;

use crate::error::Result;
use crate::toml_parser;

/// Load and parse a releaser.toml file.
pub fn load(path: &std::path::Path) -> Result<ReleaserConfig> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            crate::error::Error::Config(format!(
                "{} not found — run 'bincast init' to create one",
                path.display()
            ))
        } else {
            crate::error::Error::Io(e)
        }
    })?;
    parse(&content)
}

/// Parse a releaser.toml string into a ReleaserConfig.
pub fn parse(input: &str) -> Result<ReleaserConfig> {
    let value = toml_parser::parse(input)?;
    ReleaserConfig::from_toml(&value)
}
