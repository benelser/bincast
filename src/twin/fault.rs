//! Fault injection for digital twins.
//! Wraps a twin's TcpListener and injects errors on configurable requests.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

/// A fault-injecting proxy that sits in front of a backend server.
pub struct FaultProxy {
    pub base_url: String,
    pub faults: Arc<Mutex<FaultConfig>>,
    _handle: thread::JoinHandle<()>,
}

/// Configuration for which requests should fail.
#[derive(Debug, Default)]
pub struct FaultConfig {
    /// Fail the Nth request (1-indexed). 0 = never fail.
    pub fail_on_request: usize,
    /// HTTP status to return on failure.
    pub fail_status: u16,
    /// Error body to return.
    pub fail_body: String,
    /// Count of requests seen so far.
    pub request_count: usize,
    /// Count of injected failures.
    pub failure_count: usize,
}

impl FaultProxy {
    /// Start a proxy that forwards to the given backend URL.
    /// Inject faults based on the FaultConfig.
    pub fn start(backend_url: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{addr}");
        let faults = Arc::new(Mutex::new(FaultConfig {
            fail_on_request: 0,
            fail_status: 500,
            fail_body: r#"{"message":"Internal Server Error"}"#.to_string(),
            ..Default::default()
        }));
        let faults_clone = faults.clone();
        let backend = backend_url.to_string();

        let handle = thread::spawn(move || {
            let backend_addr = backend
                .strip_prefix("http://")
                .unwrap_or(&backend);

            loop {
                let Ok((mut client, _)) = listener.accept() else { break };
                let mut buf = vec![0u8; 1 << 20];
                let n = match client.read(&mut buf) {
                    Ok(0) | Err(_) => continue,
                    Ok(n) => n,
                };

                let mut config = faults_clone.lock().unwrap();
                config.request_count += 1;
                let count = config.request_count;

                // Check if this request should be faulted
                if config.fail_on_request > 0 && count == config.fail_on_request {
                    config.failure_count += 1;
                    let status = config.fail_status;
                    let body = config.fail_body.clone();
                    drop(config);

                    let reason = match status {
                        429 => "Too Many Requests",
                        500 => "Internal Server Error",
                        502 => "Bad Gateway",
                        503 => "Service Unavailable",
                        _ => "Error",
                    };
                    let response = format!(
                        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = client.write_all(response.as_bytes());
                    continue;
                }
                drop(config);

                // Forward to backend
                if let Ok(mut backend_stream) = std::net::TcpStream::connect(backend_addr) {
                    let _ = backend_stream.write_all(&buf[..n]);
                    let mut response = Vec::new();
                    let _ = backend_stream.read_to_end(&mut response);
                    let _ = client.write_all(&response);
                }
            }
        });

        FaultProxy {
            base_url,
            faults,
            _handle: handle,
        }
    }

    /// Configure which request number should fail (1-indexed).
    pub fn fail_request(&self, n: usize, status: u16, body: &str) {
        let mut config = self.faults.lock().unwrap();
        config.fail_on_request = n;
        config.fail_status = status;
        config.fail_body = body.to_string();
    }

    /// Get count of injected failures.
    pub fn failure_count(&self) -> usize {
        self.faults.lock().unwrap().failure_count
    }

    /// Get total request count.
    pub fn request_count(&self) -> usize {
        self.faults.lock().unwrap().request_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::twin::GitHubTwin;
    use crate::http::github as gh;

    #[test]
    fn test_fault_proxy_passes_through() {
        let twin = GitHubTwin::start();
        let proxy = FaultProxy::start(&twin.base_url);

        unsafe {
            std::env::set_var("RELEASER_GITHUB_API_URL", &proxy.base_url);
        }

        let result = gh::create_release("user", "repo", "v1.0.0", "tok");

        unsafe { std::env::remove_var("RELEASER_GITHUB_API_URL"); }

        assert!(result.is_ok(), "passthrough should succeed: {:?}", result);
        assert_eq!(proxy.request_count(), 1);
        assert_eq!(proxy.failure_count(), 0);
    }

    #[test]
    fn test_fault_proxy_injects_500() {
        let twin = GitHubTwin::start();
        let proxy = FaultProxy::start(&twin.base_url);
        proxy.fail_request(1, 500, r#"{"message":"Internal Server Error"}"#);

        unsafe {
            std::env::set_var("RELEASER_GITHUB_API_URL", &proxy.base_url);
        }

        let result = gh::create_release("user", "repo", "v1.0.0", "tok");

        unsafe { std::env::remove_var("RELEASER_GITHUB_API_URL"); }

        // Should fail because the proxy returned 500
        assert!(result.is_err(), "should have failed with injected 500");
        assert_eq!(proxy.failure_count(), 1);
    }

    #[test]
    fn test_fault_proxy_fails_only_nth_request() {
        let twin = GitHubTwin::start();
        let proxy = FaultProxy::start(&twin.base_url);
        // Fail only the 2nd request
        proxy.fail_request(2, 502, r#"{"message":"Bad Gateway"}"#);

        unsafe {
            std::env::set_var("RELEASER_GITHUB_API_URL", &proxy.base_url);
            std::env::set_var("RELEASER_GITHUB_UPLOAD_URL", &proxy.base_url);
        }

        // 1st request: create release — should succeed
        let release = gh::create_release("user", "repo", "v2.0.0", "tok");
        assert!(release.is_ok(), "1st request should pass");

        // 2nd request: upload — should fail
        let dir = std::env::temp_dir().join(format!("fault-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.tar.gz");
        std::fs::write(&file, b"data").unwrap();

        let upload = gh::upload_asset("user", "repo", release.unwrap().id, &file, "tok");
        // Upload retries 3 times but the fault only hits once, so it may succeed on retry
        // depending on timing. The key assertion is that the fault was injected.

        unsafe {
            std::env::remove_var("RELEASER_GITHUB_API_URL");
            std::env::remove_var("RELEASER_GITHUB_UPLOAD_URL");
        }
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(proxy.failure_count(), 1);
        assert!(proxy.request_count() >= 2);
    }
}
