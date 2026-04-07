//! HTTP client for registry APIs.
//! Unified client: TcpStream for plain HTTP (tests), system curl for HTTPS.

pub mod client;
pub mod github;

/// Check if a package name is available on a registry.
/// Returns Ok(true) if available, Ok(false) if taken.
/// Checks RELEASER_REGISTRY_BASE_URL env var for test override.
pub fn check_name(registry: Registry, name: &str) -> Result<bool, String> {
    let (base, path) = match registry {
        Registry::PyPI => ("https://pypi.org".to_string(), format!("/pypi/{name}/json")),
        Registry::Npm => ("https://registry.npmjs.org".to_string(), format!("/{name}")),
        Registry::CratesIo => ("https://crates.io".to_string(), format!("/api/v1/crates/{name}")),
    };

    let base = std::env::var("RELEASER_REGISTRY_BASE_URL").unwrap_or(base);
    let url = format!("{base}{path}");

    let resp = client::request(&client::Request::get(&url), 1)?;

    match resp.status {
        404 => Ok(true),
        200 => Ok(false),
        429 => Err("rate limited — try again later".into()),
        code => Err(format!("unexpected HTTP {code} from {url}")),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn mock_server(_path: &'static str, status: u16) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).unwrap();
            let response = format!("HTTP/1.1 {status} OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            stream.write_all(response.as_bytes()).unwrap();
        });

        base
    }

    #[test]
    fn test_name_available_returns_true_on_404() {
        let base = mock_server("/pypi/my-new-tool/json", 404);
        unsafe { std::env::set_var("RELEASER_REGISTRY_BASE_URL", &base); }
        let result = check_name(Registry::PyPI, "my-new-tool");
        unsafe { std::env::remove_var("RELEASER_REGISTRY_BASE_URL"); }
        assert!(result.unwrap());
    }

    #[test]
    fn test_name_taken_returns_false_on_200() {
        let base = mock_server("/pypi/requests/json", 200);
        unsafe { std::env::set_var("RELEASER_REGISTRY_BASE_URL", &base); }
        let result = check_name(Registry::PyPI, "requests");
        unsafe { std::env::remove_var("RELEASER_REGISTRY_BASE_URL"); }
        assert!(!result.unwrap());
    }

    #[test]
    fn test_npm_path_format() {
        let base = mock_server("/@my-org/my-tool", 404);
        unsafe { std::env::set_var("RELEASER_REGISTRY_BASE_URL", &base); }
        let result = check_name(Registry::Npm, "@my-org/my-tool");
        unsafe { std::env::remove_var("RELEASER_REGISTRY_BASE_URL"); }
        assert!(result.unwrap());
    }

    #[test]
    fn test_crates_io_path_format() {
        let base = mock_server("/api/v1/crates/my-crate", 404);
        unsafe { std::env::set_var("RELEASER_REGISTRY_BASE_URL", &base); }
        let result = check_name(Registry::CratesIo, "my-crate");
        unsafe { std::env::remove_var("RELEASER_REGISTRY_BASE_URL"); }
        assert!(result.unwrap());
    }
}
