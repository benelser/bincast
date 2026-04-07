//! Cross-compilation orchestration.
//! Generates the correct cargo/maturin commands for each target.
//! Does NOT execute them — that's the pipeline's job.

use crate::config::TargetTriple;

/// A build command ready to execute.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub description: String,
}

/// Generate the cargo build command for a target.
pub fn cargo_build_command(binary: &str, target: &TargetTriple, release: bool) -> BuildCommand {
    let mut args = vec![
        "build".to_string(),
        "--target".to_string(),
        target.to_string(),
    ];

    if release {
        args.push("--release".to_string());
    }

    let mut env = Vec::new();

    // jemalloc page size for aarch64/ppc64le linux
    if (target.arch() == "aarch64" || target.as_str().contains("ppc64"))
        && target.os() == "linux"
    {
        env.push(("JEMALLOC_SYS_WITH_LG_PAGE".to_string(), "16".to_string()));
    }

    BuildCommand {
        program: "cargo".to_string(),
        args,
        env,
        description: format!("build {binary} for {target}"),
    }
}

/// Generate the maturin build command for a target (PyPI wheel output).
pub fn maturin_build_command(target: &TargetTriple, release: bool) -> BuildCommand {
    let mut args = vec![
        "build".to_string(),
        "--target".to_string(),
        target.to_string(),
        "--bindings".to_string(),
        "bin".to_string(),
    ];

    if release {
        args.push("--release".to_string());
    }

    // manylinux compatibility
    if target.os() == "linux" && !target.is_musl() {
        args.push("--compatibility".to_string());
        if target.arch() == "aarch64" {
            args.push("manylinux2_28".to_string());
        } else {
            args.push("manylinux2_17".to_string());
        }
    }

    if target.os() == "linux" && target.is_musl() {
        args.push("--compatibility".to_string());
        args.push("musllinux_1_2".to_string());
    }

    let mut env = Vec::new();
    if (target.arch() == "aarch64" || target.as_str().contains("ppc64"))
        && target.os() == "linux"
    {
        env.push(("JEMALLOC_SYS_WITH_LG_PAGE".to_string(), "16".to_string()));
    }

    BuildCommand {
        program: "maturin".to_string(),
        args,
        env,
        description: format!("build wheel for {target}"),
    }
}

/// Expected binary path after a successful cargo build.
pub fn binary_path(binary: &str, target: &TargetTriple, release: bool) -> String {
    let profile = if release { "release" } else { "debug" };
    let ext = target.binary_extension();
    format!("target/{target}/{profile}/{binary}{ext}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triple(s: &str) -> TargetTriple {
        TargetTriple::new(s).unwrap()
    }

    #[test]
    fn test_cargo_build_linux_x86() {
        let cmd = cargo_build_command("my-tool", &triple("x86_64-unknown-linux-gnu"), true);
        assert_eq!(cmd.program, "cargo");
        assert!(cmd.args.contains(&"--release".to_string()));
        assert!(cmd.args.contains(&"x86_64-unknown-linux-gnu".to_string()));
        assert!(cmd.env.is_empty());
    }

    #[test]
    fn test_cargo_build_linux_aarch64_sets_jemalloc_page() {
        let cmd = cargo_build_command("my-tool", &triple("aarch64-unknown-linux-gnu"), true);
        assert_eq!(cmd.env.len(), 1);
        assert_eq!(cmd.env[0].0, "JEMALLOC_SYS_WITH_LG_PAGE");
        assert_eq!(cmd.env[0].1, "16");
    }

    #[test]
    fn test_cargo_build_macos_no_jemalloc() {
        let cmd = cargo_build_command("my-tool", &triple("aarch64-apple-darwin"), true);
        assert!(cmd.env.is_empty());
    }

    #[test]
    fn test_cargo_build_windows() {
        let cmd = cargo_build_command("my-tool", &triple("x86_64-pc-windows-msvc"), true);
        assert!(cmd.args.contains(&"x86_64-pc-windows-msvc".to_string()));
    }

    #[test]
    fn test_maturin_linux_glibc_manylinux() {
        let cmd = maturin_build_command(&triple("x86_64-unknown-linux-gnu"), true);
        assert_eq!(cmd.program, "maturin");
        assert!(cmd.args.contains(&"bin".to_string()));
        assert!(cmd.args.contains(&"manylinux2_17".to_string()));
    }

    #[test]
    fn test_maturin_linux_aarch64_manylinux2_28() {
        let cmd = maturin_build_command(&triple("aarch64-unknown-linux-gnu"), true);
        assert!(cmd.args.contains(&"manylinux2_28".to_string()));
    }

    #[test]
    fn test_maturin_linux_musl() {
        let cmd = maturin_build_command(&triple("x86_64-unknown-linux-musl"), true);
        assert!(cmd.args.contains(&"musllinux_1_2".to_string()));
    }

    #[test]
    fn test_maturin_macos_no_manylinux() {
        let cmd = maturin_build_command(&triple("aarch64-apple-darwin"), true);
        assert!(!cmd.args.contains(&"--compatibility".to_string()));
    }

    #[test]
    fn test_binary_path_release() {
        let path = binary_path("my-tool", &triple("x86_64-unknown-linux-gnu"), true);
        assert_eq!(path, "target/x86_64-unknown-linux-gnu/release/my-tool");
    }

    #[test]
    fn test_binary_path_windows() {
        let path = binary_path("my-tool", &triple("x86_64-pc-windows-msvc"), true);
        assert_eq!(path, "target/x86_64-pc-windows-msvc/release/my-tool.exe");
    }

    #[test]
    fn test_binary_path_debug() {
        let path = binary_path("my-tool", &triple("aarch64-apple-darwin"), false);
        assert_eq!(path, "target/aarch64-apple-darwin/debug/my-tool");
    }

    #[test]
    fn test_all_targets_produce_valid_commands() {
        let targets = [
            "aarch64-apple-darwin",
            "x86_64-apple-darwin",
            "aarch64-unknown-linux-gnu",
            "x86_64-unknown-linux-gnu",
            "x86_64-unknown-linux-musl",
            "x86_64-pc-windows-msvc",
        ];
        for t in targets {
            let target = triple(t);
            let cmd = cargo_build_command("test", &target, true);
            assert_eq!(cmd.program, "cargo");
            assert!(cmd.args.contains(&t.to_string()));

            let mcmd = maturin_build_command(&target, true);
            assert_eq!(mcmd.program, "maturin");
            assert!(mcmd.args.contains(&t.to_string()));
        }
    }
}
