//! README transformation for PyPI.
//! Strips HTML elements that PyPI's renderer doesn't support.

/// Transform a README for PyPI compatibility.
/// Strips: <picture>, <source>, <details>, <summary>, HTML comments.
/// Converts relative image URLs to absolute GitHub URLs.
pub fn transform_for_pypi(readme: &str, owner: &str, repo: &str, branch: &str) -> String {
    let mut output = String::with_capacity(readme.len());
    let mut chars = readme.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '<' {
            // Collect the tag
            let mut tag = String::from('<');
            for c in chars.by_ref() {
                tag.push(c);
                if c == '>' {
                    break;
                }
            }

            let tag_lower = tag.to_lowercase();

            // Strip these tags entirely (both opening and closing)
            if tag_lower.starts_with("<picture")
                || tag_lower.starts_with("</picture")
                || tag_lower.starts_with("<source")
                || tag_lower.starts_with("</source")
                || tag_lower.starts_with("<details")
                || tag_lower.starts_with("</details")
                || tag_lower.starts_with("<summary")
                || tag_lower.starts_with("</summary")
            {
                // Skip this tag
                continue;
            }

            // Strip HTML comments
            if tag.starts_with("<!--") {
                // Find end of comment
                if !tag.contains("-->") {
                    for c in chars.by_ref() {
                        tag.push(c);
                        if tag.ends_with("-->") {
                            break;
                        }
                    }
                }
                continue;
            }

            output.push_str(&tag);
        } else {
            output.push(ch);
        }
    }

    // Convert relative image URLs to absolute
    // ![alt](./image.png) → ![alt](https://raw.githubusercontent.com/owner/repo/branch/image.png)
    let base_url = format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}/");
    let output = output.replace("](./", &format!("]({base_url}"));
    let output = output.replace("](../", &format!("]({base_url}../"));

    // Clean up empty lines left by stripped tags
    let lines: Vec<&str> = output.lines().collect();
    let mut cleaned = Vec::new();
    let mut prev_empty = false;
    for line in lines {
        let is_empty = line.trim().is_empty();
        if is_empty && prev_empty {
            continue;
        }
        cleaned.push(line);
        prev_empty = is_empty;
    }

    cleaned.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strips_picture_tags() {
        let input = r#"<picture>
  <source media="(prefers-color-scheme: dark)" srcset="dark.png">
  <source media="(prefers-color-scheme: light)" srcset="light.png">
  <img alt="Logo" src="logo.png">
</picture>"#;
        let output = transform_for_pypi(input, "user", "repo", "main");
        assert!(!output.contains("<picture>"));
        assert!(!output.contains("<source"));
        assert!(!output.contains("</picture>"));
        assert!(output.contains("<img"), "should keep img tags");
    }

    #[test]
    fn test_strips_details_summary() {
        let input = r#"<details>
<summary>Click to expand</summary>

Some hidden content here.

</details>"#;
        let output = transform_for_pypi(input, "user", "repo", "main");
        assert!(!output.contains("<details>"));
        assert!(!output.contains("<summary>"));
        assert!(!output.contains("</details>"));
        assert!(output.contains("Some hidden content here."));
    }

    #[test]
    fn test_strips_html_comments() {
        let input = "before <!-- this is a comment --> after";
        let output = transform_for_pypi(input, "user", "repo", "main");
        assert!(!output.contains("<!--"));
        assert!(output.contains("before"));
        assert!(output.contains("after"));
    }

    #[test]
    fn test_converts_relative_image_urls() {
        let input = "![logo](./assets/logo.png)";
        let output = transform_for_pypi(input, "user", "repo", "main");
        assert!(output.contains("https://raw.githubusercontent.com/user/repo/main/assets/logo.png"));
    }

    #[test]
    fn test_preserves_absolute_urls() {
        let input = "![logo](https://example.com/logo.png)";
        let output = transform_for_pypi(input, "user", "repo", "main");
        assert_eq!(output, input);
    }

    #[test]
    fn test_preserves_normal_markdown() {
        let input = "# My Tool\n\nThis is a great tool.\n\n## Features\n\n- Fast\n- Safe";
        let output = transform_for_pypi(input, "user", "repo", "main");
        assert_eq!(output, input);
    }

    #[test]
    fn test_removes_excess_blank_lines() {
        let input = "<picture>\n</picture>\n\n\n\nContent here";
        let output = transform_for_pypi(input, "user", "repo", "main");
        assert!(!output.contains("\n\n\n"));
    }
}
