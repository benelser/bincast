use super::*;

#[test]
fn test_github_actions_expressions_pass_through() {
    let t = Template::new("runs-on: ${{ matrix.runner }}");
    let ctx = Context::new();
    assert_eq!(
        t.render(&ctx).unwrap(),
        "runs-on: ${{ matrix.runner }}"
    );
}

#[test]
fn test_github_actions_mixed_with_template_vars() {
    let t = Template::new("name: {{ name }}\nruns-on: ${{ matrix.runner }}");
    let mut ctx = Context::new();
    ctx.set("name", "Release");
    let result = t.render(&ctx).unwrap();
    assert_eq!(result, "name: Release\nruns-on: ${{ matrix.runner }}");
}

#[test]
fn test_github_actions_secrets() {
    let t = Template::new("token: ${{ secrets.GITHUB_TOKEN }}");
    let ctx = Context::new();
    assert_eq!(
        t.render(&ctx).unwrap(),
        "token: ${{ secrets.GITHUB_TOKEN }}"
    );
}

#[test]
fn test_github_actions_in_json() {
    let t = Template::new(r#"client-payload: '{"version": "${{ github.ref_name }}"}'"#);
    let ctx = Context::new();
    let result = t.render(&ctx).unwrap();
    assert!(result.contains("${{ github.ref_name }}"));
}

#[test]
fn test_simple_variable() {
    let t = Template::new("Hello {{ name }}!");
    let mut ctx = Context::new();
    ctx.set("name", "world");
    assert_eq!(t.render(&ctx).unwrap(), "Hello world!");
}

#[test]
fn test_missing_variable_renders_empty() {
    let t = Template::new("Hello {{ name }}!");
    let ctx = Context::new();
    assert_eq!(t.render(&ctx).unwrap(), "Hello !");
}

#[test]
fn test_boolean_value() {
    let t = Template::new("enabled: {{ flag }}");
    let mut ctx = Context::new();
    ctx.set("flag", true);
    assert_eq!(t.render(&ctx).unwrap(), "enabled: true");
}

#[test]
fn test_integer_value() {
    let t = Template::new("count: {{ n }}");
    let mut ctx = Context::new();
    ctx.set("n", 42i64);
    assert_eq!(t.render(&ctx).unwrap(), "count: 42");
}

#[test]
fn test_if_true() {
    let t = Template::new("{% if enabled %}yes{% endif %}");
    let mut ctx = Context::new();
    ctx.set("enabled", true);
    assert_eq!(t.render(&ctx).unwrap(), "yes");
}

#[test]
fn test_if_false() {
    let t = Template::new("{% if enabled %}yes{% endif %}");
    let mut ctx = Context::new();
    ctx.set("enabled", false);
    assert_eq!(t.render(&ctx).unwrap(), "");
}

#[test]
fn test_if_else() {
    let t = Template::new("{% if enabled %}yes{% else %}no{% endif %}");
    let mut ctx = Context::new();
    ctx.set("enabled", false);
    assert_eq!(t.render(&ctx).unwrap(), "no");
}

#[test]
fn test_if_elif_else() {
    let t = Template::new("{% if a %}A{% elif b %}B{% else %}C{% endif %}");

    let mut ctx = Context::new();
    ctx.set("a", false);
    ctx.set("b", true);
    assert_eq!(t.render(&ctx).unwrap(), "B");

    let mut ctx2 = Context::new();
    ctx2.set("a", false);
    ctx2.set("b", false);
    assert_eq!(t.render(&ctx2).unwrap(), "C");
}

#[test]
fn test_if_not() {
    let t = Template::new("{% if not disabled %}active{% endif %}");
    let mut ctx = Context::new();
    ctx.set("disabled", false);
    assert_eq!(t.render(&ctx).unwrap(), "active");
}

#[test]
fn test_if_missing_is_falsy() {
    let t = Template::new("{% if missing %}yes{% else %}no{% endif %}");
    let ctx = Context::new();
    assert_eq!(t.render(&ctx).unwrap(), "no");
}

#[test]
fn test_for_loop() {
    let t = Template::new("{% for item in items %}{{ item.name }}\n{% endfor %}");
    let mut ctx = Context::new();

    let items = vec![
        {
            let mut c = Context::new();
            c.set("name", "alpha");
            c
        },
        {
            let mut c = Context::new();
            c.set("name", "beta");
            c
        },
    ];
    ctx.set_list("items", items);

    assert_eq!(t.render(&ctx).unwrap(), "alpha\nbeta\n");
}

#[test]
fn test_for_loop_multiple_fields() {
    let t = Template::new(
        "{% for t in targets %}  - {{ t.triple }} ({{ t.os }})\n{% endfor %}",
    );
    let mut ctx = Context::new();

    let targets = vec![
        {
            let mut c = Context::new();
            c.set("triple", "aarch64-apple-darwin");
            c.set("os", "macos");
            c
        },
        {
            let mut c = Context::new();
            c.set("triple", "x86_64-unknown-linux-gnu");
            c.set("os", "linux");
            c
        },
    ];
    ctx.set_list("targets", targets);

    let result = t.render(&ctx).unwrap();
    assert_eq!(
        result,
        "  - aarch64-apple-darwin (macos)\n  - x86_64-unknown-linux-gnu (linux)\n"
    );
}

#[test]
fn test_empty_for_loop() {
    let t = Template::new("{% for item in items %}x{% endfor %}");
    let mut ctx = Context::new();
    ctx.set_list("items", vec![]);
    assert_eq!(t.render(&ctx).unwrap(), "");
}

#[test]
fn test_comment_stripped() {
    let t = Template::new("before{# this is a comment #}after");
    let ctx = Context::new();
    assert_eq!(t.render(&ctx).unwrap(), "beforeafter");
}

#[test]
fn test_nested_if_in_for() {
    let t = Template::new(
        "{% for t in targets %}{% if t.is_windows %}.zip{% else %}.tar.gz{% endif %}\n{% endfor %}",
    );
    let mut ctx = Context::new();
    let targets = vec![
        {
            let mut c = Context::new();
            c.set("is_windows", false);
            c
        },
        {
            let mut c = Context::new();
            c.set("is_windows", true);
            c
        },
    ];
    ctx.set_list("targets", targets);
    assert_eq!(t.render(&ctx).unwrap(), ".tar.gz\n.zip\n");
}

#[test]
fn test_plain_text_passthrough() {
    let t = Template::new("no templates here");
    let ctx = Context::new();
    assert_eq!(t.render(&ctx).unwrap(), "no templates here");
}

#[test]
fn test_unclosed_variable_error() {
    let t = Template::new("{{ unclosed");
    let ctx = Context::new();
    assert!(t.render(&ctx).is_err());
}

#[test]
fn test_unclosed_if_error() {
    let t = Template::new("{% if x %}no endif");
    let ctx = Context::new();
    assert!(t.render(&ctx).is_err());
}

#[test]
fn test_yaml_like_output() {
    let t = Template::new(
        r#"name: Release
on:
  push:
    tags: ["v*"]

jobs:
  build:
    strategy:
      matrix:
        include:
{% for t in targets %}          - target: {{ t.triple }}
            os: {{ t.runner }}
{% endfor %}"#,
    );

    let mut ctx = Context::new();
    let targets = vec![
        {
            let mut c = Context::new();
            c.set("triple", "aarch64-apple-darwin");
            c.set("runner", "macos-latest");
            c
        },
        {
            let mut c = Context::new();
            c.set("triple", "x86_64-unknown-linux-gnu");
            c.set("runner", "ubuntu-latest");
            c
        },
    ];
    ctx.set_list("targets", targets);

    let result = t.render(&ctx).unwrap();
    assert!(result.contains("- target: aarch64-apple-darwin"));
    assert!(result.contains("os: macos-latest"));
    assert!(result.contains("- target: x86_64-unknown-linux-gnu"));
    assert!(result.contains("os: ubuntu-latest"));
}
