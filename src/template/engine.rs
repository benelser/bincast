use std::collections::HashMap;

/// A simple template engine supporting:
/// - `{{ var }}` — variable substitution
/// - `{{ var.field }}` — dotted access
/// - `{% for item in list %}...{% endfor %}` — iteration
/// - `{% if cond %}...{% elif cond %}...{% else %}...{% endif %}` — conditionals
/// - `{# comment #}` — comments (stripped)

#[derive(Debug, Clone)]
pub enum Value {
    String(String),
    Bool(bool),
    List(Vec<Context>),
    Integer(i64),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::String(s) => !s.is_empty(),
            Value::Bool(b) => *b,
            Value::List(l) => !l.is_empty(),
            Value::Integer(n) => *n != 0,
        }
    }

    fn display(&self) -> String {
        match self {
            Value::String(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            Value::List(_) => "[list]".into(),
            Value::Integer(n) => n.to_string(),
        }
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Integer(n)
    }
}

/// Template context — a flat key-value map with dotted access support.
#[derive(Debug, Clone, Default)]
pub struct Context {
    values: HashMap<String, Value>,
}

impl Context {
    pub fn new() -> Self {
        Context {
            values: HashMap::new(),
        }
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        self.values.insert(key.into(), value.into());
    }

    pub fn set_list(&mut self, key: impl Into<String>, items: Vec<Context>) {
        self.values.insert(key.into(), Value::List(items));
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        // Try direct lookup first
        if let Some(v) = self.values.get(key) {
            return Some(v);
        }
        // Try dotted path: split on first dot
        if let Some(dot) = key.find('.') {
            let (prefix, _rest) = key.split_at(dot);
            if let Some(Value::List(_)) = self.values.get(prefix) {
                return None;
            }
        }
        None
    }

    fn resolve(&self, expr: &str) -> String {
        let trimmed = expr.trim();
        if let Some(val) = self.get(trimmed) {
            val.display()
        } else {
            String::new()
        }
    }

    fn is_truthy(&self, expr: &str) -> bool {
        let trimmed = expr.trim();
        // Handle negation
        if let Some(rest) = trimmed.strip_prefix("not ") {
            return !self.is_truthy(rest);
        }
        self.get(trimmed).is_some_and(|v| v.is_truthy())
    }
}

#[derive(Debug)]
pub struct Template {
    source: String,
}

impl Template {
    pub fn new(source: &str) -> Self {
        Template {
            source: source.to_string(),
        }
    }

    pub fn render(&self, ctx: &Context) -> Result<String, String> {
        render_block(&self.source, ctx)
    }
}

fn render_block(input: &str, ctx: &Context) -> Result<String, String> {
    let mut output = String::with_capacity(input.len());
    let mut pos = 0;
    let bytes = input.as_bytes();

    while pos < input.len() {
        // Look for template tags
        if pos + 1 < input.len() && bytes[pos] == b'{' {
            // Skip ${{ }} — these are GitHub Actions expressions, not our templates.
            // Output them literally (including the $).
            if pos > 0 && bytes[pos - 1] == b'$' && bytes[pos + 1] == b'{' {
                // Already output '$' in previous iteration. Now output '{{' and
                // everything up to '}}' literally.
                if let Some(end) = find_closing(input, pos + 2, "}}") {
                    output.push_str(&input[pos..end + 2]);
                    pos = end + 2;
                    continue;
                }
            }

            match bytes[pos + 1] {
                b'{' => {
                    // Variable: {{ expr }}
                    let end = find_closing(input, pos + 2, "}}")
                        .ok_or_else(|| "unclosed {{ }}".to_string())?;
                    let expr = &input[pos + 2..end];
                    output.push_str(&ctx.resolve(expr));
                    pos = end + 2;
                    continue;
                }
                b'%' => {
                    // Tag: {% ... %}
                    let end = find_closing(input, pos + 2, "%}")
                        .ok_or_else(|| "unclosed {% %}".to_string())?;
                    let tag = input[pos + 2..end].trim();

                    if let Some(rest) = tag.strip_prefix("for ") {
                        // {% for item in list %}...{% endfor %}
                        let (item_name, list_name) = parse_for_expr(rest)?;
                        let body_start = end + 2;
                        let body_end = find_end_tag(input, body_start, "for")?;
                        let body = &input[body_start..body_end];
                        let after = skip_end_tag(input, body_end, "endfor")?;

                        if let Some(Value::List(items)) = ctx.get(list_name) {
                            for item_ctx in items {
                                // Merge parent context with loop item
                                let mut merged = ctx.clone();
                                for (k, v) in &item_ctx.values {
                                    merged
                                        .values
                                        .insert(format!("{item_name}.{k}"), v.clone());
                                }
                                // Also set the item itself for simple access
                                if let Some(v) = item_ctx.values.get("_self") {
                                    merged.values.insert(item_name.to_string(), v.clone());
                                }
                                // Copy all item values with the item prefix
                                for (k, v) in &item_ctx.values {
                                    if k != "_self" {
                                        merged
                                            .values
                                            .insert(format!("{item_name}.{k}"), v.clone());
                                    }
                                }
                                output.push_str(&render_block(body, &merged)?);
                            }
                        }

                        pos = after;
                        continue;
                    } else if let Some(rest) = tag.strip_prefix("if ") {
                        // {% if cond %}...{% elif cond %}...{% else %}...{% endif %}
                        let body_start = end + 2;
                        let (branches, after) = parse_if_block(input, body_start, rest)?;

                        for (cond, body) in &branches {
                            if cond.is_empty() || ctx.is_truthy(cond) {
                                output.push_str(&render_block(body, ctx)?);
                                break;
                            }
                        }

                        pos = after;
                        continue;
                    } else if tag == "else" || tag.starts_with("elif ") || tag == "endif" || tag == "endfor" {
                        // These are handled by their parent block parsers
                        return Err(format!("unexpected tag: {{% {tag} %}}"));
                    }

                    // Unknown tag — skip
                    pos = end + 2;
                    continue;
                }
                b'#' => {
                    // Comment: {# ... #}
                    let end = find_closing(input, pos + 2, "#}")
                        .ok_or_else(|| "unclosed {# #}".to_string())?;
                    pos = end + 2;
                    continue;
                }
                _ => {}
            }
        }

        output.push(bytes[pos] as char);
        pos += 1;
    }

    Ok(output)
}

fn find_closing(input: &str, start: usize, marker: &str) -> Option<usize> {
    input[start..].find(marker).map(|i| start + i)
}

fn parse_for_expr(expr: &str) -> Result<(&str, &str), String> {
    // "item in list"
    let parts: Vec<&str> = expr.splitn(3, ' ').collect();
    if parts.len() != 3 || parts[1] != "in" {
        return Err(format!("invalid for expression: '{expr}' (expected 'item in list')"));
    }
    Ok((parts[0], parts[2]))
}

/// Find the matching end tag for a block, handling nesting.
fn find_end_tag(input: &str, start: usize, block_type: &str) -> Result<usize, String> {
    let end_tag = format!("end{block_type}");
    let mut pos = start;
    let mut depth = 1;

    while pos < input.len() {
        if let Some(tag_start) = input[pos..].find("{%") {
            let abs_start = pos + tag_start;
            if let Some(tag_end) = find_closing(input, abs_start + 2, "%}") {
                let tag = input[abs_start + 2..tag_end].trim();
                if tag.starts_with(&format!("{block_type} ")) || tag == block_type {
                    depth += 1;
                } else if tag == end_tag {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(abs_start);
                    }
                }
                pos = tag_end + 2;
            } else {
                return Err("unclosed {% %} tag".to_string());
            }
        } else {
            return Err(format!("missing {{% {end_tag} %}}"));
        }
    }

    Err(format!("missing {{% {end_tag} %}}"))
}

fn skip_end_tag(input: &str, pos: usize, tag_name: &str) -> Result<usize, String> {
    // pos points to the start of {% endXXX %}
    let tag_end = find_closing(input, pos + 2, "%}")
        .ok_or_else(|| format!("unclosed {{% {tag_name} %}}"))?;
    Ok(tag_end + 2)
}

/// Parse an if/elif/else/endif block, returning the branches and the position after endif.
fn parse_if_block(
    input: &str,
    start: usize,
    initial_cond: &str,
) -> Result<(Vec<(String, String)>, usize), String> {
    let mut branches: Vec<(String, String)> = Vec::new();
    let mut current_cond = initial_cond.to_string();
    let mut body_start = start;
    let mut pos = start;
    let mut depth = 1;

    while pos < input.len() {
        if let Some(tag_start) = input[pos..].find("{%") {
            let abs_start = pos + tag_start;
            if let Some(tag_end) = find_closing(input, abs_start + 2, "%}") {
                let tag = input[abs_start + 2..tag_end].trim();

                if tag.starts_with("if ") {
                    depth += 1;
                    pos = tag_end + 2;
                } else if tag == "endif" {
                    depth -= 1;
                    if depth == 0 {
                        let body = input[body_start..abs_start].to_string();
                        branches.push((current_cond, body));
                        return Ok((branches, tag_end + 2));
                    }
                    pos = tag_end + 2;
                } else if depth == 1 && tag.starts_with("elif ") {
                    let body = input[body_start..abs_start].to_string();
                    branches.push((current_cond, body));
                    current_cond = tag.strip_prefix("elif ").unwrap().to_string();
                    body_start = tag_end + 2;
                    pos = tag_end + 2;
                } else if depth == 1 && tag == "else" {
                    let body = input[body_start..abs_start].to_string();
                    branches.push((current_cond, body));
                    current_cond = String::new(); // empty = always true
                    body_start = tag_end + 2;
                    pos = tag_end + 2;
                } else {
                    pos = tag_end + 2;
                }
            } else {
                return Err("unclosed {% %}".into());
            }
        } else {
            return Err("missing {% endif %}".into());
        }
    }

    Err("missing {% endif %}".into())
}
