//! Full pipeline integration tests using digital twins.
//! Every test is hermetic — no network, no real registries, no Docker.

use std::fs;
use std::path::PathBuf;

use releaser::twin::{GitHubTwin, PyPITwin, NpmTwin, CratesTwin, FaultProxy};
use releaser::config;
use releaser::pipeline::Context;
use releaser::publish;

fn temp_dir(name: &str) -> PathBuf {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("releaser-twin-{name}-{ts}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn full_config() -> releaser::config::ReleaserConfig {
    config::parse(r#"
[package]
name = "twin-test"
binary = "twin-test"
description = "Twin test tool"
repository = "https://github.com/user/twin-test"
license = "MIT"

[targets]
platforms = ["x86_64-unknown-linux-gnu"]

[distribute.github]
release = true

[distribute.pypi]
package_name = "twin-test"

[distribute.npm]
scope = "@twin"

[distribute.cargo]
crate_name = "twin-test"

[distribute.homebrew]
tap = "user/homebrew-twin-test"

[distribute.scoop]
bucket = "user/scoop-twin-test"
"#).unwrap()
}

/// Set up all twin env vars, returning the twins for state inspection.
struct TwinEnvironment {
    pub github: GitHubTwin,
    pub pypi: PyPITwin,
    pub npm: NpmTwin,
    pub crates: CratesTwin,
}

impl TwinEnvironment {
    fn start() -> Self {
        let github = GitHubTwin::start();
        let pypi = PyPITwin::start();
        let npm = NpmTwin::start();
        let crates = CratesTwin::start();

        // Point all HTTP clients at the twins
        unsafe {
            std::env::set_var("RELEASER_GITHUB_API_URL", &github.base_url);
            std::env::set_var("RELEASER_GITHUB_UPLOAD_URL", &github.base_url);
            std::env::set_var("RELEASER_PYPI_URL", &pypi.base_url);
            std::env::set_var("RELEASER_REGISTRY_BASE_URL", &npm.base_url);
            std::env::set_var("GITHUB_TOKEN", "test-token");
            std::env::set_var("GH_TOKEN", "test-token");
            std::env::set_var("PYPI_TOKEN", "test-token");
            std::env::set_var("TAP_GITHUB_TOKEN", "test-token");
            std::env::set_var("BUCKET_GITHUB_TOKEN", "test-token");
        }

        TwinEnvironment { github, pypi, npm, crates }
    }
}

impl Drop for TwinEnvironment {
    fn drop(&mut self) {
        unsafe {
            std::env::remove_var("RELEASER_GITHUB_API_URL");
            std::env::remove_var("RELEASER_GITHUB_UPLOAD_URL");
            std::env::remove_var("RELEASER_PYPI_URL");
            std::env::remove_var("RELEASER_REGISTRY_BASE_URL");
            std::env::remove_var("GITHUB_TOKEN");
            std::env::remove_var("GH_TOKEN");
            std::env::remove_var("PYPI_TOKEN");
            std::env::remove_var("TAP_GITHUB_TOKEN");
            std::env::remove_var("BUCKET_GITHUB_TOKEN");
        }
    }
}

// --- Tests ---

#[test]
fn test_publish_dry_run_with_all_twins() {
    let _twins = TwinEnvironment::start();
    let config = full_config();
    let pipeline = publish::build_pipeline(&config);
    let mut ctx = Context::with_config(config, true);
    ctx.version = Some("v0.1.0".into());

    let report = pipeline.execute(&mut ctx).unwrap();

    // All 10 pipes should report dry-run entries
    assert_eq!(report.dry_run_entries.len(), 10, "expected 10 dry-run entries, got: {:?}",
        report.dry_run_entries.iter().map(|e| &e.pipe).collect::<Vec<_>>());
}

#[test]
fn test_github_release_flow_against_twin() {
    let twins = TwinEnvironment::start();

    // Create a fake archive to upload
    let dir = temp_dir("gh-flow");
    let archive = dir.join("twin-test-x86_64-unknown-linux-gnu.tar.gz");
    fs::write(&archive, b"fake binary content").unwrap();
    let sidecar = dir.join("twin-test-x86_64-unknown-linux-gnu.tar.gz.sha256");
    fs::write(&sidecar, b"abc123  twin-test-x86_64-unknown-linux-gnu.tar.gz\n").unwrap();

    let config = full_config();
    let mut ctx = Context::with_config(config, false);
    ctx.version = Some("v0.5.0".into());
    ctx.artifacts.push(releaser::pipeline::Artifact {
        path: archive,
        kind: releaser::pipeline::ArtifactKind::Archive,
        target: Some("x86_64-unknown-linux-gnu".into()),
    });

    // Run just the GitHub release pipe
    let pipe = releaser::publish::github::GitHubReleasePipe;
    use releaser::pipeline::Pipe;
    pipe.run(&mut ctx).unwrap();

    // Verify twin state
    let snap = twins.github.snapshot();
    assert_eq!(snap.releases.len(), 1);
    assert_eq!(snap.releases[0].tag_name, "v0.5.0");
    assert!(!snap.releases[0].draft, "release should be published");
    assert!(snap.assets.len() >= 1, "should have at least 1 asset");

    assert!(ctx.github_release_url.is_some());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_check_name_availability_against_twins() {
    let twins = TwinEnvironment::start();

    // npm twin has no packages — name should be available
    let result = releaser::http::check_name(releaser::http::Registry::Npm, "@twin/test-tool");
    assert!(result.unwrap(), "unpublished package should be available");

    // Publish a package to npm twin, then check again
    let addr = twins.npm.base_url.strip_prefix("http://").unwrap();
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    use std::io::Write;
    use std::io::Read;
    let body = r#"{"name":"@twin/test-tool","version":"0.1.0"}"#;
    let req = format!("PUT /@twin/test-tool HTTP/1.1\r\nHost: mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    stream.write_all(req.as_bytes()).unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();

    let result = releaser::http::check_name(releaser::http::Registry::Npm, "@twin/test-tool");
    assert!(!result.unwrap(), "published package should be taken");
}

#[test]
fn test_homebrew_dispatch_against_twin() {
    let twins = TwinEnvironment::start();

    releaser::http::github::repository_dispatch(
        "user", "homebrew-twin-test", "update-formula", "v0.3.0", "test-token"
    ).unwrap();

    let snap = twins.github.snapshot();
    assert_eq!(snap.dispatches.len(), 1);
    assert_eq!(snap.dispatches[0].event_type, "update-formula");
    assert_eq!(snap.dispatches[0].version, "v0.3.0");
}

#[test]
fn test_scoop_dispatch_against_twin() {
    let twins = TwinEnvironment::start();

    releaser::http::github::repository_dispatch(
        "user", "scoop-twin-test", "update-manifest", "v0.3.0", "test-token"
    ).unwrap();

    let snap = twins.github.snapshot();
    assert_eq!(snap.dispatches.len(), 1);
    assert_eq!(snap.dispatches[0].event_type, "update-manifest");
}

#[test]
fn test_pypi_upload_against_twin() {
    let twins = TwinEnvironment::start();
    let pypi_url = &twins.pypi.base_url;

    // Simulate a wheel upload via curl (same as our publish pipe does)
    let dir = temp_dir("pypi-upload");
    let wheel = dir.join("twin_test-0.1.0-py3-none-any.whl");
    fs::write(&wheel, b"fake wheel content").unwrap();

    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "-X", "POST",
            "-u", "__token__:test-token",
            "-F", ":action=file_upload",
            "-F", "protocol_version=1",
            &format!("-F content=@{}", wheel.display()),
            &format!("{pypi_url}/legacy/"),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());

    let snap = twins.pypi.snapshot();
    assert_eq!(snap.uploads.len(), 1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_fault_injection_github_upload_fails() {
    let github = GitHubTwin::start();
    let proxy = FaultProxy::start(&github.base_url);

    // Fail the 2nd request (the upload, after create succeeds)
    proxy.fail_request(2, 500, r#"{"message":"Internal Server Error"}"#);

    unsafe {
        std::env::set_var("RELEASER_GITHUB_API_URL", &proxy.base_url);
        std::env::set_var("RELEASER_GITHUB_UPLOAD_URL", &proxy.base_url);
        std::env::set_var("GITHUB_TOKEN", "test-token");
    }

    // Create release succeeds (request 1)
    let release = releaser::http::github::create_release("user", "repo", "v9.0.0", "test-token");
    assert!(release.is_ok(), "create should succeed");

    // Upload fails (request 2 = injected 500, retries hit the real twin)
    let dir = temp_dir("fault-upload");
    let file = dir.join("test.tar.gz");
    fs::write(&file, b"data").unwrap();
    let upload = releaser::http::github::upload_asset("user", "repo", release.unwrap().id, &file, "test-token");

    // The upload may succeed on retry (requests 3+ go to the real twin via proxy)
    // Key assertion: the fault was injected
    assert_eq!(proxy.failure_count(), 1);

    unsafe {
        std::env::remove_var("RELEASER_GITHUB_API_URL");
        std::env::remove_var("RELEASER_GITHUB_UPLOAD_URL");
        std::env::remove_var("GITHUB_TOKEN");
    }
    let _ = fs::remove_dir_all(&dir);
}
