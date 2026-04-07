//! PyPI upload API digital twin.
//! Models the legacy upload endpoint used by twine/maturin.
//! Tracks uploaded packages, rejects duplicates.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

pub struct PyPITwin {
    pub base_url: String,
    pub state: Arc<Mutex<PyPIState>>,
    _handle: thread::JoinHandle<()>,
}

#[derive(Debug, Default)]
pub struct PyPIState {
    pub uploads: Vec<PackageUpload>,
}

#[derive(Debug, Clone)]
pub struct PackageUpload {
    pub filename: String,
    pub size: usize,
    pub sha256: Option<String>,
}

impl PyPITwin {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{addr}");
        let state = Arc::new(Mutex::new(PyPIState::default()));
        let state_clone = state.clone();

        let handle = thread::spawn(move || {
            loop {
                let Ok((mut stream, _)) = listener.accept() else { break };
                let mut buf = vec![0u8; 1 << 20]; // 1MB buffer for wheel uploads
                let n = match stream.read(&mut buf) {
                    Ok(0) | Err(_) => continue,
                    Ok(n) => n,
                };
                let raw = &buf[..n];
                let request = String::from_utf8_lossy(raw);

                let response = if request.starts_with("POST") {
                    handle_upload(&state_clone, &request, raw)
                } else {
                    http_response(404, "Not Found")
                };

                let _ = stream.write_all(response.as_bytes());
            }
        });

        PyPITwin {
            base_url,
            state,
            _handle: handle,
        }
    }

    pub fn snapshot(&self) -> PyPIState {
        let s = self.state.lock().unwrap();
        PyPIState {
            uploads: s.uploads.clone(),
        }
    }
}

fn handle_upload(state: &Arc<Mutex<PyPIState>>, request: &str, raw: &[u8]) -> String {
    // Extract filename from multipart Content-Disposition
    let filename = extract_multipart_field(request, "filename")
        .or_else(|| extract_content_filename(request))
        .unwrap_or_else(|| "unknown.whl".to_string());

    let sha256 = extract_form_field(request, "sha256_digest");

    let mut s = state.lock().unwrap();

    // Reject duplicate filenames (PyPI behavior)
    if s.uploads.iter().any(|u| u.filename == filename) {
        return http_response(
            400,
            &format!("File already exists: {filename}. See https://pypi.org/help/#file-name-reuse"),
        );
    }

    // Estimate body size
    let body_start = find_body_start(raw).unwrap_or(raw.len());
    let body_size = raw.len() - body_start;

    s.uploads.push(PackageUpload {
        filename,
        size: body_size,
        sha256,
    });

    http_response(200, "OK")
}

fn extract_multipart_field(request: &str, field: &str) -> Option<String> {
    let pattern = format!("{field}=\"");
    let pos = request.find(&pattern)?;
    let start = pos + pattern.len();
    let end = request[start..].find('"')?;
    Some(request[start..start + end].to_string())
}

fn extract_content_filename(request: &str) -> Option<String> {
    // Look for content=@filename in -F form data, or filename in content-disposition
    let pattern = "filename=\"";
    let pos = request.find(pattern)?;
    let start = pos + pattern.len();
    let end = request[start..].find('"')?;
    Some(request[start..start + end].to_string())
}

fn extract_form_field(request: &str, field: &str) -> Option<String> {
    // Look for field value in multipart form data
    let pattern = format!("name=\"{field}\"");
    let pos = request.find(&pattern)?;
    let after = &request[pos + pattern.len()..];
    // Skip past the \r\n\r\n boundary to the value
    let value_start = after.find("\r\n\r\n").map(|i| i + 4)?;
    let value_end = after[value_start..].find("\r\n")?;
    Some(after[value_start..value_start + value_end].to_string())
}

fn find_body_start(raw: &[u8]) -> Option<usize> {
    for i in 0..raw.len().saturating_sub(3) {
        if &raw[i..i + 4] == b"\r\n\r\n" {
            return Some(i + 4);
        }
    }
    None
}

fn http_response(status: u16, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Unknown",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pypi_twin_accepts_upload() {
        let twin = PyPITwin::start();

        // Simulate a curl upload
        let mut stream = std::net::TcpStream::connect(
            twin.base_url.strip_prefix("http://").unwrap()
        ).unwrap();

        let body = "------boundary\r\nContent-Disposition: form-data; name=\":action\"\r\n\r\nfile_upload\r\n------boundary\r\nContent-Disposition: form-data; name=\"content\"; filename=\"my_tool-0.1.0-py3-none-any.whl\"\r\n\r\nfake wheel data\r\n------boundary--\r\n";
        let request = format!(
            "POST /legacy/ HTTP/1.1\r\nHost: mock\r\nContent-Length: {}\r\nContent-Type: multipart/form-data; boundary=----boundary\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.contains("200"), "expected 200, got: {response}");

        let snap = twin.snapshot();
        assert_eq!(snap.uploads.len(), 1);
        assert!(snap.uploads[0].filename.contains(".whl"));
    }

    #[test]
    fn test_pypi_twin_rejects_duplicate() {
        let twin = PyPITwin::start();
        let addr = twin.base_url.strip_prefix("http://").unwrap();

        for i in 0..2 {
            let mut stream = std::net::TcpStream::connect(addr).unwrap();
            let body = "------b\r\nContent-Disposition: form-data; name=\"content\"; filename=\"dupe-0.1.0.whl\"\r\n\r\ndata\r\n------b--\r\n";
            let request = format!(
                "POST /legacy/ HTTP/1.1\r\nHost: mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(request.as_bytes()).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();

            if i == 0 {
                assert!(response.contains("200"));
            } else {
                assert!(response.contains("400"), "duplicate should fail: {response}");
                assert!(response.contains("already exists"));
            }
        }
    }
}
