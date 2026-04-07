//! npm registry digital twin.
//! Models the publish endpoint. Tracks packages, rejects duplicate versions.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

pub struct NpmTwin {
    pub base_url: String,
    pub state: Arc<Mutex<NpmState>>,
    _handle: thread::JoinHandle<()>,
}

#[derive(Debug, Default)]
pub struct NpmState {
    pub packages: Vec<PublishedPackage>,
}

#[derive(Debug, Clone)]
pub struct PublishedPackage {
    pub name: String,
    pub version: String,
}

impl NpmTwin {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{addr}");
        let state = Arc::new(Mutex::new(NpmState::default()));
        let state_clone = state.clone();

        let handle = thread::spawn(move || {
            loop {
                let Ok((mut stream, _)) = listener.accept() else { break };
                let mut buf = vec![0u8; 1 << 20];
                let n = match stream.read(&mut buf) {
                    Ok(0) | Err(_) => continue,
                    Ok(n) => n,
                };
                let request = String::from_utf8_lossy(&buf[..n]).to_string();

                let response = if request.starts_with("PUT") {
                    handle_publish(&state_clone, &request)
                } else if request.starts_with("GET") {
                    handle_get(&state_clone, &request)
                } else {
                    http_response(405, r#"{"error":"Method Not Allowed"}"#)
                };

                let _ = stream.write_all(response.as_bytes());
            }
        });

        NpmTwin {
            base_url,
            state,
            _handle: handle,
        }
    }

    pub fn snapshot(&self) -> NpmState {
        let s = self.state.lock().unwrap();
        NpmState {
            packages: s.packages.clone(),
        }
    }
}

fn handle_publish(state: &Arc<Mutex<NpmState>>, request: &str) -> String {
    // Extract package name from PUT /@scope/package-name
    let path = request
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .nth(1)
        .unwrap_or("");

    let pkg_name = path.trim_start_matches('/').to_string();

    // Extract version from the JSON body
    let body_start = request.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
    let body = &request[body_start..];
    let version = extract_json_str(body, "version")
        .or_else(|| extract_dist_tag_version(body))
        .unwrap_or_else(|| "0.0.0".to_string());

    let mut s = state.lock().unwrap();

    // Reject duplicate name+version
    if s.packages.iter().any(|p| p.name == pkg_name && p.version == version) {
        return http_response(
            409,
            &format!(r#"{{"error":"cannot publish over previously published version \"{version}\""}}"#),
        );
    }

    s.packages.push(PublishedPackage {
        name: pkg_name.clone(),
        version: version.clone(),
    });

    http_response(
        200,
        &format!(r#"{{"ok":"created new package version","name":"{pkg_name}","version":"{version}"}}"#),
    )
}

fn handle_get(state: &Arc<Mutex<NpmState>>, request: &str) -> String {
    let path = request
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .nth(1)
        .unwrap_or("");

    let pkg_name = path.trim_start_matches('/');

    let s = state.lock().unwrap();
    if s.packages.iter().any(|p| p.name == pkg_name) {
        http_response(200, &format!(r#"{{"name":"{pkg_name}"}}"#))
    } else {
        http_response(404, r#"{"error":"not found"}"#)
    }
}

fn extract_json_str(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let pos = json.find(&pattern)?;
    let after = &json[pos + pattern.len()..];
    let after = after.trim_start().strip_prefix(':')?;
    let after = after.trim_start().strip_prefix('"')?;
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

fn extract_dist_tag_version(body: &str) -> Option<String> {
    // npm publish sends versions as keys under "versions": {"0.1.0": {...}}
    let pos = body.find("\"versions\"")?;
    let after = &body[pos + 10..];
    let after = after.trim_start().strip_prefix(':')?;
    let after = after.trim_start().strip_prefix('{')?;
    let after = after.trim_start().strip_prefix('"')?;
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

fn http_response(status: u16, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        _ => "Unknown",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publish_to(twin: &NpmTwin, name: &str, version: &str) -> String {
        let addr = twin.base_url.strip_prefix("http://").unwrap();
        let mut stream = std::net::TcpStream::connect(addr).unwrap();
        let body = format!(r#"{{"name":"{name}","version":"{version}"}}"#);
        let request = format!(
            "PUT /{name} HTTP/1.1\r\nHost: mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn test_npm_twin_accepts_publish() {
        let twin = NpmTwin::start();
        let response = publish_to(&twin, "@my-org/my-tool-darwin-arm64", "0.1.0");
        assert!(response.contains("200"), "expected 200: {response}");

        let snap = twin.snapshot();
        assert_eq!(snap.packages.len(), 1);
        assert_eq!(snap.packages[0].name, "@my-org/my-tool-darwin-arm64");
        assert_eq!(snap.packages[0].version, "0.1.0");
    }

    #[test]
    fn test_npm_twin_rejects_duplicate_version() {
        let twin = NpmTwin::start();
        let r1 = publish_to(&twin, "@my-org/tool", "0.1.0");
        assert!(r1.contains("200"));

        let r2 = publish_to(&twin, "@my-org/tool", "0.1.0");
        assert!(r2.contains("409"), "duplicate should fail: {r2}");
        assert!(r2.contains("cannot publish over"));
    }

    #[test]
    fn test_npm_twin_allows_different_versions() {
        let twin = NpmTwin::start();
        publish_to(&twin, "@my-org/tool", "0.1.0");
        let r2 = publish_to(&twin, "@my-org/tool", "0.2.0");
        assert!(r2.contains("200"));

        let snap = twin.snapshot();
        assert_eq!(snap.packages.len(), 2);
    }

    #[test]
    fn test_npm_twin_get_returns_404_for_unknown() {
        let twin = NpmTwin::start();
        let addr = twin.base_url.strip_prefix("http://").unwrap();
        let mut stream = std::net::TcpStream::connect(addr).unwrap();
        let request = "GET /@unknown/pkg HTTP/1.1\r\nHost: mock\r\nConnection: close\r\n\r\n";
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.contains("404"));
    }

    #[test]
    fn test_npm_twin_get_returns_200_for_known() {
        let twin = NpmTwin::start();
        publish_to(&twin, "@my-org/tool", "0.1.0");

        let addr = twin.base_url.strip_prefix("http://").unwrap();
        let mut stream = std::net::TcpStream::connect(addr).unwrap();
        let request = "GET /@my-org/tool HTTP/1.1\r\nHost: mock\r\nConnection: close\r\n\r\n";
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.contains("200"));
    }
}
