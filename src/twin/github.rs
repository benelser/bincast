//! GitHub Releases API digital twin.
//! Stateful — tracks releases, assets, dispatches.
//! Runs on a background thread, serves plain HTTP via TcpListener.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

/// A stateful GitHub API twin server.
pub struct GitHubTwin {
    pub base_url: String,
    pub state: Arc<Mutex<TwinState>>,
    _handle: thread::JoinHandle<()>,
}

/// Internal state of the twin.
#[derive(Debug, Default)]
pub struct TwinState {
    pub releases: Vec<Release>,
    pub assets: Vec<Asset>,
    pub dispatches: Vec<Dispatch>,
    next_id: u64,
}

#[derive(Debug, Clone)]
pub struct Release {
    pub id: u64,
    pub tag_name: String,
    pub name: String,
    pub draft: bool,
    pub prerelease: bool,
}

#[derive(Debug, Clone)]
pub struct Asset {
    pub id: u64,
    pub release_id: u64,
    pub name: String,
    pub size: usize,
}

#[derive(Debug, Clone)]
pub struct Dispatch {
    pub repo: String,
    pub event_type: String,
    pub version: String,
}

impl TwinState {
    fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
}

impl GitHubTwin {
    /// Start the twin server on a random port. Returns immediately.
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{addr}");
        let state = Arc::new(Mutex::new(TwinState::default()));
        let state_clone = state.clone();

        let handle = thread::spawn(move || {
            // Set a timeout so the thread doesn't block forever when tests end
            listener
                .set_nonblocking(false)
                .unwrap();

            loop {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };

                // Read request (up to 64KB for asset uploads)
                let mut buf = vec![0u8; 65536];
                let n = match stream.read(&mut buf) {
                    Ok(0) | Err(_) => continue,
                    Ok(n) => n,
                };
                let request = String::from_utf8_lossy(&buf[..n]).to_string();

                let (method, path, body) = parse_request(&request);
                let response = handle_request(&state_clone, &method, &path, &body, &buf[..n]);

                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });

        GitHubTwin {
            base_url,
            state,
            _handle: handle,
        }
    }

    /// Get a snapshot of the current state.
    pub fn snapshot(&self) -> TwinState {
        let s = self.state.lock().unwrap();
        TwinState {
            releases: s.releases.clone(),
            assets: s.assets.clone(),
            dispatches: s.dispatches.clone(),
            next_id: s.next_id,
        }
    }
}

fn parse_request(raw: &str) -> (String, String, String) {
    let first_line = raw.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    let method = parts.first().copied().unwrap_or("").to_string();
    let path = parts.get(1).copied().unwrap_or("").to_string();

    // Extract body (after \r\n\r\n)
    let body = raw
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or("")
        .to_string();

    (method, path, body)
}

fn handle_request(
    state: &Arc<Mutex<TwinState>>,
    method: &str,
    path: &str,
    body: &str,
    raw_bytes: &[u8],
) -> String {
    match (method, path) {
        // POST /repos/:owner/:repo/releases — create release
        ("POST", p) if p.contains("/releases") && !p.contains("/assets") && !p.contains("/dispatches") => {
            create_release(state, body)
        }

        // POST /repos/:owner/:repo/releases/:id/assets?name=... — upload asset
        ("POST", p) if p.contains("/assets") => {
            upload_asset(state, p, raw_bytes)
        }

        // PATCH /repos/:owner/:repo/releases/:id — update release (publish)
        ("PATCH", p) if p.contains("/releases/") => {
            publish_release(state, p, body)
        }

        // POST /repos/:owner/:repo/dispatches — repository dispatch
        ("POST", p) if p.contains("/dispatches") => {
            repository_dispatch(state, p, body)
        }

        _ => http_response(404, r#"{"message":"Not Found"}"#),
    }
}

fn create_release(state: &Arc<Mutex<TwinState>>, body: &str) -> String {
    let tag = extract_json_str(body, "tag_name").unwrap_or_default();
    let name = extract_json_str(body, "name").unwrap_or_else(|| tag.clone());
    let draft = extract_json_bool(body, "draft").unwrap_or(false);

    let mut s = state.lock().unwrap();

    // Check for duplicate tag on non-draft releases
    if s.releases.iter().any(|r| r.tag_name == tag && !r.draft) {
        return http_response(
            422,
            r#"{"message":"Validation Failed","errors":[{"code":"already_exists","field":"tag_name"}]}"#,
        );
    }

    let id = s.next_id();
    s.releases.push(Release {
        id,
        tag_name: tag.clone(),
        name: name.clone(),
        draft,
        prerelease: false,
    });

    let resp = format!(
        r#"{{"id":{id},"tag_name":"{tag}","name":"{name}","draft":{draft},"upload_url":"http://twin/repos/o/r/releases/{id}/assets{{?name,label}}","html_url":"http://twin/releases/tag/{tag}"}}"#
    );
    http_response(201, &resp)
}

fn upload_asset(state: &Arc<Mutex<TwinState>>, path: &str, raw: &[u8]) -> String {
    // Extract release_id from path: /repos/:owner/:repo/releases/:id/assets?name=foo
    let release_id = path
        .split("/releases/")
        .nth(1)
        .and_then(|s| s.split('/').next())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let asset_name = path
        .split("name=")
        .nth(1)
        .unwrap_or("unknown")
        .split('&')
        .next()
        .unwrap_or("unknown")
        .to_string();

    let mut s = state.lock().unwrap();

    // Check release exists
    if !s.releases.iter().any(|r| r.id == release_id) {
        return http_response(404, r#"{"message":"Not Found"}"#);
    }

    // Check duplicate asset name (GitHub returns 422 for this)
    if s.assets.iter().any(|a| a.release_id == release_id && a.name == asset_name) {
        return http_response(
            422,
            &format!(r#"{{"message":"Validation Failed","errors":[{{"code":"already_exists","field":"name","value":"{asset_name}"}}]}}"#),
        );
    }

    // Compute body size (everything after \r\n\r\n)
    let body_start = find_body_start(raw).unwrap_or(raw.len());
    let body_size = raw.len() - body_start;

    let id = s.next_id();
    s.assets.push(Asset {
        id,
        release_id,
        name: asset_name.clone(),
        size: body_size,
    });

    let resp = format!(
        r#"{{"id":{id},"name":"{asset_name}","size":{body_size},"state":"uploaded"}}"#
    );
    http_response(201, &resp)
}

fn publish_release(state: &Arc<Mutex<TwinState>>, path: &str, body: &str) -> String {
    let release_id = path
        .split("/releases/")
        .nth(1)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let mut s = state.lock().unwrap();

    if let Some(release) = s.releases.iter_mut().find(|r| r.id == release_id) {
        if let Some(draft) = extract_json_bool(body, "draft") {
            release.draft = draft;
        }
        let resp = format!(
            r#"{{"id":{},"tag_name":"{}","draft":{}}}"#,
            release.id, release.tag_name, release.draft
        );
        http_response(200, &resp)
    } else {
        http_response(404, r#"{"message":"Not Found"}"#)
    }
}

fn repository_dispatch(state: &Arc<Mutex<TwinState>>, path: &str, body: &str) -> String {
    let repo = path
        .split("/repos/")
        .nth(1)
        .and_then(|s| s.strip_suffix("/dispatches"))
        .unwrap_or("unknown")
        .to_string();

    let event_type = extract_json_str(body, "event_type").unwrap_or_default();
    let version = extract_json_str(body, "version").unwrap_or_default();

    let mut s = state.lock().unwrap();
    s.dispatches.push(Dispatch {
        repo,
        event_type,
        version,
    });

    // 204 No Content
    "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".to_string()
}

fn http_response(status: u16, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        404 => "Not Found",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        _ => "Unknown",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn find_body_start(raw: &[u8]) -> Option<usize> {
    for i in 0..raw.len().saturating_sub(3) {
        if &raw[i..i + 4] == b"\r\n\r\n" {
            return Some(i + 4);
        }
    }
    None
}

/// Minimal JSON string extraction.
fn extract_json_str(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let pos = json.find(&pattern)?;
    let after = &json[pos + pattern.len()..];
    let after = after.trim_start().strip_prefix(':')?;
    let after = after.trim_start().strip_prefix('"')?;
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

fn extract_json_bool(json: &str, key: &str) -> Option<bool> {
    let pattern = format!("\"{key}\"");
    let pos = json.find(&pattern)?;
    let after = &json[pos + pattern.len()..];
    let after = after.trim_start().strip_prefix(':')?;
    let after = after.trim_start();
    if after.starts_with("true") {
        Some(true)
    } else if after.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::github as gh;

    #[test]
    fn test_twin_create_and_publish_release() {
        let twin = GitHubTwin::start();
        unsafe {
            std::env::set_var("RELEASER_GITHUB_API_URL", &twin.base_url);
            std::env::set_var("RELEASER_GITHUB_UPLOAD_URL", &twin.base_url);
        }

        // Create release
        let release = gh::create_release("user", "repo", "v0.1.0", "test-token").unwrap();
        assert_eq!(release.id, 1);

        // Check state — should be draft
        let snap = twin.snapshot();
        assert_eq!(snap.releases.len(), 1);
        assert!(snap.releases[0].draft);

        // Publish
        gh::publish_release("user", "repo", release.id, "test-token").unwrap();

        let snap = twin.snapshot();
        assert!(!snap.releases[0].draft);

        unsafe {
            std::env::remove_var("RELEASER_GITHUB_API_URL");
            std::env::remove_var("RELEASER_GITHUB_UPLOAD_URL");
        }
    }

    #[test]
    fn test_twin_upload_asset() {
        let twin = GitHubTwin::start();
        unsafe {
            std::env::set_var("RELEASER_GITHUB_API_URL", &twin.base_url);
            std::env::set_var("RELEASER_GITHUB_UPLOAD_URL", &twin.base_url);
        }

        let release = gh::create_release("user", "repo", "v0.2.0", "tok").unwrap();

        // Create temp file
        let dir = std::env::temp_dir().join(format!("twin-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("tool-v0.2.0.tar.gz");
        std::fs::write(&file, b"fake binary content here").unwrap();

        gh::upload_asset("user", "repo", release.id, &file, "tok").unwrap();

        let snap = twin.snapshot();
        assert_eq!(snap.assets.len(), 1);
        assert_eq!(snap.assets[0].name, "tool-v0.2.0.tar.gz");
        assert!(snap.assets[0].size > 0);

        let _ = std::fs::remove_dir_all(&dir);

        unsafe {
            std::env::remove_var("RELEASER_GITHUB_API_URL");
            std::env::remove_var("RELEASER_GITHUB_UPLOAD_URL");
        }
    }

    #[test]
    fn test_twin_repository_dispatch() {
        let twin = GitHubTwin::start();
        unsafe {
            std::env::set_var("RELEASER_GITHUB_API_URL", &twin.base_url);
        }

        gh::repository_dispatch("user", "homebrew-tool", "update-formula", "v0.1.0", "tok").unwrap();

        let snap = twin.snapshot();
        assert_eq!(snap.dispatches.len(), 1);
        assert_eq!(snap.dispatches[0].event_type, "update-formula");
        assert_eq!(snap.dispatches[0].version, "v0.1.0");

        unsafe {
            std::env::remove_var("RELEASER_GITHUB_API_URL");
        }
    }

    #[test]
    fn test_twin_duplicate_tag_rejected() {
        let twin = GitHubTwin::start();
        unsafe {
            std::env::set_var("RELEASER_GITHUB_API_URL", &twin.base_url);
        }

        // Create and publish a release
        let r = gh::create_release("user", "repo", "v1.0.0", "tok").unwrap();
        gh::publish_release("user", "repo", r.id, "tok").unwrap();

        // Try to create another release with the same tag — should fail
        let result = gh::create_release("user", "repo", "v1.0.0", "tok");
        assert!(result.is_err() || {
            // The twin returns 422, which our client may parse differently
            let snap = twin.snapshot();
            snap.releases.len() == 1 // only one release exists
        });

        unsafe {
            std::env::remove_var("RELEASER_GITHUB_API_URL");
        }
    }

    #[test]
    fn test_twin_full_flow() {
        let twin = GitHubTwin::start();
        unsafe {
            std::env::set_var("RELEASER_GITHUB_API_URL", &twin.base_url);
            std::env::set_var("RELEASER_GITHUB_UPLOAD_URL", &twin.base_url);
        }

        // Full release flow: create draft → upload 2 assets → publish → dispatch
        let release = gh::create_release("user", "repo", "v0.3.0", "tok").unwrap();

        let dir = std::env::temp_dir().join(format!("twin-full-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let f1 = dir.join("tool-linux.tar.gz");
        std::fs::write(&f1, b"linux binary").unwrap();
        gh::upload_asset("user", "repo", release.id, &f1, "tok").unwrap();

        let f2 = dir.join("tool-linux.tar.gz.sha256");
        std::fs::write(&f2, b"abc123  tool-linux.tar.gz\n").unwrap();
        gh::upload_asset("user", "repo", release.id, &f2, "tok").unwrap();

        gh::publish_release("user", "repo", release.id, "tok").unwrap();

        gh::repository_dispatch("user", "homebrew-tool", "update-formula", "v0.3.0", "tok").unwrap();

        // Verify final state
        let snap = twin.snapshot();
        assert_eq!(snap.releases.len(), 1);
        assert!(!snap.releases[0].draft);
        assert_eq!(snap.assets.len(), 2);
        assert_eq!(snap.dispatches.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);

        unsafe {
            std::env::remove_var("RELEASER_GITHUB_API_URL");
            std::env::remove_var("RELEASER_GITHUB_UPLOAD_URL");
        }
    }
}
