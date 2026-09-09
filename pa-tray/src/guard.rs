//! `pa-tray guard`: a Claude Code PreToolUse hook that fences file and shell tools to the allowed folders.
//! Reads the hook JSON on stdin, exits 2 (block, reason on stderr) when a target path is inside a protected
//! folder or outside every allowed folder; exits 0 otherwise. Works in every permission mode and on every OS.
//!
//! Allowed: the project folder, `.pa/allowed-folders.txt` (one per line; `C:\` or `/` allows everything),
//! `permissions.additionalDirectories` from `.claude/settings.json`, and the system temp dir.
//! Protected: `.pa/protected-folders.txt` plus the built-in list (ssh/aws/gnupg keys, Claude's own config).

use serde_json::Value;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const BUILTIN_PROTECTED: &[&str] = &["~/.ssh", "~/.aws", "~/.gnupg", "~/.claude.json", "~/.claude", "~/.config/gcloud", "~/.kube", "~/.docker"];

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Absolute, `..`-free, native separators; `~` expanded; MSYS `/c/...` turned into `C:\...` on Windows.
fn resolve(raw: &str, cwd: &Path) -> PathBuf {
    let mut s = raw.trim().trim_matches('"').trim_matches('\'').to_string();
    if s == "~" || s.starts_with("~/") || s.starts_with("~\\") {
        s = format!("{}{}", home_dir().display(), &s[1..]);
    }
    if cfg!(windows) {
        let b = s.as_bytes();
        if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && (b[2] == b'/' || b.len() == 2) {
            s = format!("{}:{}", (b[1] as char).to_ascii_uppercase(), if b.len() > 2 { &s[2..] } else { "\\" });
        }
    }
    let p = PathBuf::from(&s);
    let abs = if p.is_absolute() { p } else { cwd.join(p) };
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn key(p: &Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\");
    let s = s.trim_end_matches('\\').to_string();
    if cfg!(windows) { s.to_lowercase() } else { s.replace('\\', "/") }
}

fn under(p: &Path, root: &Path) -> bool {
    let a = key(p);
    let r = key(root);
    if r.is_empty() || r == "\\" || r == "/" {
        return true;
    }
    // "c:" (drive root) allows the whole drive
    let r_drive_root = cfg!(windows) && r.len() == 2 && r.ends_with(':');
    if r_drive_root {
        return a.starts_with(&r);
    }
    a == r || a.starts_with(&format!("{r}{}", if cfg!(windows) { '\\' } else { '/' }))
}

fn read_list(p: &Path) -> Vec<String> {
    std::fs::read_to_string(p)
        .map(|t| t.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty() && !l.starts_with('#')).collect())
        .unwrap_or_default()
}

fn allowed_roots(root: &Path) -> Vec<PathBuf> {
    let mut v = vec![root.to_path_buf(), std::env::temp_dir()];
    for l in read_list(&root.join(".pa").join("allowed-folders.txt")) {
        v.push(resolve(&l, root));
    }
    if let Ok(t) = std::fs::read_to_string(root.join(".claude").join("settings.json")) {
        if let Ok(s) = serde_json::from_str::<Value>(&t) {
            if let Some(arr) = s.get("permissions").and_then(|p| p.get("additionalDirectories")).and_then(|a| a.as_array()) {
                for d in arr.iter().filter_map(|x| x.as_str()) {
                    v.push(resolve(d, root));
                }
            }
        }
    }
    v
}

fn protected_roots(root: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = BUILTIN_PROTECTED.iter().map(|s| resolve(s, root)).collect();
    for l in read_list(&root.join(".pa").join("protected-folders.txt")) {
        v.push(resolve(&l, root));
    }
    v
}

/// Literal paths in a shell command: `C:\x`, `/c/x`, `/a/b`, `~/x`, and relative tokens that climb out with `..`.
fn paths_in_command(cmd: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in cmd.split(|c: char| c.is_whitespace() || "|;&<>()=\"'`".contains(c)) {
        let t = raw.trim_matches(|c: char| c == ',' || c == ':' && false);
        if t.is_empty() || t.contains("://") {
            continue;
        }
        let b = t.as_bytes();
        let win_abs = b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b.len() == 2 || b[2] == b'\\' || b[2] == b'/');
        let msys = b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b'/';
        let unix_abs = b[0] == b'/' && t.matches('/').count() >= 2 && !t.starts_with("/dev/") && !t.starts_with("/proc/") && !t.starts_with("/tmp/");
        let home = t.starts_with("~/") || t == "~";
        let climbs = t.starts_with("../") || t.starts_with("..\\") || t.contains("/../") || t.contains("\\..\\") || t == "..";
        if win_abs || msys || home || climbs || (unix_abs && !cfg!(windows)) {
            out.push(t.to_string());
        }
    }
    out
}

pub fn run(root: &Path) -> i32 {
    let mut input = String::new();
    let read = std::io::stdin().read_to_string(&mut input);
    if std::env::var_os("PA_GUARD_DEBUG").is_some() {
        let _ = std::fs::write(std::env::temp_dir().join("pa-guard-debug.txt"), format!("read={read:?} len={} input={input}\n", input.len()));
    }
    let v: Value = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            if std::env::var_os("PA_GUARD_DEBUG").is_some() {
                let _ = std::fs::write(std::env::temp_dir().join("pa-guard-debug.txt"), format!("json error: {e}\ninput={input}\n"));
            }
            return 0;
        }
    };
    let tool = v.get("tool_name").and_then(|t| t.as_str()).unwrap_or("");
    let ti = v.get("tool_input").cloned().unwrap_or(Value::Null);
    let cwd = v.get("cwd").and_then(|c| c.as_str()).map(PathBuf::from).unwrap_or_else(|| root.to_path_buf());
    let field = |k: &str| ti.get(k).and_then(|x| x.as_str()).map(|s| s.to_string());

    let mut targets: Vec<String> = Vec::new();
    match tool {
        "Read" | "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
            if let Some(p) = field("file_path").or_else(|| field("notebook_path")) {
                targets.push(p);
            }
        }
        "Glob" | "Grep" | "LS" => {
            if let Some(p) = field("path") {
                targets.push(p);
            }
        }
        "Bash" => {
            if let Some(c) = field("command") {
                targets.extend(paths_in_command(&c));
            }
        }
        _ => return 0,
    }
    if targets.is_empty() {
        return 0;
    }
    let allowed = allowed_roots(root);
    let protected = protected_roots(root);
    for raw in targets {
        let p = resolve(&raw, &cwd);
        if protected.iter().any(|r| under(&p, r)) {
            eprintln!("pa folder guard: {raw} is a protected path. Protected folders are listed in .pa/protected-folders.txt.");
            return 2;
        }
        if !allowed.iter().any(|r| under(&p, r)) {
            let list = allowed.iter().take(6).map(|a| a.to_string_lossy().to_string()).collect::<Vec<_>>().join(", ");
            eprintln!("pa folder guard: {raw} is outside the allowed folders ({list}). Ask the user to add it to .pa/allowed-folders.txt (one folder per line; a drive root like C:\\ allows everything).");
            return 2;
        }
    }
    0
}
