use super::*;

#[test]
fn test_simple_key_value() {
    let input = r#"name = "releaser""#;
    let val = parse(input).unwrap();
    assert_eq!(val.get_str("name"), Some("releaser"));
}

#[test]
fn test_integer_value() {
    let input = "count = 42";
    let val = parse(input).unwrap();
    assert_eq!(val.get_path("count").unwrap().as_integer(), Some(42));
}

#[test]
fn test_boolean_value() {
    let input = "enabled = true\ndisabled = false";
    let val = parse(input).unwrap();
    assert_eq!(val.get_path("enabled").unwrap().as_bool(), Some(true));
    assert_eq!(val.get_path("disabled").unwrap().as_bool(), Some(false));
}

#[test]
fn test_table() {
    let input = r#"
[package]
name = "releaser"
version = "0.1.0"
"#;
    let val = parse(input).unwrap();
    assert_eq!(val.get_str("package.name"), Some("releaser"));
    assert_eq!(val.get_str("package.version"), Some("0.1.0"));
}

#[test]
fn test_nested_table() {
    let input = r#"
[distribute.pypi]
package_name = "durable"

[distribute.npm]
scope = "@durable"
"#;
    let val = parse(input).unwrap();
    assert_eq!(val.get_str("distribute.pypi.package_name"), Some("durable"));
    assert_eq!(val.get_str("distribute.npm.scope"), Some("@durable"));
}

#[test]
fn test_array() {
    let input = r#"
[targets]
platforms = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-unknown-linux-gnu",
]
"#;
    let val = parse(input).unwrap();
    let platforms = val.get_string_array("targets.platforms").unwrap();
    assert_eq!(platforms.len(), 3);
    assert_eq!(platforms[0], "aarch64-apple-darwin");
    assert_eq!(platforms[2], "x86_64-unknown-linux-gnu");
}

#[test]
fn test_inline_table() {
    let input = r#"clap = { version = "4.6.0", features = "derive" }"#;
    let val = parse(input).unwrap();
    let clap = val.get("clap").unwrap().as_table().unwrap();
    assert_eq!(clap.get("version").unwrap().as_str(), Some("4.6.0"));
}

#[test]
fn test_dotted_key() {
    let input = r#"package.name = "releaser""#;
    let val = parse(input).unwrap();
    assert_eq!(val.get_str("package.name"), Some("releaser"));
}

#[test]
fn test_comments_ignored() {
    let input = r#"
# This is a comment
name = "releaser"  # inline comment
"#;
    let val = parse(input).unwrap();
    assert_eq!(val.get_str("name"), Some("releaser"));
}

#[test]
fn test_escape_sequences() {
    let input = r#"path = "C:\\Users\\test\n""#;
    let val = parse(input).unwrap();
    assert_eq!(val.get_str("path"), Some("C:\\Users\\test\n"));
}

#[test]
fn test_literal_string() {
    let input = r#"path = 'C:\Users\test'"#;
    let val = parse(input).unwrap();
    assert_eq!(val.get_str("path"), Some(r"C:\Users\test"));
}

#[test]
fn test_empty_string() {
    let input = r#"name = """#;
    let val = parse(input).unwrap();
    assert_eq!(val.get_str("name"), Some(""));
}

#[test]
fn test_full_releaser_toml() {
    let input = r#"
[package]
name = "durable"
binary = "durable"
description = "The SQLite of durable agent execution"
repository = "https://github.com/benelser/durable"
license = "MIT"

[targets]
platforms = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "aarch64-unknown-linux-gnu",
  "x86_64-unknown-linux-gnu",
  "x86_64-unknown-linux-musl",
  "x86_64-pc-windows-msvc",
]

[distribute.github]
release = true

[distribute.pypi]
package_name = "durable"

[distribute.npm]
scope = "@durable"

[distribute.homebrew]
tap = "benelser/homebrew-durable"


[distribute.cargo]
crate_name = "durable-runtime"

[distribute.install_script]
enabled = true
"#;
    let val = parse(input).unwrap();
    assert_eq!(val.get_str("package.name"), Some("durable"));
    assert_eq!(val.get_str("package.binary"), Some("durable"));
    assert_eq!(val.get_str("package.license"), Some("MIT"));

    let platforms = val.get_string_array("targets.platforms").unwrap();
    assert_eq!(platforms.len(), 6);
    assert_eq!(platforms[4], "x86_64-unknown-linux-musl");

    assert_eq!(
        val.get_path("distribute.github.release").unwrap().as_bool(),
        Some(true)
    );
    assert_eq!(val.get_str("distribute.pypi.package_name"), Some("durable"));
    assert_eq!(val.get_str("distribute.npm.scope"), Some("@durable"));
    assert_eq!(
        val.get_str("distribute.homebrew.tap"),
        Some("benelser/homebrew-durable")
    );
    assert_eq!(
        val.get_str("distribute.cargo.crate_name"),
        Some("durable-runtime")
    );
    assert_eq!(
        val.get_path("distribute.install_script.enabled")
            .unwrap()
            .as_bool(),
        Some(true)
    );
}

#[test]
fn test_cargo_toml_parse() {
    let input = r#"
[package]
name = "durable"
version = "0.1.0"
edition = "2024"
description = "The SQLite of durable agent execution"
license = "MIT"
repository = "https://github.com/benelser/durable"

[dependencies]
serde = { version = "1", features = "derive" }

[dev-dependencies]
tempfile = "3"
"#;
    let val = parse(input).unwrap();
    assert_eq!(val.get_str("package.name"), Some("durable"));
    assert_eq!(val.get_str("package.version"), Some("0.1.0"));
    assert_eq!(val.get_str("package.edition"), Some("2024"));
    assert_eq!(val.get_str("package.repository"), Some("https://github.com/benelser/durable"));
}

#[test]
fn test_multiline_basic_string() {
    let input = r#"desc = """
hello
world""""#;
    let val = parse(input).unwrap();
    assert_eq!(val.get_str("desc"), Some("hello\nworld"));
}

#[test]
fn test_multiline_literal_string() {
    let input = r#"path = '''
C:\Users\test
second line'''"#;
    let val = parse(input).unwrap();
    assert_eq!(val.get_str("path"), Some("C:\\Users\\test\nsecond line"));
}

#[test]
fn test_array_of_tables() {
    let input = r#"
[[bin]]
name = "my-tool"
path = "src/main.rs"

[[bin]]
name = "helper"
path = "src/helper.rs"
"#;
    let val = parse(input).unwrap();
    let bins = val.get("bin").unwrap().as_array().unwrap();
    assert_eq!(bins.len(), 2);
    assert_eq!(bins[0].get_str("name"), Some("my-tool"));
    assert_eq!(bins[1].get_str("name"), Some("helper"));
}

#[test]
fn test_array_of_tables_with_other_sections() {
    let input = r#"
[package]
name = "my-lib"
version = "0.1.0"

[[bin]]
name = "my-tool"
path = "src/main.rs"

[dependencies]
"#;
    let val = parse(input).unwrap();
    assert_eq!(val.get_str("package.name"), Some("my-lib"));
    let bins = val.get("bin").unwrap().as_array().unwrap();
    assert_eq!(bins.len(), 1);
    assert_eq!(bins[0].get_str("name"), Some("my-tool"));
}

#[test]
fn test_negative_integer() {
    let input = "offset = -42";
    let val = parse(input).unwrap();
    assert_eq!(val.get_path("offset").unwrap().as_integer(), Some(-42));
}
