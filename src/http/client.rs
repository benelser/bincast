//! Unified HTTP client.
//!
//! Uses std::net::TcpStream for plain HTTP (tests, digital twins).
//! Uses rustls for HTTPS (production APIs — GitHub, PyPI, npm, crates.io).
//!
//! No curl. No shell-outs. No external binaries.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// HTTP method.
#[derive(Debug, Clone, Copy)]
pub enum Method {
    Get,
    Post,
    Patch,
    Put,
}

/// An HTTP request.
pub struct Request<'a> {
    pub method: Method,
    pub url: &'a str,
    pub headers: Vec<(&'a str, String)>,
    pub body: Body<'a>,
}

/// Request body.
pub enum Body<'a> {
    None,
    Json(&'a str),
    Bytes(&'a [u8]),
    File(&'a Path),
    Multipart(Vec<FormField<'a>>),
}

/// A multipart form field.
pub struct FormField<'a> {
    pub name: &'a str,
    pub value: FormValue<'a>,
}

pub enum FormValue<'a> {
    Text(&'a str),
    File(&'a Path),
}

/// HTTP response.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

impl Response {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Perform an HTTP request with retry logic.
pub fn request(req: &Request, retries: u32) -> Result<Response, String> {
    let mut last_err = String::new();

    for attempt in 0..=retries {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(500 * attempt as u64));
        }

        let result = if req.url.starts_with("http://") {
            // Plain HTTP — use TcpStream directly (for tests/mocks)
            request_tcp(req)
        } else if req.url.starts_with("https://") {
            // HTTPS — native TLS via rustls
            request_tls(req)
        } else {
            Err(format!("unsupported URL scheme: {}", req.url))
        };

        match result {
            Ok(resp) if resp.status == 429 => {
                last_err = "rate limited (429)".into();
                continue;
            }
            Ok(resp) if resp.status >= 500 && attempt < retries => {
                last_err = format!("server error ({})", resp.status);
                continue;
            }
            Ok(resp) => return Ok(resp),
            Err(e) if attempt < retries => {
                last_err = e;
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    Err(format!("request failed after {} retries: {last_err}", retries + 1))
}

/// Plain HTTP via TcpStream — for tests and mock servers.
fn request_tcp(req: &Request) -> Result<Response, String> {
    let url = req.url.strip_prefix("http://").unwrap_or(req.url);
    let (host_port, path) = url.split_once('/').unwrap_or((url, "/"));
    let path = format!("/{path}");

    let mut stream = TcpStream::connect(host_port)
        .map_err(|e| format!("connect failed: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();

    let method = match req.method {
        Method::Get => "GET",
        Method::Post => "POST",
        Method::Patch => "PATCH",
        Method::Put => "PUT",
    };

    let body_bytes = match &req.body {
        Body::None => Vec::new(),
        Body::Json(s) => s.as_bytes().to_vec(),
        Body::Bytes(b) => b.to_vec(),
        Body::File(p) => std::fs::read(p).map_err(|e| format!("read file: {e}"))?,
        Body::Multipart(_) => return Err("multipart not supported over plain HTTP".into()),
    };

    let mut request = format!("{method} {path} HTTP/1.1\r\nHost: {host_port}\r\nConnection: close\r\n");
    for (k, v) in &req.headers {
        request.push_str(&format!("{k}: {v}\r\n"));
    }
    if !body_bytes.is_empty() {
        request.push_str(&format!("Content-Length: {}\r\n", body_bytes.len()));
    }
    request.push_str("\r\n");

    // Send headers + body as one write to avoid the twin missing the body
    let mut full_request = request.into_bytes();
    full_request.extend_from_slice(&body_bytes);
    stream.write_all(&full_request).map_err(|e| format!("write: {e}"))?;

    let mut response = Vec::new();
    // Read until connection closes — ignore errors after getting some data
    let mut buf = [0u8; 8192];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => response.extend_from_slice(&buf[..n]),
            Err(e) => {
                if response.is_empty() {
                    return Err(format!("read: {e}"));
                }
                break; // Got some data before error — use it
            }
        }
    }

    let response_str = String::from_utf8_lossy(&response);
    parse_http_response(&response_str)
}

/// HTTPS via rustls — native TLS, no external binaries.
fn request_tls(req: &Request) -> Result<Response, String> {
    let url = req.url.strip_prefix("https://").unwrap_or(req.url);
    let (host_port, path) = url.split_once('/').unwrap_or((url, ""));
    let path = format!("/{path}");

    // Parse host and port
    let (host, port) = if host_port.contains(':') {
        let (h, p) = host_port.split_once(':').unwrap();
        (h, p.parse::<u16>().unwrap_or(443))
    } else {
        (host_port, 443)
    };

    // Set up TLS
    let root_store = rustls::RootCertStore::from_iter(
        webpki_roots::TLS_SERVER_ROOTS.iter().cloned()
    );
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|e| format!("invalid hostname '{host}': {e}"))?;

    let mut conn = rustls::ClientConnection::new(Arc::new(config), server_name)
        .map_err(|e| format!("TLS setup failed: {e}"))?;

    let mut sock = TcpStream::connect(format!("{host}:{port}"))
        .map_err(|e| format!("connect to {host}:{port} failed: {e}"))?;
    sock.set_read_timeout(Some(Duration::from_secs(30))).ok();

    let mut tls = rustls::Stream::new(&mut conn, &mut sock);

    // Build request
    let method = match req.method {
        Method::Get => "GET",
        Method::Post => "POST",
        Method::Patch => "PATCH",
        Method::Put => "PUT",
    };

    let body_bytes = match &req.body {
        Body::None => Vec::new(),
        Body::Json(s) => s.as_bytes().to_vec(),
        Body::Bytes(b) => b.to_vec(),
        Body::File(p) => std::fs::read(p).map_err(|e| format!("read file: {e}"))?,
        Body::Multipart(fields) => build_multipart(fields)?,
    };

    let mut request = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    for (k, v) in &req.headers {
        request.push_str(&format!("{k}: {v}\r\n"));
    }

    // For multipart, add the boundary content-type
    if matches!(&req.body, Body::Multipart(_)) {
        request.push_str("Content-Type: multipart/form-data; boundary=----bincast\r\n");
    }

    if !body_bytes.is_empty() {
        request.push_str(&format!("Content-Length: {}\r\n", body_bytes.len()));
    }
    request.push_str("\r\n");

    let mut full_request = request.into_bytes();
    full_request.extend_from_slice(&body_bytes);

    tls.write_all(&full_request).map_err(|e| format!("TLS write: {e}"))?;

    // Read response
    let mut response = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match tls.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => response.extend_from_slice(&buf[..n]),
            Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                if response.is_empty() {
                    return Err(format!("TLS read: {e}"));
                }
                break;
            }
        }
    }

    let response_str = String::from_utf8_lossy(&response);
    parse_http_response(&response_str)
}

/// Build a multipart/form-data body.
fn build_multipart(fields: &[FormField]) -> Result<Vec<u8>, String> {
    let boundary = "----bincast";
    let mut body = Vec::new();

    for field in fields {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        match &field.value {
            FormValue::Text(t) => {
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n{}\r\n", field.name, t).as_bytes()
                );
            }
            FormValue::File(p) => {
                let filename = p.file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or("file");
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: application/octet-stream\r\n\r\n", field.name, filename).as_bytes()
                );
                let file_data = std::fs::read(p).map_err(|e| format!("read {}: {e}", p.display()))?;
                body.extend_from_slice(&file_data);
                body.extend_from_slice(b"\r\n");
            }
        }
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    Ok(body)
}

fn parse_http_response(raw: &str) -> Result<Response, String> {
    let status_line = raw.lines().next().ok_or("empty response")?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .ok_or("no status code")?
        .parse::<u16>()
        .map_err(|_| format!("invalid status: {status_line}"))?;

    let body = raw
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or("")
        .to_string();

    Ok(Response { status, body })
}

// --- Convenience builders ---

impl<'a> Request<'a> {
    pub fn get(url: &'a str) -> Self {
        Request {
            method: Method::Get,
            url,
            headers: Vec::new(),
            body: Body::None,
        }
    }

    pub fn post(url: &'a str) -> Self {
        Request {
            method: Method::Post,
            url,
            headers: Vec::new(),
            body: Body::None,
        }
    }

    pub fn with_header(mut self, key: &'a str, value: impl Into<String>) -> Self {
        self.headers.push((key, value.into()));
        self
    }

    pub fn with_bearer(self, token: &str) -> Self {
        self.with_header("Authorization", format!("Bearer {token}"))
    }

    pub fn with_json(mut self, json: &'a str) -> Self {
        self.headers.push(("Content-Type", "application/json".into()));
        self.body = Body::Json(json);
        self
    }

    pub fn with_file(mut self, path: &'a Path) -> Self {
        self.headers.push(("Content-Type", "application/octet-stream".into()));
        self.body = Body::File(path);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    fn mock_server(status: u16, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");

        thread::spawn(move || {
            // Handle up to 5 requests (for retry tests)
            for _ in 0..5 {
                let Ok((mut stream, _)) = listener.accept() else { break };
                let mut buf = [0u8; 8192];
                let _ = stream.read(&mut buf);
                let resp = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
            }
        });

        base
    }

    #[test]
    fn test_get_200() {
        let base = mock_server(200, r#"{"ok":true}"#);
        let url = format!("{base}/test");
        let resp = request(&Request::get(&url), 0).unwrap();
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("ok"));
        assert!(resp.is_success());
    }

    #[test]
    fn test_get_404() {
        let base = mock_server(404, "not found");
        let url = format!("{base}/missing");
        let resp = request(&Request::get(&url), 0).unwrap();
        assert_eq!(resp.status, 404);
        assert!(!resp.is_success());
    }

    #[test]
    fn test_post_json() {
        let base = mock_server(201, r#"{"id":1}"#);
        let url = format!("{base}/create");
        let resp = request(
            &Request::post(&url)
                .with_bearer("test-token")
                .with_json(r#"{"name":"test"}"#),
            0,
        ).unwrap();
        assert_eq!(resp.status, 201);
    }

    #[test]
    fn test_retry_on_500() {
        // Server returns 500 twice then 200
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");

        thread::spawn(move || {
            for i in 0..3 {
                let Ok((mut stream, _)) = listener.accept() else { break };
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let (status, body) = if i < 2 { (500, "error") } else { (200, "ok") };
                let resp = format!("HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = stream.write_all(resp.as_bytes());
            }
        });

        let url = format!("{base}/retry");
        let resp = request(&Request::get(&url), 2).unwrap();
        assert_eq!(resp.status, 200);
    }

    #[test]
    fn test_no_retry_on_400() {
        let base = mock_server(400, "bad request");
        let url = format!("{base}/bad");
        let resp = request(&Request::get(&url), 2).unwrap();
        // 400 is NOT retried — returns immediately
        assert_eq!(resp.status, 400);
    }

    #[test]
    fn test_response_is_success() {
        assert!(Response { status: 200, body: String::new() }.is_success());
        assert!(Response { status: 201, body: String::new() }.is_success());
        assert!(Response { status: 204, body: String::new() }.is_success());
        assert!(!Response { status: 400, body: String::new() }.is_success());
        assert!(!Response { status: 404, body: String::new() }.is_success());
        assert!(!Response { status: 500, body: String::new() }.is_success());
    }
}
