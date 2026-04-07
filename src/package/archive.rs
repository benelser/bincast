//! Archive creation — tar.gz and zip.
//! Uses std::process to shell out to tar/zip commands.
//! This avoids pulling in flate2/tar/zip crates while keeping archives correct.

use std::path::Path;
use std::process::Command;

/// Create a .tar.gz archive containing a single binary.
pub fn create_tar_gz(binary_path: &Path, archive_path: &Path) -> Result<(), String> {
    let binary_name = binary_path
        .file_name()
        .ok_or("invalid binary path")?
        .to_str()
        .ok_or("non-utf8 binary name")?;

    let parent = binary_path
        .parent()
        .ok_or("binary has no parent directory")?;

    let output = Command::new("tar")
        .args(["czf", archive_path.to_str().unwrap(), binary_name])
        .current_dir(parent)
        .output()
        .map_err(|e| format!("failed to run tar: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("tar failed: {stderr}"));
    }

    Ok(())
}

/// Create a .zip archive containing a single binary.
pub fn create_zip(binary_path: &Path, archive_path: &Path) -> Result<(), String> {
    let binary_name = binary_path
        .file_name()
        .ok_or("invalid binary path")?
        .to_str()
        .ok_or("non-utf8 binary name")?;

    let parent = binary_path
        .parent()
        .ok_or("binary has no parent directory")?;

    let output = Command::new("zip")
        .args(["-j", archive_path.to_str().unwrap(), binary_name])
        .current_dir(parent)
        .output()
        .map_err(|e| format!("failed to run zip: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("zip failed: {stderr}"));
    }

    Ok(())
}

/// Create the appropriate archive for a target (zip for Windows, tar.gz for everything else).
pub fn create_archive(
    binary_path: &Path,
    output_dir: &Path,
    name: &str,
    target: &str,
    is_windows: bool,
) -> Result<std::path::PathBuf, String> {
    let ext = if is_windows { "zip" } else { "tar.gz" };
    let archive_path = output_dir.join(format!("{name}-{target}.{ext}"));

    if is_windows {
        create_zip(binary_path, &archive_path)?;
    } else {
        create_tar_gz(binary_path, &archive_path)?;
    }

    Ok(archive_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> std::path::PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("releaser-archive-{ts}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_create_tar_gz_and_verify() {
        let dir = temp_dir();
        let binary = dir.join("my-tool");
        fs::write(&binary, b"#!/bin/sh\necho hello").unwrap();

        let archive = dir.join("my-tool.tar.gz");
        create_tar_gz(&binary, &archive).unwrap();

        assert!(archive.exists());
        assert!(fs::metadata(&archive).unwrap().len() > 0);

        // Verify contents by listing
        let output = Command::new("tar")
            .args(["tzf", archive.to_str().unwrap()])
            .output()
            .unwrap();
        let listing = String::from_utf8_lossy(&output.stdout);
        assert!(listing.contains("my-tool"), "archive should contain my-tool, got: {listing}");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_create_zip_and_verify() {
        let dir = temp_dir();
        let binary = dir.join("my-tool.exe");
        fs::write(&binary, b"MZ fake exe").unwrap();

        let archive = dir.join("my-tool.zip");
        create_zip(&binary, &archive).unwrap();

        assert!(archive.exists());
        assert!(fs::metadata(&archive).unwrap().len() > 0);

        // Verify contents by listing
        let output = Command::new("unzip")
            .args(["-l", archive.to_str().unwrap()])
            .output()
            .unwrap();
        let listing = String::from_utf8_lossy(&output.stdout);
        assert!(listing.contains("my-tool.exe"), "archive should contain my-tool.exe");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_create_archive_selects_format() {
        let dir = temp_dir();
        let binary = dir.join("my-tool");
        fs::write(&binary, b"binary content").unwrap();

        let result = create_archive(&binary, &dir, "my-tool", "x86_64-unknown-linux-gnu", false).unwrap();
        assert!(result.to_str().unwrap().ends_with(".tar.gz"));

        let binary_exe = dir.join("my-tool.exe");
        fs::write(&binary_exe, b"binary content").unwrap();
        let result_win = create_archive(&binary_exe, &dir, "my-tool", "x86_64-pc-windows-msvc", true).unwrap();
        assert!(result_win.to_str().unwrap().ends_with(".zip"));

        let _ = fs::remove_dir_all(&dir);
    }
}
