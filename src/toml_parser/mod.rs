mod lexer;
mod parser;
mod value;

pub use parser::parse;
pub use value::Value;

#[cfg(test)]
mod tests;
