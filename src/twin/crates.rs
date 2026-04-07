//! crates.io publish API digital twin.
//! Models the publish endpoint used by cargo publish.
//! Tracks published crates, rejects duplicates.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

pub struct CratesTwin {
    pub base_url: String,
    pub state: Arc<Mutex<CratesState>>,
    _handle: thread::JoinHandle<()>,
}

#[derive(Debug, Default)]
pub struct CratesState {
    pub publishes: Vec<CratePublish>,
    pub lookups: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CratePublish {
    pub name: String,
    pub version: String,
}

impl CratesTwin {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{addr}");
        let state = Arc::new(Mutex::new(CratesState::default()));
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
                let (method, path) = parse_method_path(&request);

                let response = match (method.as_str(), path.as_str()) {
                    // PUT /api/v1/crates/new — publish a crate
                    ("PUT", "/api/v1/crates/new") => {
                        handle_publish(&state_clone, &request)
                    }
                    // GET /api/v1/crates/:name — check if crate exists
                    ("GET", p) if p.starts_with("/api/v1/crates/") => {
                        handle_lookup(&state_clone, p)
                    }
                    _ => http_response(404, r#"{"errors":[{"detail":"Not Found"}]}"#),
                };

                let _ = stream.write_all(response.as_bytes());
            }
        });

        CratesTwin {
            base_url,
            state,
            _handle: handle,
        }
    }

    pub fn snapshot(&self) -> CratesState {
        let s = self.state.lock().unwrap();
        CratesState {
            publishes: s.publishes.clone(),
            lookups: s.lookups.clone(),
        }
    }
}

fn parse_method_path(request: &str) -> (String, String) {
    let first = request.lines().next().unwrap_or("");
    let parts: Vec<&str> = first.split_whitespace().collect();
    (
        parts.first().copied().unwrap_or("").to_string(),
        parts.get(1).copied().unwrap_or("").to_string(),
    )
}

fn handle_publish(state: &Arc<Mutex<CratesState>>, request: &str) -> String {
    // cargo publish sends a binary payload with metadata + .crate tarball.
    // The metadata is a JSON blob prepended with its length as a u32 LE.
    // For our twin, extract name and version from the JSON in the body.
    let body = request.split("\r\n\r\n").nth(1).unwrap_or("");

    let name = extract_json_str(body, "name")
        .unwrap_or_else(|| "unknown".to_string());
    let version = extract_json_str(body, "vers")
        .or_else(|| extract_json_str(body, "version"))
        .unwrap_or_else(|| "0.0.0".to_string());

    let mut s = state.lock().unwrap();

    // Reject duplicate name+version
    if s.publishes.iter().any(|p| p.name == name && p.version == version) {
        return http_response(
            409,
            &format!(r#"{{"errors":[{{"detail":"crate version `{name}@{version}` already exists"}}]}}"#),
        );
    }

    s.publishes.push(CratePublish {
        name: name.clone(),
        version: version.clone(),
    });

    http_response(200, r#"{"warnings":{"invalid_categories":[],"invalid_badges":[],"other":[]}}"#)
}

fn handle_lookup(state: &Arc<Mutex<CratesState>>, path: &str) -> String {
    let crate_name = path
        .strip_prefix("/api/v1/crates/")
        .unwrap_or("")
        .split('/')
        .next()
        .unwrap_or("");

    let mut s = state.lock().unwrap();
    s.lookups.push(crate_name.to_string());

    if s.publishes.iter().any(|p| p.name == crate_name) {
        http_response(
            200,
            &format!(r#"{{"crate":{{"id":"{crate_name}","name":"{crate_name}"}}}}"#),
        )
    } else {
        http_response(404, r#"{"errors":[{"detail":"Not Found"}]}"#)
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

fn http_response(status: u16, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
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

    fn publish_to(twin: &CratesTwin, name: &str, version: &str) -> String {
        let addr = twin.base_url.strip_prefix("http://").unwrap();
        let mut stream = std::net::TcpStream::connect(addr).unwrap();
        let body = format!(r#"{{"name":"{name}","vers":"{version}"}}"#);
        let request = format!(
            "PUT /api/v1/crates/new HTTP/1.1\r\nHost: mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    fn lookup(twin: &CratesTwin, name: &str) -> String {
        let addr = twin.base_url.strip_prefix("http://").unwrap();
        let mut stream = std::net::TcpStream::connect(addr).unwrap();
        let request = format!(
            "GET /api/v1/crates/{name} HTTP/1.1\r\nHost: mock\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn test_crates_twin_publish() {
        let twin = CratesTwin::start();
        let r = publish_to(&twin, "my-crate", "0.1.0");
        assert!(r.contains("200"), "expected 200: {r}");

        let snap = twin.snapshot();
        assert_eq!(snap.publishes.len(), 1);
        assert_eq!(snap.publishes[0].name, "my-crate");
        assert_eq!(snap.publishes[0].version, "0.1.0");
    }

    #[test]
    fn test_crates_twin_rejects_duplicate() {
        let twin = CratesTwin::start();
        publish_to(&twin, "my-crate", "0.1.0");
        let r = publish_to(&twin, "my-crate", "0.1.0");
        assert!(r.contains("409"), "duplicate should 409: {r}");
        assert!(r.contains("already exists"));
    }

    #[test]
    fn test_crates_twin_allows_different_versions() {
        let twin = CratesTwin::start();
        publish_to(&twin, "my-crate", "0.1.0");
        let r = publish_to(&twin, "my-crate", "0.2.0");
        assert!(r.contains("200"));
        assert_eq!(twin.snapshot().publishes.len(), 2);
    }

    #[test]
    fn test_crates_twin_lookup_not_found() {
        let twin = CratesTwin::start();
        let r = lookup(&twin, "nonexistent");
        assert!(r.contains("404"));
    }

    #[test]
    fn test_crates_twin_lookup_found_after_publish() {
        let twin = CratesTwin::start();
        publish_to(&twin, "my-crate", "0.1.0");
        let r = lookup(&twin, "my-crate");
        assert!(r.contains("200"));
        assert!(r.contains("my-crate"));
    }
}
