//! GitHub API client — create releases and upload assets.
//! Uses the unified HTTP client (TcpStream for tests, curl for HTTPS).

use std::path::Path;
use super::client::{self, Body, Method, Request};

const API_VERSION: &str = "2026-03-10";

#[derive(Debug)]
pub struct ReleaseResult {
    pub id: u64,
    pub upload_url: String,
    pub html_url: String,
}

fn api_base() -> String {
    std::env::var("RELEASER_GITHUB_API_URL")
        .unwrap_or_else(|_| "https://api.github.com".to_string())
}

fn upload_base() -> String {
    std::env::var("RELEASER_GITHUB_UPLOAD_URL")
        .unwrap_or_else(|_| "https://uploads.github.com".to_string())
}

fn github_headers(token: &str) -> Vec<(&str, String)> {
    vec![
        ("Authorization", format!("Bearer {token}")),
        ("Accept", "application/vnd.github+json".into()),
        ("X-GitHub-Api-Version", API_VERSION.into()),
    ]
}

pub fn create_release(
    owner: &str,
    repo: &str,
    tag: &str,
    token: &str,
) -> Result<ReleaseResult, String> {
    let base = api_base();
    let url = format!("{base}/repos/{owner}/{repo}/releases");
    let body = format!(
        r#"{{"tag_name":"{tag}","name":"{tag}","draft":true,"generate_release_notes":true}}"#
    );

    let mut req = Request::post(&url).with_json(&body);
    for (k, v) in github_headers(token) {
        req = req.with_header(k, v);
    }

    let resp = client::request(&req, 2)?;
    if !resp.is_success() {
        return Err(format!("create release failed ({}): {}", resp.status, resp.body));
    }

    parse_release_response(&resp.body)
}

pub fn upload_asset(
    owner: &str,
    repo: &str,
    release_id: u64,
    file_path: &Path,
    token: &str,
) -> Result<(), String> {
    let file_name = file_path
        .file_name().ok_or("invalid file path")?
        .to_str().ok_or("non-utf8 filename")?;

    let base = upload_base();
    let url = format!("{base}/repos/{owner}/{repo}/releases/{release_id}/assets?name={file_name}");

    let mut req = Request {
        method: Method::Post,
        url: &url,
        headers: Vec::new(),
        body: Body::File(file_path),
    };
    for (k, v) in github_headers(token) {
        req.headers.push((k, v));
    }
    req.headers.push(("Content-Type", "application/octet-stream".into()));

    let resp = client::request(&req, 3)?;
    if !resp.is_success() {
        return Err(format!("upload asset failed ({}): {}", resp.status, resp.body));
    }

    Ok(())
}

pub fn publish_release(
    owner: &str,
    repo: &str,
    release_id: u64,
    token: &str,
) -> Result<(), String> {
    let base = api_base();
    let url = format!("{base}/repos/{owner}/{repo}/releases/{release_id}");
    let body = r#"{"draft":false}"#;

    let mut req = Request {
        method: Method::Patch,
        url: &url,
        headers: Vec::new(),
        body: Body::Json(body),
    };
    for (k, v) in github_headers(token) {
        req.headers.push((k, v));
    }
    req.headers.push(("Content-Type", "application/json".into()));

    let resp = client::request(&req, 2)?;
    if !resp.is_success() {
        return Err(format!("publish release failed ({}): {}", resp.status, resp.body));
    }

    Ok(())
}

pub fn repository_dispatch(
    owner: &str,
    repo: &str,
    event_type: &str,
    version: &str,
    token: &str,
) -> Result<(), String> {
    let base = api_base();
    let url = format!("{base}/repos/{owner}/{repo}/dispatches");
    let body = format!(
        r#"{{"event_type":"{event_type}","client_payload":{{"version":"{version}"}}}}"#
    );

    let mut req = Request::post(&url).with_json(&body);
    for (k, v) in github_headers(token) {
        req = req.with_header(k, v);
    }

    let resp = client::request(&req, 2)?;
    // 204 is success for dispatches
    if resp.status != 204 && !resp.is_success() {
        return Err(format!("repository-dispatch failed ({}): {}", resp.status, resp.body));
    }

    Ok(())
}

fn parse_release_response(response: &str) -> Result<ReleaseResult, String> {
    let id = extract_json_number(response, "id")
        .ok_or_else(|| format!("no 'id' in release response: {response}"))?;
    let upload_url = extract_json_string(response, "upload_url")
        .ok_or("no 'upload_url' in release response")?;
    let html_url = extract_json_string(response, "html_url")
        .ok_or("no 'html_url' in release response")?;

    let upload_url = upload_url.split('{').next().unwrap_or(&upload_url).to_string();

    Ok(ReleaseResult { id, upload_url, html_url })
}

fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let key_pos = json.find(&pattern)?;
    let after = &json[key_pos + pattern.len()..];
    let after = after.trim_start().strip_prefix(':')?;
    let after = after.trim_start().strip_prefix('"')?;
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

fn extract_json_number(json: &str, key: &str) -> Option<u64> {
    let pattern = format!("\"{key}\"");
    let key_pos = json.find(&pattern)?;
    let after = &json[key_pos + pattern.len()..];
    let after = after.trim_start().strip_prefix(':')?;
    let after = after.trim_start();
    let end = after.find(|c: char| !c.is_ascii_digit())?;
    after[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_release_response() {
        let json = r#"{"id":12345,"upload_url":"https://uploads.github.com/repos/user/repo/releases/12345/assets{?name,label}","html_url":"https://github.com/user/repo/releases/tag/v0.1.0"}"#;
        let result = parse_release_response(json).unwrap();
        assert_eq!(result.id, 12345);
        assert!(result.upload_url.ends_with("/assets"));
    }

    #[test]
    fn test_extract_json_string() {
        let json = r#"{"name": "hello", "url": "https://example.com"}"#;
        assert_eq!(extract_json_string(json, "name"), Some("hello".into()));
        assert_eq!(extract_json_string(json, "missing"), None);
    }

    #[test]
    fn test_extract_json_number() {
        let json = r#"{"id": 42, "count": 100}"#;
        assert_eq!(extract_json_number(json, "id"), Some(42));
        assert_eq!(extract_json_number(json, "missing"), None);
    }

    #[test]
    fn test_create_release_with_mock() {
        let twin = crate::twin::GitHubTwin::start();
        unsafe {
            std::env::set_var("RELEASER_GITHUB_API_URL", &twin.base_url);
        }

        let result = create_release("user", "repo", "v0.1.0", "fake-token");

        unsafe { std::env::remove_var("RELEASER_GITHUB_API_URL"); }

        let release = result.unwrap();
        assert_eq!(release.id, 1);
    }

    #[test]
    fn test_upload_asset_with_mock() {
        let twin = crate::twin::GitHubTwin::start();
        unsafe {
            std::env::set_var("RELEASER_GITHUB_API_URL", &twin.base_url);
            std::env::set_var("RELEASER_GITHUB_UPLOAD_URL", &twin.base_url);
        }

        let release = create_release("user", "repo", "v0.2.0", "tok").unwrap();

        let dir = std::env::temp_dir().join(format!("client-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.tar.gz");
        std::fs::write(&file, b"fake content").unwrap();

        let result = upload_asset("user", "repo", release.id, &file, "tok");

        unsafe {
            std::env::remove_var("RELEASER_GITHUB_API_URL");
            std::env::remove_var("RELEASER_GITHUB_UPLOAD_URL");
        }
        let _ = std::fs::remove_dir_all(&dir);

        assert!(result.is_ok(), "upload failed: {:?}", result);
    }

    #[test]
    fn test_repository_dispatch_with_mock() {
        let twin = crate::twin::GitHubTwin::start();
        unsafe { std::env::set_var("RELEASER_GITHUB_API_URL", &twin.base_url); }

        let result = repository_dispatch("user", "repo", "update-formula", "v0.1.0", "tok");

        unsafe { std::env::remove_var("RELEASER_GITHUB_API_URL"); }

        assert!(result.is_ok());
    }
}
