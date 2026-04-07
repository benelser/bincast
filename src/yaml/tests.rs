use super::*;

#[test]
fn test_simple_mapping() {
    let input = "name: Release\nversion: 1";
    let val = parse(input).unwrap();
    assert_eq!(val.get("name").unwrap().as_str(), Some("Release"));
    assert_eq!(val.get("version").unwrap(), &YamlValue::Integer(1));
}

#[test]
fn test_nested_mapping() {
    let input = "on:\n  push:\n    tags:\n      - v*";
    let val = parse(input).unwrap();
    let tags = val.get_path("on.push.tags").unwrap();
    assert!(tags.as_sequence().is_some());
}

#[test]
fn test_sequence() {
    let input = "items:\n  - alpha\n  - beta\n  - gamma";
    let val = parse(input).unwrap();
    let items = val.get("items").unwrap().as_sequence().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].as_str(), Some("alpha"));
}

#[test]
fn test_inline_sequence() {
    let input = r#"tags: ["v*"]"#;
    let val = parse(input).unwrap();
    let tags = val.get("tags").unwrap().as_sequence().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].as_str(), Some("v*"));
}

#[test]
fn test_sequence_of_mappings() {
    let input = "steps:\n  - name: Checkout\n    uses: actions/checkout@v4\n  - name: Build\n    run: cargo build";
    let val = parse(input).unwrap();
    let steps = val.get("steps").unwrap().as_sequence().unwrap();
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].get("name").unwrap().as_str(), Some("Checkout"));
    assert_eq!(steps[1].get("name").unwrap().as_str(), Some("Build"));
}

#[test]
fn test_boolean_values() {
    let input = "enabled: true\ndisabled: false";
    let val = parse(input).unwrap();
    assert_eq!(val.get("enabled").unwrap().as_bool(), Some(true));
    assert_eq!(val.get("disabled").unwrap().as_bool(), Some(false));
}

#[test]
fn test_block_scalar() {
    let input = "script: |\n  echo hello\n  echo world";
    let val = parse(input).unwrap();
    let script = val.get("script").unwrap().as_str().unwrap();
    assert!(script.contains("echo hello"));
    assert!(script.contains("echo world"));
}

#[test]
fn test_comments_ignored() {
    let input = "# this is a comment\nname: test\n# another comment\nversion: 1";
    let val = parse(input).unwrap();
    assert_eq!(val.get("name").unwrap().as_str(), Some("test"));
    assert_eq!(val.get("version").unwrap(), &YamlValue::Integer(1));
}

#[test]
fn test_quoted_string_value() {
    let input = r#"name: "my project""#;
    let val = parse(input).unwrap();
    assert_eq!(val.get("name").unwrap().as_str(), Some("my project"));
}

#[test]
fn test_get_path() {
    let input = "a:\n  b:\n    c: deep";
    let val = parse(input).unwrap();
    assert_eq!(val.get_path("a.b.c").unwrap().as_str(), Some("deep"));
}

#[test]
fn test_keys() {
    let input = "a: 1\nb: 2\nc: 3";
    let val = parse(input).unwrap();
    let keys = val.keys();
    assert!(keys.contains(&"a"));
    assert!(keys.contains(&"b"));
    assert!(keys.contains(&"c"));
}

#[test]
fn test_matrix_include() {
    let input = r#"strategy:
  matrix:
    include:
      - target: aarch64-apple-darwin
        runner: macos-latest
      - target: x86_64-unknown-linux-gnu
        runner: ubuntu-latest"#;
    let val = parse(input).unwrap();
    let include = val.get_path("strategy.matrix.include").unwrap().as_sequence().unwrap();
    assert_eq!(include.len(), 2);
    assert_eq!(include[0].get("target").unwrap().as_str(), Some("aarch64-apple-darwin"));
    assert_eq!(include[1].get("runner").unwrap().as_str(), Some("ubuntu-latest"));
}

#[test]
fn test_github_actions_permissions() {
    let input = "permissions:\n  contents: write\n  id-token: write\n  attestations: write";
    let val = parse(input).unwrap();
    let perms = val.get("permissions").unwrap().as_mapping().unwrap();
    assert_eq!(perms.get("contents").unwrap().as_str(), Some("write"));
    assert_eq!(perms.get("id-token").unwrap().as_str(), Some("write"));
    assert_eq!(perms.get("attestations").unwrap().as_str(), Some("write"));
}

#[test]
fn test_on_push_tags() {
    let input = r#"on:
  push:
    tags: ["v*"]"#;
    let val = parse(input).unwrap();
    let tags = val.get_path("on.push.tags").unwrap().as_sequence().unwrap();
    assert_eq!(tags[0].as_str(), Some("v*"));
}
