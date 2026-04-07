//! npm platform package scaffolding — the esbuild pattern.
//! Generates package.json for platform packages and the root meta-package.

use crate::config::{NpmConfig, TargetTriple};

/// Generate package.json for a platform-specific npm package.
/// e.g., @scope/binary-darwin-arm64
pub fn platform_package_json(
    scope: &str,
    binary_name: &str,
    target: &TargetTriple,
    version: &str,
) -> String {
    let os = target.npm_os();
    let cpu = target.npm_cpu();
    let pkg_name = format!("{scope}/{binary_name}-{os}-{cpu}");

    format!(
        r#"{{
  "name": "{pkg_name}",
  "version": "{version}",
  "description": "Platform-specific binary for {binary_name} ({target})",
  "os": ["{os}"],
  "cpu": ["{cpu}"],
  "bin": {{
    "{binary_name}": "bin/{binary_name}{ext}"
  }},
  "preferUnplugged": true
}}"#,
        ext = target.binary_extension()
    )
}

/// Generate package.json for the root meta-package with optional dependencies.
pub fn root_package_json(
    config: &NpmConfig,
    binary_name: &str,
    targets: &[TargetTriple],
    version: &str,
) -> String {
    let scope = &config.scope;
    let pkg_name = config
        .package_name
        .as_deref()
        .unwrap_or(binary_name);

    let optional_deps: Vec<String> = targets
        .iter()
        .map(|t| {
            let os = t.npm_os();
            let cpu = t.npm_cpu();
            format!("    \"{scope}/{binary_name}-{os}-{cpu}\": \"{version}\"")
        })
        .collect();

    format!(
        r#"{{
  "name": "{scope}/{pkg_name}",
  "version": "{version}",
  "description": "Platform-specific binary distribution",
  "bin": {{
    "{binary_name}": "bin/{binary_name}.js"
  }},
  "optionalDependencies": {{
{deps}
  }}
}}"#,
        deps = optional_deps.join(",\n")
    )
}

/// Generate the launcher script (bin/binary.js) that finds and spawns the platform binary.
pub fn launcher_js(binary_name: &str, scope: &str) -> String {
    format!(
        r#"#!/usr/bin/env node
"use strict";

const {{ execFileSync }} = require("child_process");
const path = require("path");
const os = require("os");

const PLATFORMS = {{
  "darwin-arm64": "{scope}/{binary_name}-darwin-arm64",
  "darwin-x64": "{scope}/{binary_name}-darwin-x64",
  "linux-arm64": "{scope}/{binary_name}-linux-arm64",
  "linux-x64": "{scope}/{binary_name}-linux-x64",
  "win32-x64": "{scope}/{binary_name}-win32-x64",
}};

const key = `${{os.platform()}}-${{os.arch()}}`;
const pkg = PLATFORMS[key];
if (!pkg) {{
  console.error(`Unsupported platform: ${{key}}`);
  console.error(`Supported: ${{Object.keys(PLATFORMS).join(", ")}}`);
  process.exit(1);
}}

let binPath;
try {{
  binPath = require.resolve(`${{pkg}}/bin/{binary_name}${{os.platform() === "win32" ? ".exe" : ""}}`);
}} catch {{
  console.error(`Package ${{pkg}} is not installed.`);
  console.error("This usually means the optional dependency was not installed for your platform.");
  process.exit(1);
}}

const result = execFileSync(binPath, process.argv.slice(2), {{ stdio: "inherit" }});
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triple(s: &str) -> TargetTriple {
        TargetTriple::new(s).unwrap()
    }

    #[test]
    fn test_platform_package_json_darwin_arm64() {
        let json = platform_package_json(
            "@my-org",
            "my-tool",
            &triple("aarch64-apple-darwin"),
            "0.1.0",
        );
        assert!(json.contains("\"@my-org/my-tool-darwin-arm64\""));
        assert!(json.contains("\"os\": [\"darwin\"]"));
        assert!(json.contains("\"cpu\": [\"arm64\"]"));
        assert!(json.contains("\"version\": \"0.1.0\""));
        // No .exe extension for darwin
        assert!(json.contains("\"bin/my-tool\""));
    }

    #[test]
    fn test_platform_package_json_windows() {
        let json = platform_package_json(
            "@my-org",
            "my-tool",
            &triple("x86_64-pc-windows-msvc"),
            "0.1.0",
        );
        assert!(json.contains("\"@my-org/my-tool-win32-x64\""));
        assert!(json.contains("\"os\": [\"win32\"]"));
        assert!(json.contains("\"cpu\": [\"x64\"]"));
        assert!(json.contains("bin/my-tool.exe"));
    }

    #[test]
    fn test_root_package_json() {
        let config = NpmConfig {
            scope: "@my-org".into(),
            package_name: Some("cli".into()),
        };
        let targets = vec![
            triple("aarch64-apple-darwin"),
            triple("x86_64-unknown-linux-gnu"),
        ];
        let json = root_package_json(&config, "my-tool", &targets, "0.1.0");

        assert!(json.contains("\"@my-org/cli\""));
        assert!(json.contains("\"@my-org/my-tool-darwin-arm64\": \"0.1.0\""));
        assert!(json.contains("\"@my-org/my-tool-linux-x64\": \"0.1.0\""));
        assert!(json.contains("optionalDependencies"));
    }

    #[test]
    fn test_launcher_js() {
        let js = launcher_js("my-tool", "@my-org");
        assert!(js.contains("#!/usr/bin/env node"));
        assert!(js.contains("@my-org/my-tool-darwin-arm64"));
        assert!(js.contains("@my-org/my-tool-linux-x64"));
        assert!(js.contains("@my-org/my-tool-win32-x64"));
        assert!(js.contains("execFileSync"));
        assert!(js.contains("os.platform()"));
    }
}
