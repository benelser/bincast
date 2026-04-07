use crate::template::{Context, Template};
use super::GenerateContext;

pub fn render(gctx: &GenerateContext) -> Result<String, String> {
    let template = Template::new(SCOOP_TEMPLATE);
    let mut ctx = Context::new();

    ctx.set("name", gctx.config.package.name.as_str());
    ctx.set("binary", gctx.config.package.binary.as_str());
    ctx.set("owner", gctx.owner.as_str());
    ctx.set("repo", gctx.repo.as_str());

    let desc = gctx.config.package.description.as_deref().unwrap_or("");
    ctx.set("description", desc);

    let license = gctx.config.package.license.as_deref().unwrap_or("MIT");
    ctx.set("license", license);

    let homepage = gctx.config.package.homepage.as_deref()
        .unwrap_or(&gctx.config.package.repository);
    ctx.set("homepage", homepage);

    let has_win_x86 = gctx.config.targets.platforms.iter().any(|t| t.as_str() == "x86_64-pc-windows-msvc");
    let has_win_arm = gctx.config.targets.platforms.iter().any(|t| t.as_str() == "aarch64-pc-windows-msvc");
    ctx.set("has_win_x86", has_win_x86);
    ctx.set("has_win_arm", has_win_arm);

    template.render(&ctx)
}

const SCOOP_TEMPLATE: &str = r#"{
    "version": "0.0.0",
    "description": "{{ description }}",
    "homepage": "{{ homepage }}",
    "license": "{{ license }}",
    "architecture": {
{% if has_win_x86 %}        "64bit": {
            "url": "https://github.com/{{ owner }}/{{ repo }}/releases/download/v$version/{{ name }}-x86_64-pc-windows-msvc.zip",
            "hash": "PLACEHOLDER"
        }{% if has_win_arm %},{% endif %}

{% endif %}{% if has_win_arm %}        "arm64": {
            "url": "https://github.com/{{ owner }}/{{ repo }}/releases/download/v$version/{{ name }}-aarch64-pc-windows-msvc.zip",
            "hash": "PLACEHOLDER"
        }
{% endif %}    },
    "bin": "{{ binary }}.exe",
    "checkver": "github",
    "autoupdate": {
        "architecture": {
{% if has_win_x86 %}            "64bit": {
                "url": "https://github.com/{{ owner }}/{{ repo }}/releases/download/v$version/{{ name }}-x86_64-pc-windows-msvc.zip"
            }{% if has_win_arm %},{% endif %}

{% endif %}{% if has_win_arm %}            "arm64": {
                "url": "https://github.com/{{ owner }}/{{ repo }}/releases/download/v$version/{{ name }}-aarch64-pc-windows-msvc.zip"
            }
{% endif %}        }
    }
}
"#;
