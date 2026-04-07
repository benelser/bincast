//! Unified HTTP client — zero dependencies.
//!
//! Uses std::net::TcpStream for plain HTTP (tests, mock servers).
//! Uses system curl for HTTPS (production) — bundled on macOS, Linux, Windows 10+.
//!
//! Single API for both: `HttpClient::request()`.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::Command;
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
            // HTTPS — use system curl
            request_curl(req)
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

/// HTTPS via system curl.
fn request_curl(req: &Request) -> Result<Response, String> {
    let mut args: Vec<String> = vec![
        "-s".into(),
        "-w".into(),
        "\n__BINCAST_STATUS__%{http_code}".into(),
        "--max-time".into(),
        "30".into(),
    ];

    match req.method {
        Method::Get => {}
        Method::Post => { args.push("-X".into()); args.push("POST".into()); }
        Method::Patch => { args.push("-X".into()); args.push("PATCH".into()); }
        Method::Put => { args.push("-X".into()); args.push("PUT".into()); }
    }

    for (k, v) in &req.headers {
        args.push("-H".into());
        args.push(format!("{k}: {v}"));
    }

    match &req.body {
        Body::None => {}
        Body::Json(s) => {
            args.push("-d".into());
            args.push(s.to_string());
        }
        Body::Bytes(b) => {
            args.push("--data-binary".into());
            args.push(String::from_utf8_lossy(b).into_owned());
        }
        Body::File(p) => {
            args.push("-T".into());
            args.push(p.to_str().ok_or("non-utf8 path")?.into());
        }
        Body::Multipart(fields) => {
            for field in fields {
                match &field.value {
                    FormValue::Text(t) => {
                        args.push("-F".into());
                        args.push(format!("{}={}", field.name, t));
                    }
                    FormValue::File(p) => {
                        args.push("-F".into());
                        args.push(format!("{}=@{}", field.name, p.display()));
                    }
                }
            }
        }
    }

    args.push(req.url.into());

    let output = Command::new("curl")
        .args(&args)
        .output()
        .map_err(|e| format!("failed to run curl: {e}"))?;

    let raw = String::from_utf8_lossy(&output.stdout);

    // Parse status from our marker
    if let Some(marker_pos) = raw.rfind("__BINCAST_STATUS__") {
        let body = &raw[..marker_pos];
        let status_str = &raw[marker_pos + "__BINCAST_STATUS__".len()..];
        let status = status_str.trim().parse::<u16>().unwrap_or(0);
        Ok(Response {
            status,
            body: body.to_string(),
        })
    } else {
        Err(format!("curl returned unexpected output: {raw}"))
    }
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
