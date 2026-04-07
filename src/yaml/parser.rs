use std::collections::BTreeMap;
use super::value::YamlValue;

/// Parse a YAML string into a YamlValue.
pub fn parse(input: &str) -> Result<YamlValue, String> {
    let lines: Vec<&str> = input.lines().collect();
    let mut parser = Parser::new(&lines);
    parser.parse_value(0)
}

struct Parser<'a> {
    lines: &'a [&'a str],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(lines: &'a [&'a str]) -> Self {
        Parser { lines, pos: 0 }
    }

    fn current_line(&self) -> Option<&'a str> {
        self.lines.get(self.pos).copied()
    }

    fn skip_empty_and_comments(&mut self) {
        while let Some(line) = self.current_line() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn indent_of(&self, line: &str) -> usize {
        line.len() - line.trim_start().len()
    }

    fn parse_value(&mut self, min_indent: usize) -> Result<YamlValue, String> {
        self.skip_empty_and_comments();

        let line = match self.current_line() {
            Some(l) => l,
            None => return Ok(YamlValue::Null),
        };

        let trimmed = line.trim();

        // Flow sequence: [a, b, c]
        if trimmed.starts_with('[') {
            return self.parse_flow_sequence();
        }

        // Flow mapping: {a: b, c: d}
        if trimmed.starts_with('{') {
            return self.parse_flow_mapping();
        }

        // Block sequence: starts with "- "
        if trimmed.starts_with("- ") || trimmed == "-" {
            return self.parse_block_sequence(min_indent);
        }

        // Block mapping: has a ":"
        if trimmed.contains(": ") || trimmed.ends_with(':') {
            return self.parse_block_mapping(min_indent);
        }

        // Scalar
        self.pos += 1;
        Ok(parse_scalar(trimmed))
    }

    fn parse_block_mapping(&mut self, min_indent: usize) -> Result<YamlValue, String> {
        let mut map = BTreeMap::new();

        while let Some(line) = self.current_line() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                self.pos += 1;
                continue;
            }

            let indent = self.indent_of(line);
            if indent < min_indent {
                break;
            }

            // If this is a sequence item at the same indent, we're done
            if trimmed.starts_with("- ") && indent == min_indent && !map.is_empty() {
                break;
            }

            if let Some((key, rest)) = split_key_value(trimmed) {
                self.pos += 1;

                let value = if rest.is_empty() {
                    // Value on next line(s), indented further
                    self.skip_empty_and_comments();
                    if let Some(next_line) = self.current_line() {
                        let next_indent = self.indent_of(next_line);
                        if next_indent > indent {
                            self.parse_value(next_indent)?
                        } else {
                            YamlValue::Null
                        }
                    } else {
                        YamlValue::Null
                    }
                } else if rest.starts_with('[') {
                    parse_inline_sequence(rest)?
                } else if rest.starts_with('{') {
                    parse_inline_mapping(rest)?
                } else if rest == "|" || rest == "|-" || rest == "|+" {
                    self.parse_block_scalar(indent, rest)?
                } else if rest == ">" || rest == ">-" || rest == ">+" {
                    self.parse_folded_scalar(indent, rest)?
                } else {
                    parse_scalar(rest)
                };

                map.insert(key.to_string(), value);
            } else {
                // Not a key-value pair, might be a continuation or error
                break;
            }
        }

        if map.is_empty() {
            Ok(YamlValue::Null)
        } else {
            Ok(YamlValue::Mapping(map))
        }
    }

    fn parse_block_sequence(&mut self, min_indent: usize) -> Result<YamlValue, String> {
        let mut items = Vec::new();

        while let Some(line) = self.current_line() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                self.pos += 1;
                continue;
            }

            let indent = self.indent_of(line);
            if indent < min_indent {
                break;
            }

            if !trimmed.starts_with("- ") && trimmed != "-" {
                break;
            }

            let after_dash = if trimmed == "-" {
                ""
            } else {
                &trimmed[2..]
            };

            if after_dash.is_empty() {
                // Block sequence item with value on next line
                self.pos += 1;
                self.skip_empty_and_comments();
                if let Some(next_line) = self.current_line() {
                    let next_indent = self.indent_of(next_line);
                    if next_indent > indent {
                        items.push(self.parse_value(next_indent)?);
                    } else {
                        items.push(YamlValue::Null);
                    }
                }
            } else if after_dash.contains(": ") || after_dash.ends_with(':') {
                // Sequence item is a mapping: "- key: value"
                // We need to parse the mapping starting from the content after "-"
                self.pos += 1;
                let mut map = BTreeMap::new();
                if let Some((key, rest)) = split_key_value(after_dash) {
                    let value = if rest.is_empty() {
                        self.skip_empty_and_comments();
                        if let Some(next_line) = self.current_line() {
                            let next_indent = self.indent_of(next_line);
                            if next_indent > indent + 2 {
                                self.parse_value(next_indent)?
                            } else {
                                YamlValue::Null
                            }
                        } else {
                            YamlValue::Null
                        }
                    } else if rest.starts_with('[') {
                        parse_inline_sequence(rest)?
                    } else if rest.starts_with('{') {
                        parse_inline_mapping(rest)?
                    } else {
                        parse_scalar(rest)
                    };
                    map.insert(key.to_string(), value);
                }
                // Continue reading sibling keys at dash+2 indent
                let item_indent = indent + 2;
                while let Some(next) = self.current_line() {
                    let nt = next.trim();
                    if nt.is_empty() || nt.starts_with('#') {
                        self.pos += 1;
                        continue;
                    }
                    let ni = self.indent_of(next);
                    if ni < item_indent {
                        break;
                    }
                    if let Some((k, v)) = split_key_value(nt) {
                        self.pos += 1;
                        let val = if v.is_empty() {
                            self.skip_empty_and_comments();
                            if let Some(nl) = self.current_line() {
                                let nli = self.indent_of(nl);
                                if nli > ni {
                                    self.parse_value(nli)?
                                } else {
                                    YamlValue::Null
                                }
                            } else {
                                YamlValue::Null
                            }
                        } else if v.starts_with('[') {
                            parse_inline_sequence(v)?
                        } else if v.starts_with('{') {
                            parse_inline_mapping(v)?
                        } else {
                            parse_scalar(v)
                        };
                        map.insert(k.to_string(), val);
                    } else {
                        break;
                    }
                }
                items.push(YamlValue::Mapping(map));
            } else {
                // Simple scalar item
                self.pos += 1;
                items.push(parse_scalar(after_dash));
            }
        }

        Ok(YamlValue::Sequence(items))
    }

    fn parse_flow_sequence(&mut self) -> Result<YamlValue, String> {
        let line = self.current_line().unwrap().trim();
        self.pos += 1;
        parse_inline_sequence(line)
    }

    fn parse_flow_mapping(&mut self) -> Result<YamlValue, String> {
        let line = self.current_line().unwrap().trim();
        self.pos += 1;
        parse_inline_mapping(line)
    }

    fn parse_block_scalar(&mut self, parent_indent: usize, _indicator: &str) -> Result<YamlValue, String> {
        let mut lines = Vec::new();
        let mut content_indent: Option<usize> = None;

        while let Some(line) = self.current_line() {
            if line.trim().is_empty() {
                lines.push("");
                self.pos += 1;
                continue;
            }
            let indent = self.indent_of(line);
            if indent <= parent_indent {
                break;
            }
            if content_indent.is_none() {
                content_indent = Some(indent);
            }
            let ci = content_indent.unwrap();
            if indent >= ci {
                lines.push(&line[ci..]);
            }
            self.pos += 1;
        }

        // Remove trailing empty lines
        while lines.last() == Some(&"") {
            lines.pop();
        }

        Ok(YamlValue::String(lines.join("\n")))
    }

    fn parse_folded_scalar(&mut self, parent_indent: usize, indicator: &str) -> Result<YamlValue, String> {
        // For our purposes, folded and literal produce the same result
        self.parse_block_scalar(parent_indent, indicator)
    }
}

/// Split "key: value" or "key:" into (key, value).
fn split_key_value(s: &str) -> Option<(&str, &str)> {
    // Handle quoted keys
    if s.starts_with('"') || s.starts_with('\'') {
        let quote = s.as_bytes()[0];
        if let Some(end) = s[1..].find(quote as char) {
            let key = &s[1..end + 1];
            let rest = &s[end + 2..];
            if let Some(stripped) = rest.strip_prefix(": ") {
                return Some((key, stripped.trim()));
            }
            if rest == ":" {
                return Some((key, ""));
            }
        }
        return None;
    }

    if let Some(colon) = s.find(':') {
        let key = &s[..colon];
        // Ensure key is a valid bare key (no spaces in key part)
        if key.contains(' ') && !key.starts_with("on") && !key.starts_with("if") {
            return None;
        }
        let rest = &s[colon + 1..];
        if rest.is_empty() {
            Some((key, ""))
        } else if rest.starts_with(' ') {
            Some((key, rest.trim_start()))
        } else {
            // colon is part of a URL or value, not a key separator
            None
        }
    } else {
        None
    }
}

fn parse_scalar(s: &str) -> YamlValue {
    let s = s.trim();

    // Handle quoted strings
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        return YamlValue::String(s[1..s.len() - 1].to_string());
    }

    // Booleans
    match s {
        "true" | "True" | "TRUE" | "yes" | "Yes" | "YES" | "on" | "On" | "ON" => {
            return YamlValue::Bool(true);
        }
        "false" | "False" | "FALSE" | "no" | "No" | "NO" | "off" | "Off" | "OFF" => {
            return YamlValue::Bool(false);
        }
        "null" | "Null" | "NULL" | "~" => {
            return YamlValue::Null;
        }
        _ => {}
    }

    // Integers
    if let Ok(n) = s.parse::<i64>() {
        return YamlValue::Integer(n);
    }

    YamlValue::String(s.to_string())
}

fn parse_inline_sequence(s: &str) -> Result<YamlValue, String> {
    let s = s.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return Err(format!("invalid inline sequence: {s}"));
    }
    let inner = &s[1..s.len() - 1];
    if inner.trim().is_empty() {
        return Ok(YamlValue::Sequence(vec![]));
    }
    let items: Vec<YamlValue> = split_flow_items(inner)
        .iter()
        .map(|item| parse_scalar(item))
        .collect();
    Ok(YamlValue::Sequence(items))
}

fn parse_inline_mapping(s: &str) -> Result<YamlValue, String> {
    let s = s.trim();
    if !s.starts_with('{') || !s.ends_with('}') {
        return Err(format!("invalid inline mapping: {s}"));
    }
    let inner = &s[1..s.len() - 1];
    if inner.trim().is_empty() {
        return Ok(YamlValue::Mapping(BTreeMap::new()));
    }
    let mut map = BTreeMap::new();
    for pair in split_flow_items(inner) {
        if let Some((k, v)) = split_key_value(pair.trim()) {
            map.insert(k.to_string(), parse_scalar(v));
        }
    }
    Ok(YamlValue::Mapping(map))
}

/// Split flow items by comma, respecting nested brackets and quotes.
fn split_flow_items(s: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut depth = 0;
    let mut in_quote = false;
    let mut quote_char = ' ';
    let mut start = 0;
    let bytes = s.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        if in_quote {
            if b == quote_char as u8 {
                in_quote = false;
            }
            continue;
        }
        match b {
            b'"' | b'\'' => {
                in_quote = true;
                quote_char = b as char;
            }
            b'[' | b'{' => depth += 1,
            b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                items.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        items.push(&s[start..]);
    }
    items
}
