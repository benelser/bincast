//! HTTP client for registry APIs.
//! Uses curl for HTTPS (zero deps), raw TcpStream for mock servers in tests.

pub mod github;

use std::io::{Read, Write};
use std::net::TcpStream;

/// Check if a package name is available on a registry.
/// Returns Ok(true) if available, Ok(false) if taken.
/// Checks RELEASER_REGISTRY_BASE_URL env var for test override.
pub fn check_name(registry: Registry, name: &str) -> Result<bool, String> {
    let override_url = std::env::var("RELEASER_REGISTRY_BASE_URL").ok();
    check_name_with_override(registry, name, override_url.as_deref())
}

/// Check name availability, optionally against a mock server base URL.
pub fn check_name_with_override(
    registry: Registry,
    name: &str,
    base_url_override: Option<&str>,
) -> Result<bool, String> {
    let (host, path) = match registry {
        Registry::PyPI => ("pypi.org".to_string(), format!("/pypi/{name}/json")),
        Registry::Npm => ("registry.npmjs.org".to_string(), format!("/{name}")),
        Registry::CratesIo => ("crates.io".to_string(), format!("/api/v1/crates/{name}")),
    };

    let status = if let Some(base_url) = base_url_override {
        http_get_status_mock(base_url, &path)?
    } else {
        https_get_status(&host, &path)?
    };

    match status {
        404 => Ok(true),  // not found = available
        200 => Ok(false), // exists = taken
        429 => Err("rate limited — try again later".into()),
        code => Err(format!("unexpected HTTP {code} from {host}{path}")),
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Registry {
    PyPI,
    Npm,
    CratesIo,
}

impl Registry {
    pub fn display_name(&self) -> &str {
        match self {
            Registry::PyPI => "PyPI",
            Registry::Npm => "npm",
            Registry::CratesIo => "crates.io",
        }
    }
}

/// HTTPS GET via curl — zero deps, works everywhere.
fn https_get_status(host: &str, path: &str) -> Result<u16, String> {
    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "-o", "/dev/null",
            "-w", "%{http_code}",
            "--max-time", "10",
            &format!("https://{host}{path}"),
        ])
        .output()
        .map_err(|e| format!("failed to run curl: {e}"))?;

    let status_str = String::from_utf8_lossy(&output.stdout);
    status_str
        .trim()
        .parse::<u16>()
        .map_err(|_| format!("invalid status from curl: {status_str}"))
}

/// Plain HTTP GET against a mock server (for tests).
fn http_get_status_mock(base_url: &str, path: &str) -> Result<u16, String> {
    let addr = base_url
        .strip_prefix("http://")
        .ok_or_else(|| format!("mock base_url must start with http://: {base_url}"))?;

    let mut stream = TcpStream::connect(addr)
        .map_err(|e| format!("failed to connect to mock server at {addr}: {e}"))?;

    let request = format!("GET {path} HTTP/1.1\r\nHost: mock\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("failed to send request: {e}"))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| format!("failed to read response: {e}"))?;

    let status_line = response
        .lines()
        .next()
        .ok_or_else(|| "empty response from mock".to_string())?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| format!("malformed status line: {status_line}"))?
        .parse::<u16>()
        .map_err(|_| format!("invalid status code in: {status_line}"))?;

    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    /// Start a mock HTTP server that responds with a specific status for a given path.
    fn mock_server(_path: &'static str, status: u16) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{addr}");

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).unwrap();
            let response = format!("HTTP/1.1 {status} OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            stream.write_all(response.as_bytes()).unwrap();
        });

        base_url
    }

    #[test]
    fn test_name_available_returns_true_on_404() {
        let base = mock_server("/pypi/my-new-tool/json", 404);
        let result = check_name_with_override(Registry::PyPI, "my-new-tool", Some(&base));
        assert!(result.unwrap(), "expected name to be available");
    }

    #[test]
    fn test_name_taken_returns_false_on_200() {
        let base = mock_server("/pypi/requests/json", 200);
        let result = check_name_with_override(Registry::PyPI, "requests", Some(&base));
        assert!(!result.unwrap(), "expected name to be taken");
    }

    #[test]
    fn test_npm_path_format() {
        let base = mock_server("/@my-org/my-tool", 404);
        let result = check_name_with_override(Registry::Npm, "@my-org/my-tool", Some(&base));
        assert!(result.unwrap());
    }

    #[test]
    fn test_crates_io_path_format() {
        let base = mock_server("/api/v1/crates/my-crate", 404);
        let result = check_name_with_override(Registry::CratesIo, "my-crate", Some(&base));
        assert!(result.unwrap());
    }
}
