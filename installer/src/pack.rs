//! Everything the installer ships is compiled into the binary from the pack folder
//! (`../plugins/pa`, `../templates`) and the installer's own `assets/` directory.

use include_dir::{include_dir, Dir};
use serde::Serialize;

pub static SKILLS: Dir = include_dir!("$CARGO_MANIFEST_DIR/../plugins/pa/skills");
pub static AGENTS: Dir = include_dir!("$CARGO_MANIFEST_DIR/../plugins/pa/agents");
pub static ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets");
pub static MCP_JSON: &str = include_str!("../../plugins/pa/.mcp.json");
pub static MEMORY_MD: &str = include_str!("../../templates/state/memory.md");
pub static INDEX_HTML: &str = include_str!("../ui/index.html");
/// The pa-tray binary for this platform, embedded by build.rs (empty when it was not built first).
pub static TRAY_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/pa-tray.bin"));
pub static TRAY_NAME: &str = include_str!(concat!(env!("OUT_DIR"), "/pa-tray.name"));
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Clone, Debug)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub required: bool,
}

/// Parse `name:` / `description:` out of a SKILL.md frontmatter block.
fn frontmatter(src: &str) -> (String, String) {
    let mut name = String::new();
    let mut desc = String::new();
    let mut inside = false;
    for line in src.lines() {
        let t = line.trim_end();
        if t == "---" {
            if inside { break; }
            inside = true;
            continue;
        }
        if !inside { continue; }
        if let Some(v) = t.strip_prefix("name:") { name = v.trim().to_string(); }
        if let Some(v) = t.strip_prefix("description:") { desc = v.trim().to_string(); }
    }
    (name, desc)
}

pub fn skills() -> Vec<SkillInfo> {
    let mut out: Vec<SkillInfo> = SKILLS
        .dirs()
        .filter_map(|d| {
            let f = d.get_file(d.path().join("SKILL.md"))?;
            let src = f.contents_utf8()?;
            let (name, description) = frontmatter(src);
            let name = if name.is_empty() { d.path().file_name()?.to_string_lossy().to_string() } else { name };
            Some(SkillInfo { required: name == "pa", name, description })
        })
        .collect();
    // Charter first, then alphabetical.
    out.sort_by(|a, b| b.required.cmp(&a.required).then(a.name.cmp(&b.name)));
    out
}

pub fn skill_source(name: &str) -> Option<&'static str> {
    SKILLS.get_file(format!("{name}/SKILL.md"))?.contents_utf8()
}

/// (file name, contents) for every agent definition.
pub fn agents() -> Vec<(String, &'static str)> {
    let mut v: Vec<(String, &'static str)> = AGENTS
        .files()
        .filter_map(|f| Some((f.path().file_name()?.to_string_lossy().to_string(), f.contents_utf8()?)))
        .collect();
    v.sort();
    v
}

/// Files directly under an assets subfolder, e.g. `bin` or `hooks`.
pub fn asset_files(sub: &str) -> Vec<(String, &'static str)> {
    let mut v: Vec<(String, &'static str)> = ASSETS
        .get_dir(sub)
        .map(|d| {
            d.files()
                .filter_map(|f| Some((f.path().file_name()?.to_string_lossy().to_string(), f.contents_utf8()?)))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

pub fn asset(path: &str) -> &'static str {
    ASSETS.get_file(path).and_then(|f| f.contents_utf8()).unwrap_or_default()
}
