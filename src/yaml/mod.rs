//! Minimal YAML parser — enough to validate GitHub Actions workflow structure.
//! Supports: mappings, sequences, scalars, multiline scalars (block and flow).
//! Does NOT support: anchors, aliases, tags, complex keys.

mod parser;
mod value;

pub use parser::parse;
pub use value::YamlValue;

#[cfg(test)]
mod tests;
