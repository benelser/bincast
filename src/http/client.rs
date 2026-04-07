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
    pub headers: Vec<(String, String)>,
}

impl Response {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        let lower = name.to_lowercase();
        self.headers.iter()
            .find(|(k, _)| k.to_lowercase() == lower)
            .map(|(_, v)| v.as_str())
    }
}

// Connect timeout not used with TcpStream::connect (hostname-based)
// but kept for documentation of the intended timeout behavior.
#[allow(dead_code)]
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REDIRECTS: u32 = 5;

/// Perform an HTTP request with retry logic and redirect following.
pub fn request(req: &Request, retries: u32) -> Result<Response, String> {
    let mut last_err = String::new();
    let current_url = req.url.to_string();

    for attempt in 0..=retries {
        if attempt > 0 {
            let delay = match last_err.as_str() {
                s if s.contains("429") => Duration::from_secs(2 * attempt as u64),
                _ => Duration::from_millis(500 * attempt as u64),
            };
            std::thread::sleep(delay);
        }

        // Build request with potentially redirected URL
        let actual_req = Request {
            method: req.method,
            url: &current_url,
            headers: req.headers.clone(),
            body: Body::None, // Body only sent on first attempt
        };

        let req_to_send = if attempt == 0 { req } else { &actual_req };

        let result = execute_with_redirects(req_to_send);

        match result {
            Ok(resp) if resp.status == 429 => {
                if let Some(retry_after) = resp.header("Retry-After")
                    && let Ok(secs) = retry_after.parse::<u64>()
                {
                    std::thread::sleep(Duration::from_secs(secs));
                }
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

/// Execute request, following redirects up to MAX_REDIRECTS.
fn execute_with_redirects(req: &Request) -> Result<Response, String> {
    let mut current_url = req.url.to_string();

    for _redirect in 0..MAX_REDIRECTS {
        let resp = execute_single(&Request {
            method: req.method,
            url: &current_url,
            headers: req.headers.clone(),
            body: Body::None, // Can't re-send body on redirect
        }, if _redirect == 0 { &req.body } else { &Body::None })?;

        match resp.status {
            301 | 302 | 307 | 308 => {
                if let Some(location) = resp.header("Location") {
                    // Handle relative redirects
                    if location.starts_with("http://") || location.starts_with("https://") {
                        current_url = location.to_string();
                    } else {
                        // Relative URL — combine with current
                        let base = current_url.rfind('/').map(|i| &current_url[..i]).unwrap_or(&current_url);
                        current_url = format!("{base}{location}");
                    }
                    continue;
                }
                return Ok(resp);
            }
            _ => return Ok(resp),
        }
    }

    Err("too many redirects".into())
}

/// Execute a single HTTP request (no retries, no redirects).
fn execute_single(req: &Request, body: &Body) -> Result<Response, String> {
    if req.url.starts_with("http://") {
        request_tcp(req, body)
    } else if req.url.starts_with("https://") {
        request_tls(req, body)
    } else {
        Err(format!("unsupported URL scheme: {}", req.url))
    }
}

/// Parse URL into (host, port, path) components.
fn parse_url(url: &str) -> Result<(&str, u16, String), String> {
    let without_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);

    let (host_port, path) = without_scheme.split_once('/').unwrap_or((without_scheme, ""));
    let path = format!("/{path}"); // Preserves query strings: /path?key=value

    let (host, port) = if host_port.contains(':') {
        let (h, p) = host_port.split_once(':').unwrap();
        (h, p.parse::<u16>().unwrap_or(if url.starts_with("https") { 443 } else { 80 }))
    } else {
        let default_port = if url.starts_with("https") { 443 } else { 80 };
        (host_port, default_port)
    };

    Ok((host, port, path))
}

/// Build the raw HTTP request bytes.
fn build_request_bytes(method: Method, host: &str, path: &str, headers: &[(&str, String)], body: &Body) -> Result<Vec<u8>, String> {
    let method_str = match method {
        Method::Get => "GET",
        Method::Post => "POST",
        Method::Patch => "PATCH",
        Method::Put => "PUT",
    };

    let body_bytes = match body {
        Body::None => Vec::new(),
        Body::Json(s) => s.as_bytes().to_vec(),
        Body::Bytes(b) => b.to_vec(),
        Body::File(p) => std::fs::read(p).map_err(|e| format!("read file {}: {e}", p.display()))?,
        Body::Multipart(fields) => build_multipart(fields)?,
    };

    let mut request = format!("{method_str} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");

    for (k, v) in headers {
        request.push_str(&format!("{k}: {v}\r\n"));
    }

    if matches!(body, Body::Multipart(_)) {
        request.push_str("Content-Type: multipart/form-data; boundary=----bincast\r\n");
    }

    if !body_bytes.is_empty() {
        request.push_str(&format!("Content-Length: {}\r\n", body_bytes.len()));
    }
    request.push_str("\r\n");

    let mut full = request.into_bytes();
    full.extend_from_slice(&body_bytes);
    Ok(full)
}

/// Read a full HTTP response from a stream.
fn read_response(stream: &mut impl Read) -> Result<Response, String> {
    let mut response = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => response.extend_from_slice(&buf[..n]),
            Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(ref e) if e.kind() == std::io::ErrorKind::ConnectionReset => {
                if response.is_empty() {
                    return Err("connection reset by peer".into());
                }
                break;
            }
            Err(e) => {
                if response.is_empty() {
                    return Err(format!("read: {e}"));
                }
                break;
            }
        }
    }

    let raw = String::from_utf8_lossy(&response);
    parse_http_response(&raw)
}

/// Plain HTTP via TcpStream — for tests and mock servers.
fn request_tcp(req: &Request, body: &Body) -> Result<Response, String> {
    let (host, port, path) = parse_url(req.url)?;
    let addr = format!("{host}:{port}");

    // Use connect() not connect_timeout() — connect() accepts hostnames
    // and does DNS resolution. Set timeouts after connecting.
    let mut stream = TcpStream::connect(&addr)
        .map_err(|e| format!("connect to {addr}: {e}"))?;

    stream.set_read_timeout(Some(READ_TIMEOUT)).ok();
    stream.set_write_timeout(Some(Duration::from_secs(30))).ok();

    let data = build_request_bytes(req.method, host, &path, &req.headers, body)?;
    stream.write_all(&data).map_err(|e| format!("write: {e}"))?;

    read_response(&mut stream)
}

/// HTTPS via rustls — native TLS, no external binaries.
fn request_tls(req: &Request, body: &Body) -> Result<Response, String> {
    let (host, port, path) = parse_url(req.url)?;

    // TLS config — cached via thread_local would be better but this is correct
    let root_store = rustls::RootCertStore::from_iter(
        webpki_roots::TLS_SERVER_ROOTS.iter().cloned()
    );
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|e| format!("invalid hostname '{host}': {e}"))?;

    let mut conn = rustls::ClientConnection::new(Arc::new(config), server_name)
        .map_err(|e| format!("TLS setup: {e}"))?;

    let addr = format!("{host}:{port}");
    let mut sock = TcpStream::connect(&addr)
        .map_err(|e| format!("connect to {addr}: {e}"))?;

    sock.set_read_timeout(Some(READ_TIMEOUT)).ok();
    sock.set_write_timeout(Some(Duration::from_secs(30))).ok();

    let mut tls = rustls::Stream::new(&mut conn, &mut sock);

    let data = build_request_bytes(req.method, host, &path, &req.headers, body)?;
    tls.write_all(&data).map_err(|e| format!("TLS write: {e}"))?;

    read_response(&mut tls)
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
                let filename = p.file_name().and_then(|f| f.to_str()).unwrap_or("file");
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
    let header_end = raw.find("\r\n\r\n").unwrap_or(raw.len());
    let header_section = &raw[..header_end];
    let body = if header_end + 4 <= raw.len() {
        raw[header_end + 4..].to_string()
    } else {
        String::new()
    };

    let status_line = header_section.lines().next().ok_or("empty response")?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .ok_or("no status code")?
        .parse::<u16>()
        .map_err(|_| format!("invalid status: {status_line}"))?;

    let mut headers = Vec::new();
    for line in header_section.lines().skip(1) {
        if let Some((key, value)) = line.split_once(": ") {
            headers.push((key.to_string(), value.to_string()));
        } else if let Some((key, value)) = line.split_once(':') {
            headers.push((key.to_string(), value.trim().to_string()));
        }
    }

    Ok(Response { status, body, headers })
}

// --- Convenience builders ---

impl<'a> Request<'a> {
    pub fn get(url: &'a str) -> Self {
        Request { method: Method::Get, url, headers: Vec::new(), body: Body::None }
    }

    pub fn post(url: &'a str) -> Self {
        Request { method: Method::Post, url, headers: Vec::new(), body: Body::None }
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

    fn mock(status: u16, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");
        thread::spawn(move || {
            for _ in 0..5 {
                let Ok((mut s, _)) = listener.accept() else { break };
                let mut buf = [0u8; 65536];
                let _ = s.read(&mut buf);
                let resp = format!("HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = s.write_all(resp.as_bytes());
            }
        });
        base
    }

    fn mock_with_headers(status: u16, headers: &'static str, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");
        thread::spawn(move || {
            let Ok((mut s, _)) = listener.accept() else { return };
            let mut buf = [0u8; 65536];
            let _ = s.read(&mut buf);
            let resp = format!("HTTP/1.1 {status} OK\r\n{headers}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            let _ = s.write_all(resp.as_bytes());
        });
        base
    }

    // --- Basic request tests ---

    #[test]
    fn test_get_200() {
        let base = mock(200, r#"{"ok":true}"#);
        let url = format!("{base}/test");
        let resp = request(&Request::get(&url), 0).unwrap();
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("ok"));
        assert!(resp.is_success());
    }

    #[test]
    fn test_get_404() {
        let base = mock(404, "not found");
        let url = format!("{base}/missing");
        let resp = request(&Request::get(&url), 0).unwrap();
        assert_eq!(resp.status, 404);
        assert!(!resp.is_success());
    }

    #[test]
    fn test_post_json() {
        let base = mock(201, r#"{"id":1}"#);
        let url = format!("{base}/create");
        let resp = request(
            &Request::post(&url).with_bearer("tok").with_json(r#"{"name":"test"}"#),
            0,
        ).unwrap();
        assert_eq!(resp.status, 201);
    }

    #[test]
    fn test_204_empty_body() {
        let base = mock(204, "");
        let url = format!("{base}/dispatch");
        let resp = request(&Request::post(&url), 0).unwrap();
        assert_eq!(resp.status, 204);
        assert!(resp.body.is_empty());
        assert!(resp.is_success());
    }

    // --- Retry tests ---

    #[test]
    fn test_retry_on_500() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");
        thread::spawn(move || {
            for i in 0..3 {
                let Ok((mut s, _)) = listener.accept() else { break };
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                let (st, bd) = if i < 2 { (500, "error") } else { (200, "ok") };
                let resp = format!("HTTP/1.1 {st} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bd}", bd.len());
                let _ = s.write_all(resp.as_bytes());
            }
        });
        let url = format!("{base}/retry");
        let resp = request(&Request::get(&url), 2).unwrap();
        assert_eq!(resp.status, 200);
    }

    #[test]
    fn test_no_retry_on_400() {
        let base = mock(400, "bad request");
        let url = format!("{base}/bad");
        let resp = request(&Request::get(&url), 2).unwrap();
        assert_eq!(resp.status, 400);
    }

    // --- Redirect tests ---

    #[test]
    fn test_follows_302_redirect() {
        // First server returns 302 pointing to second
        let target = mock(200, "final");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");
        let target_url = format!("{target}/final");
        thread::spawn(move || {
            let Ok((mut s, _)) = listener.accept() else { return };
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let resp = format!("HTTP/1.1 302 Found\r\nLocation: {target_url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            let _ = s.write_all(resp.as_bytes());
        });
        let url = format!("{base}/redirect");
        let resp = request(&Request::get(&url), 0).unwrap();
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("final"));
    }

    // --- Header tests ---

    #[test]
    fn test_response_headers_parsed() {
        let base = mock_with_headers(200, "X-Custom: hello\r\nX-Other: world", "body");
        let url = format!("{base}/headers");
        let resp = request(&Request::get(&url), 0).unwrap();
        assert_eq!(resp.header("X-Custom"), Some("hello"));
        assert_eq!(resp.header("x-custom"), Some("hello")); // case-insensitive
        assert_eq!(resp.header("X-Other"), Some("world"));
        assert_eq!(resp.header("X-Missing"), None);
    }

    // --- Multipart tests ---

    #[test]
    fn test_build_multipart_text_fields() {
        let fields = vec![
            FormField { name: "action", value: FormValue::Text("upload") },
            FormField { name: "version", value: FormValue::Text("1") },
        ];
        let body = build_multipart(&fields).unwrap();
        let body_str = String::from_utf8_lossy(&body);
        assert!(body_str.contains("name=\"action\""));
        assert!(body_str.contains("upload"));
        assert!(body_str.contains("name=\"version\""));
        assert!(body_str.contains("------bincast--"));
    }

    #[test]
    fn test_build_multipart_with_file() {
        let dir = std::env::temp_dir().join(format!("mp-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.whl");
        std::fs::write(&file, b"wheel data here").unwrap();

        let fields = vec![
            FormField { name: "action", value: FormValue::Text("file_upload") },
            FormField { name: "content", value: FormValue::File(&file) },
        ];
        let body = build_multipart(&fields).unwrap();
        let body_str = String::from_utf8_lossy(&body);
        assert!(body_str.contains("filename=\"test.whl\""));
        assert!(body_str.contains("wheel data here"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- File upload test ---

    #[test]
    fn test_file_upload() {
        let base = mock(201, "ok");
        let dir = std::env::temp_dir().join(format!("upload-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("binary.tar.gz");
        std::fs::write(&file, b"fake binary data 12345").unwrap();

        let url = format!("{base}/upload");
        let resp = request(&Request::post(&url).with_file(&file), 0).unwrap();
        assert_eq!(resp.status, 201);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- Connection error tests ---

    #[test]
    fn test_connection_refused() {
        let result = request(&Request::get("http://127.0.0.1:1"), 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("connect"), "error should mention connect: {err}");
    }

    #[test]
    fn test_unsupported_scheme() {
        let result = request(&Request::get("ftp://example.com"), 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unsupported"));
    }

    // --- URL parsing tests ---

    #[test]
    fn test_parse_url_with_query() {
        let (host, port, path) = parse_url("https://uploads.github.com/repos/o/r/releases/1/assets?name=foo.tar.gz").unwrap();
        assert_eq!(host, "uploads.github.com");
        assert_eq!(port, 443);
        assert_eq!(path, "/repos/o/r/releases/1/assets?name=foo.tar.gz");
    }

    #[test]
    fn test_parse_url_with_port() {
        let (host, port, path) = parse_url("http://127.0.0.1:8080/api/test").unwrap();
        assert_eq!(host, "127.0.0.1");
        assert_eq!(port, 8080);
        assert_eq!(path, "/api/test");
    }

    #[test]
    fn test_parse_url_default_ports() {
        let (_, port, _) = parse_url("https://example.com/path").unwrap();
        assert_eq!(port, 443);
        let (_, port, _) = parse_url("http://example.com/path").unwrap();
        assert_eq!(port, 80);
    }

    // --- Status classification ---

    #[test]
    fn test_response_is_success() {
        assert!(Response { status: 200, body: String::new(), headers: vec![] }.is_success());
        assert!(Response { status: 201, body: String::new(), headers: vec![] }.is_success());
        assert!(Response { status: 204, body: String::new(), headers: vec![] }.is_success());
        assert!(!Response { status: 400, body: String::new(), headers: vec![] }.is_success());
        assert!(!Response { status: 404, body: String::new(), headers: vec![] }.is_success());
        assert!(!Response { status: 500, body: String::new(), headers: vec![] }.is_success());
    }
}
