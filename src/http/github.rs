//! GitHub API client — create releases and upload assets.
//! Uses curl via std::process::Command. Zero deps.
//!
//! For hermetic testing, set RELEASER_GITHUB_API_URL to point at a mock server.
//! The mock server receives plain HTTP (no TLS).

use std::path::Path;
use std::process::Command;

/// Result of creating a GitHub release.
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

/// Create a draft GitHub Release.
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

    let output = Command::new("curl")
        .args([
            "-s",
            "-X", "POST",
            "-H", &format!("Authorization: Bearer {token}"),
            "-H", "Accept: application/vnd.github+json",
            "-H", "X-GitHub-Api-Version: 2026-03-10",
            "-H", "Content-Type: application/json",
            "-d", &body,
            &url,
        ])
        .output()
        .map_err(|e| format!("failed to run curl: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("curl failed: {stderr}"));
    }

    let response = String::from_utf8_lossy(&output.stdout);
    parse_release_response(&response)
}

/// Upload a file as a release asset.
pub fn upload_asset(
    owner: &str,
    repo: &str,
    release_id: u64,
    file_path: &Path,
    token: &str,
) -> Result<(), String> {
    let file_name = file_path
        .file_name()
        .ok_or("invalid file path")?
        .to_str()
        .ok_or("non-utf8 filename")?;

    let base = upload_base();
    let url = format!(
        "{base}/repos/{owner}/{repo}/releases/{release_id}/assets?name={file_name}"
    );

    let file_str = file_path.to_str().ok_or("non-utf8 path")?;

    // Retry up to 3 times
    let mut last_err = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(500 * attempt as u64));
        }

        let output = Command::new("curl")
            .args([
                "-s",
                "-X", "POST",
                "-H", &format!("Authorization: Bearer {token}"),
                "-H", "Accept: application/vnd.github+json",
                "-H", "Content-Type: application/octet-stream",
                "-T", file_str,
                &url,
            ])
            .output()
            .map_err(|e| format!("failed to run curl: {e}"))?;

        let response = String::from_utf8_lossy(&output.stdout);

        if output.status.success() && !response.contains("\"errors\"") {
            return Ok(());
        }

        last_err = format!("upload failed (attempt {}): {}", attempt + 1, response);
    }

    Err(last_err)
}

/// Publish a draft release (flip draft to false).
pub fn publish_release(
    owner: &str,
    repo: &str,
    release_id: u64,
    token: &str,
) -> Result<(), String> {
    let base = api_base();
    let url = format!("{base}/repos/{owner}/{repo}/releases/{release_id}");

    let output = Command::new("curl")
        .args([
            "-s",
            "-X", "PATCH",
            "-H", &format!("Authorization: Bearer {token}"),
            "-H", "Accept: application/vnd.github+json",
            "-H", "X-GitHub-Api-Version: 2026-03-10",
            "-H", "Content-Type: application/json",
            "-d", r#"{"draft":false}"#,
            &url,
        ])
        .output()
        .map_err(|e| format!("failed to run curl: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("publish release failed: {stderr}"));
    }

    Ok(())
}

/// Send a repository-dispatch event.
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

    let output = Command::new("curl")
        .args([
            "-s",
            "-o", "/dev/null",
            "-w", "%{http_code}",
            "-X", "POST",
            "-H", &format!("Authorization: Bearer {token}"),
            "-H", "Accept: application/vnd.github+json",
            "-H", "Content-Type: application/json",
            "-d", &body,
            &url,
        ])
        .output()
        .map_err(|e| format!("failed to run curl: {e}"))?;

    let status = String::from_utf8_lossy(&output.stdout);
    if status.trim() == "204" {
        Ok(())
    } else {
        Err(format!("repository-dispatch failed with HTTP {status}"))
    }
}

/// Parse the JSON response from create release.
fn parse_release_response(response: &str) -> Result<ReleaseResult, String> {
    let id = extract_json_number(response, "id")
        .ok_or_else(|| format!("no 'id' in release response: {response}"))?;
    let upload_url = extract_json_string(response, "upload_url")
        .ok_or("no 'upload_url' in release response")?;
    let html_url = extract_json_string(response, "html_url")
        .ok_or("no 'html_url' in release response")?;

    let upload_url = upload_url
        .split('{')
        .next()
        .unwrap_or(&upload_url)
        .to_string();

    Ok(ReleaseResult {
        id,
        upload_url,
        html_url,
    })
}

fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let key_pos = json.find(&pattern)?;
    let after_key = &json[key_pos + pattern.len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    if !after_colon.starts_with('"') {
        return None;
    }
    let value_start = 1;
    let value_end = after_colon[value_start..].find('"')?;
    Some(after_colon[value_start..value_start + value_end].to_string())
}

fn extract_json_number(json: &str, key: &str) -> Option<u64> {
    let pattern = format!("\"{key}\"");
    let key_pos = json.find(&pattern)?;
    let after_key = &json[key_pos + pattern.len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    let end = after_colon.find(|c: char| !c.is_ascii_digit())?;
    after_colon[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn test_parse_release_response() {
        let json = r#"{"id":12345,"upload_url":"https://uploads.github.com/repos/user/repo/releases/12345/assets{?name,label}","html_url":"https://github.com/user/repo/releases/tag/v0.1.0","tag_name":"v0.1.0"}"#;
        let result = parse_release_response(json).unwrap();
        assert_eq!(result.id, 12345);
        assert_eq!(
            result.upload_url,
            "https://uploads.github.com/repos/user/repo/releases/12345/assets"
        );
        assert_eq!(
            result.html_url,
            "https://github.com/user/repo/releases/tag/v0.1.0"
        );
    }

    #[test]
    fn test_parse_release_response_with_whitespace() {
        let json = r#"{
  "id": 67890,
  "upload_url": "https://uploads.github.com/repos/o/r/releases/67890/assets{?name,label}",
  "html_url": "https://github.com/o/r/releases/tag/v0.2.0",
  "name": "v0.2.0"
}"#;
        let result = parse_release_response(json).unwrap();
        assert_eq!(result.id, 67890);
        assert!(result.upload_url.ends_with("/assets"));
    }

    #[test]
    fn test_extract_json_string() {
        let json = r#"{"name": "hello", "url": "https://example.com"}"#;
        assert_eq!(extract_json_string(json, "name"), Some("hello".into()));
        assert_eq!(extract_json_string(json, "url"), Some("https://example.com".into()));
        assert_eq!(extract_json_string(json, "missing"), None);
    }

    #[test]
    fn test_extract_json_number() {
        let json = r#"{"id": 42, "count": 100}"#;
        assert_eq!(extract_json_number(json, "id"), Some(42));
        assert_eq!(extract_json_number(json, "count"), Some(100));
        assert_eq!(extract_json_number(json, "missing"), None);
    }

    /// Start a single-request mock server that responds once then exits.
    fn mock_one_request(status: u16, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");

        thread::spawn(move || {
            listener.set_nonblocking(false).unwrap();
            let Ok((mut stream, _)) = listener.accept() else { return };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        });

        base
    }

    #[test]
    fn test_create_release_with_mock() {
        let base = mock_one_request(
            201,
            r#"{"id":99999,"upload_url":"http://mock/upload{?name,label}","html_url":"http://mock/releases/v0.1.0"}"#,
        );
        unsafe { std::env::set_var("RELEASER_GITHUB_API_URL", &base); }

        let result = create_release("user", "repo", "v0.1.0", "fake-token");

        unsafe { std::env::remove_var("RELEASER_GITHUB_API_URL"); }

        let release = result.unwrap();
        assert_eq!(release.id, 99999);
        assert!(release.html_url.contains("v0.1.0"));
    }

    #[test]
    fn test_upload_asset_with_mock() {
        let base = mock_one_request(201, r#"{"id":1,"name":"test.tar.gz"}"#);
        unsafe {
            std::env::set_var("RELEASER_GITHUB_UPLOAD_URL", &base);
        }

        let dir = std::env::temp_dir().join(format!(
            "releaser-upload-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.tar.gz");
        std::fs::write(&file, b"fake archive content").unwrap();

        let result = upload_asset("user", "repo", 99999, &file, "fake-token");

        unsafe {
            std::env::remove_var("RELEASER_GITHUB_UPLOAD_URL");
        }
        let _ = std::fs::remove_dir_all(&dir);

        assert!(result.is_ok(), "upload failed: {:?}", result);
    }

    #[test]
    fn test_repository_dispatch_with_mock() {
        let base = mock_one_request(204, "");
        unsafe { std::env::set_var("RELEASER_GITHUB_API_URL", &base); }

        let result = repository_dispatch("user", "repo", "update-formula", "v0.1.0", "fake-token");

        unsafe { std::env::remove_var("RELEASER_GITHUB_API_URL"); }

        assert!(result.is_ok(), "dispatch failed: {:?}", result);
    }
}
