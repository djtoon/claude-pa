//! Everything that happens around the file writes: workspace trust, marketplace + plugins, Bun,
//! the proactive schedule, sign-in / service status, Telegram Bot API calls, terminals, and a tiny
//! background-job registry for the slow checks. All shell-outs live here.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

pub const OFFICIAL_MARKETPLACE: &str = "claude-plugins-official";
pub const OFFICIAL_MARKETPLACE_SRC: &str = "anthropics/claude-plugins-official";
pub const TELEGRAM_PLUGIN: &str = "telegram@claude-plugins-official";

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn os_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// Minimal PATH lookup (no external crate).
pub fn which(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    // Windows: prefer real launchers over extensionless shell shims (npm drops both next to each other).
    let exts: Vec<&str> = if cfg!(windows) { vec![".exe", ".cmd", ".bat", ""] } else { vec![""] };
    for dir in std::env::split_paths(&path) {
        for ext in &exts {
            let p = dir.join(format!("{cmd}{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn exe(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

/// Find a bash to run the shipped scripts with. On Windows that is Git Bash.
pub fn find_bash() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(p) = which("bash") {
        // On Windows, avoid the WSL launcher in System32; prefer Git's bash.
        if !(cfg!(windows) && p.to_string_lossy().to_lowercase().contains("system32")) {
            return Some(p);
        }
    }
    if cfg!(windows) {
        for c in [
            r"C:\Program Files\Git\bin\bash.exe",
            r"C:\Program Files\Git\usr\bin\bash.exe",
            r"C:\Program Files (x86)\Git\bin\bash.exe",
        ] {
            if Path::new(c).is_file() {
                return Some(PathBuf::from(c));
            }
        }
        let local = home_dir().join(r"AppData\Local\Programs\Git\bin\bash.exe");
        if local.is_file() {
            return Some(local);
        }
    }
    None
}

pub fn claude_bin() -> Option<PathBuf> {
    which("claude").or_else(|| {
        let p = home_dir().join(".local").join("bin").join(exe("claude"));
        p.is_file().then_some(p)
    })
}

pub fn bun_bin() -> Option<PathBuf> {
    which("bun").or_else(|| {
        let p = home_dir().join(".bun").join("bin").join(exe("bun"));
        p.is_file().then_some(p)
    })
}

fn cmd_output(mut c: Command) -> Result<(bool, String), String> {
    c.stdin(Stdio::null());
    // A nested Claude Code refuses to start; the installer may well be launched from inside one.
    c.env_remove("CLAUDECODE");
    let out = c.output().map_err(|e| e.to_string())?;
    let mut text = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
    let err = String::from_utf8_lossy(&out.stderr);
    if !err.trim().is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(err.trim_end());
    }
    Ok((out.status.success(), text))
}

pub fn run_claude(cwd: Option<&Path>, args: &[&str]) -> Result<(bool, String), String> {
    let bin = claude_bin().ok_or("claude CLI not found. Install Claude Code first.")?;
    let mut c = Command::new(bin);
    c.args(args);
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    cmd_output(c)
}

pub fn run_bash(cwd: &Path, script: &Path, args: &[&str], env: &[(&str, String)]) -> Result<(bool, String), String> {
    let bash = find_bash().ok_or("bash not found. On Windows install Git for Windows (Git Bash).")?;
    if !script.is_file() {
        return Err(format!("{} not found", script.display()));
    }
    let mut c = Command::new(bash);
    c.arg(script).args(args).current_dir(cwd);
    for (k, v) in env {
        c.env(k, v);
    }
    cmd_output(c)
}

// ---------- workspace trust (~/.claude.json) ----------

pub fn claude_json_path() -> PathBuf {
    home_dir().join(".claude.json")
}

/// Claude Code keys projects by their path with forward slashes.
pub fn project_key(root: &Path) -> String {
    root.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_string()
}

fn read_claude_json() -> Result<Value, String> {
    let p = claude_json_path();
    if !p.is_file() {
        return Ok(json!({}));
    }
    let text = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("{} is not valid JSON: {e}", p.display()))
}

pub fn is_trusted(root: &Path) -> bool {
    read_claude_json()
        .ok()
        .and_then(|v| v.get("projects")?.get(project_key(root))?.get("hasTrustDialogAccepted")?.as_bool())
        .unwrap_or(false)
}

/// Mark the folder trusted so Claude Code applies the project's permissions without the dialog. Returns whether anything changed.
pub fn set_trust(root: &Path) -> Result<bool, String> {
    let mut v = read_claude_json()?;
    let obj = v.as_object_mut().ok_or("~/.claude.json is not an object")?;
    let projects = obj.entry("projects").or_insert_with(|| json!({}));
    let projects = projects.as_object_mut().ok_or("~/.claude.json projects is not an object")?;
    let entry = projects.entry(project_key(root)).or_insert_with(|| json!({}));
    let entry = entry.as_object_mut().ok_or("project entry is not an object")?;
    if entry.get("hasTrustDialogAccepted").and_then(|b| b.as_bool()) == Some(true) {
        return Ok(false);
    }
    entry.insert("hasTrustDialogAccepted".into(), json!(true));
    let text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    let p = claude_json_path();
    let tmp = p.with_extension("json.pa-tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
    Ok(true)
}

// ---------- marketplace + plugins ----------

pub fn marketplace_known(name: &str) -> bool {
    let p = home_dir().join(".claude").join("plugins").join("known_marketplaces.json");
    std::fs::read_to_string(p)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .map(|v| v.get(name).is_some())
        .unwrap_or(false)
}

pub fn ensure_marketplace(cwd: &Path) -> Result<(bool, String), String> {
    if marketplace_known(OFFICIAL_MARKETPLACE) {
        return Ok((false, "already registered".into()));
    }
    let (ok, out) = run_claude(Some(cwd), &["plugin", "marketplace", "add", OFFICIAL_MARKETPLACE_SRC])?;
    if ok { Ok((true, out)) } else { Err(out) }
}

pub fn plugin_enabled(root: &Path, id: &str) -> bool {
    let p = root.join(".claude").join("settings.json");
    std::fs::read_to_string(p)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("enabledPlugins")?.get(id)?.as_bool())
        .unwrap_or(false)
}

pub fn install_plugin(root: &Path, id: &str) -> Result<(bool, String), String> {
    if plugin_enabled(root, id) {
        return Ok((false, "already enabled for this project".into()));
    }
    let (ok, out) = run_claude(Some(root), &["plugin", "install", id, "--scope", "project", "-y"])?;
    if ok { Ok((true, out)) } else { Err(out) }
}

// ---------- Bun ----------

pub fn install_bun() -> Result<String, String> {
    let c = if cfg!(windows) {
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", "irm bun.sh/install.ps1 | iex"]);
        c
    } else {
        let mut c = Command::new("bash");
        c.args(["-c", "curl -fsSL https://bun.sh/install | bash"]);
        c
    };
    let (ok, out) = cmd_output(c)?;
    if ok && bun_bin().is_some() {
        Ok(out)
    } else {
        Err(format!("bun install did not complete:\n{out}"))
    }
}

// ---------- schedule ----------

/// Same sanitisation as pa-schedule.sh: anything but [A-Za-z0-9_] becomes '-'.
pub fn proj_name(root: &Path) -> String {
    let base = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    base.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '-' }).collect()
}

pub fn schedule_installed(root: &Path) -> bool {
    let proj = proj_name(root);
    if cfg!(windows) {
        let mut c = Command::new("schtasks");
        c.args(["/Query", "/TN", &format!("pa-{proj}\\morning")]);
        cmd_output(c).map(|(ok, _)| ok).unwrap_or(false)
    } else if cfg!(target_os = "macos") {
        home_dir().join("Library/LaunchAgents").join(format!("com.pa.{proj}.morning.plist")).is_file()
    } else {
        let mut c = Command::new("crontab");
        c.arg("-l");
        cmd_output(c).map(|(_, out)| out.contains(&format!("# pa:{}", root.to_string_lossy()))).unwrap_or(false)
    }
}

pub fn schedule_script(root: &Path, mode: &str, dry: bool) -> Result<(bool, String), String> {
    let script = root.join(".pa").join("bin").join("pa-schedule.sh");
    let args: Vec<&str> = if mode.is_empty() { vec![] } else { vec![mode] };
    let env = if dry { vec![("PA_SCHEDULE_DRY", "1".to_string())] } else { vec![] };
    run_bash(root, &script, &args, &env)
}

// ---------- sign-in ----------

pub fn auth_status() -> Value {
    match run_claude(None, &["auth", "status", "--json"]) {
        Ok((_, out)) => {
            let start = out.find('{').unwrap_or(0);
            match serde_json::from_str::<Value>(&out[start..]) {
                Ok(v) => json!({
                    "logged_in": v.get("loggedIn").and_then(|b| b.as_bool()).unwrap_or(false),
                    "email": v.get("email").cloned().unwrap_or(Value::Null),
                    "method": v.get("authMethod").cloned().unwrap_or(Value::Null),
                }),
                Err(_) => json!({ "logged_in": false, "error": out }),
            }
        }
        Err(e) => json!({ "logged_in": false, "error": e }),
    }
}

// ---------- Telegram Bot API (via curl, present on every supported OS) ----------

pub fn telegram_api(token: &str, method: &str) -> Result<Value, String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("no bot token".into());
    }
    let mut c = Command::new("curl");
    c.args(["-sS", "--max-time", "20", &format!("https://api.telegram.org/bot{token}/{method}")]);
    let (ok, out) = cmd_output(c)?;
    if !ok && out.trim().is_empty() {
        return Err("curl failed".into());
    }
    let v: Value = serde_json::from_str(&out).map_err(|_| format!("Telegram did not answer with JSON: {}", out.chars().take(200).collect::<String>()))?;
    if v.get("ok").and_then(|b| b.as_bool()) != Some(true) {
        return Err(v.get("description").and_then(|d| d.as_str()).unwrap_or("Telegram API error").to_string());
    }
    Ok(v)
}

pub fn telegram_me(token: &str) -> Value {
    match telegram_api(token, "getMe") {
        Ok(v) => {
            let r = &v["result"];
            json!({ "ok": true, "username": r["username"], "first_name": r["first_name"], "id": r["id"] })
        }
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

/// Look at the latest update: whoever wrote to the bot last is the user we allowlist.
pub fn telegram_detect(token: &str) -> Value {
    match telegram_api(token, "getUpdates?limit=10") {
        Ok(v) => {
            let empty = vec![];
            let updates = v["result"].as_array().unwrap_or(&empty);
            let last = updates
                .iter()
                .rev()
                .find_map(|u| u.get("message").or_else(|| u.get("edited_message")).cloned());
            match last {
                Some(m) => json!({
                    "ok": true,
                    "user_id": m["from"]["id"].as_i64().map(|i| i.to_string()),
                    "chat_id": m["chat"]["id"].as_i64().map(|i| i.to_string()),
                    "first_name": m["from"]["first_name"],
                    "username": m["from"]["username"],
                    "text": m["text"],
                }),
                None => json!({ "ok": false, "error": "No messages yet. Open the bot in Telegram, send it any message, then detect again." }),
            }
        }
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

// ---------- terminals, folders, browser ----------

pub fn open_in_browser(url: &str) -> Result<(), String> {
    let r = if cfg!(windows) {
        Command::new("cmd").args(["/C", "start", "", url]).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn()
    } else {
        Command::new("xdg-open").arg(url).spawn()
    };
    r.map(|_| ()).map_err(|e| e.to_string())
}

pub fn open_folder(path: &str) -> Result<(), String> {
    let r = if cfg!(windows) {
        Command::new("explorer").arg(path).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(path).spawn()
    } else {
        Command::new("xdg-open").arg(path).spawn()
    };
    r.map(|_| ()).map_err(|e| e.to_string())
}

/// Open a visible terminal window in `dir` running `cmd` (which stays open afterwards).
pub fn open_terminal(dir: &Path, cmd: &str) -> Result<(), String> {
    let d = dir.to_string_lossy().to_string();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // start's /D sets the working directory, so the inner command needs no nested quoting.
        let mut c = Command::new("cmd");
        c.raw_arg(format!("/C start \"pa assistant\" /D \"{d}\" cmd /K {cmd}"));
        return c.spawn().map(|_| ()).map_err(|e| e.to_string());
    }
    #[cfg(target_os = "macos")]
    {
        let script = format!("cd '{}' && {}", d.replace('\'', "'\\''"), cmd);
        return Command::new("osascript")
            .args(["-e", &format!("tell application \"Terminal\" to do script \"{}\"", script.replace('"', "\\\"")), "-e", "tell application \"Terminal\" to activate"])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let script = format!("cd '{}' && {}; exec bash", d.replace('\'', "'\\''"), cmd);
        let attempts: Vec<(&str, Vec<&str>)> = vec![
            ("x-terminal-emulator", vec!["-e", "bash", "-lc", &script]),
            ("gnome-terminal", vec!["--", "bash", "-lc", &script]),
            ("konsole", vec!["-e", "bash", "-lc", &script]),
            ("xfce4-terminal", vec!["-e", &format!("bash -lc \"{}\"", script.replace('"', "\\\""))]),
            ("xterm", vec!["-e", "bash", "-lc", &script]),
        ];
        for (bin, args) in attempts {
            if which(bin).is_some() {
                return Command::new(bin).args(&args).spawn().map(|_| ()).map_err(|e| e.to_string());
            }
        }
        return Err("no terminal emulator found; run it yourself:\n  ".to_string() + &script);
    }
    #[allow(unreachable_code)]
    Err("unsupported platform".into())
}

// ---------- `claude mcp list` parsing ----------

pub fn parse_mcp_list(text: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((left, right)) = line.rsplit_once(" - ") else { continue };
        let name = left.split(": ").next().unwrap_or(left).trim();
        let lower = right.to_lowercase();
        let status = if lower.contains("connected") && !lower.contains("failed") {
            "connected"
        } else if lower.contains("needs authentication") {
            "needs_auth"
        } else if lower.contains("pending") {
            "pending"
        } else if lower.contains("failed") || lower.contains("error") {
            "failed"
        } else {
            "unknown"
        };
        let n = name.to_lowercase();
        let relevant = ["gmail", "calendar", "gcal", "drive", "slack", "atlassian", "jira"].iter().any(|k| n.contains(k));
        out.push(json!({ "name": name, "status": status, "detail": right, "relevant": relevant }));
    }
    out
}

// ---------- background jobs ----------

#[derive(Clone)]
struct JobState {
    label: String,
    done: bool,
    ok: bool,
    output: String,
}

#[derive(Clone, Default)]
pub struct Jobs {
    inner: Arc<Mutex<HashMap<u64, JobState>>>,
    next: Arc<Mutex<u64>>,
}

impl Jobs {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn spawn<F>(&self, label: &str, f: F) -> u64
    where
        F: FnOnce() -> (bool, String) + Send + 'static,
    {
        let id = {
            let mut n = self.next.lock().unwrap();
            *n += 1;
            *n
        };
        self.inner.lock().unwrap().insert(id, JobState { label: label.into(), done: false, ok: false, output: String::new() });
        let inner = self.inner.clone();
        std::thread::spawn(move || {
            let (ok, output) = f();
            if let Some(j) = inner.lock().unwrap().get_mut(&id) {
                j.done = true;
                j.ok = ok;
                j.output = output;
            }
        });
        id
    }
    pub fn get(&self, id: u64) -> Value {
        match self.inner.lock().unwrap().get(&id) {
            Some(j) => json!({ "id": id, "label": j.label, "done": j.done, "ok": j.ok, "output": j.output }),
            None => json!({ "id": id, "done": true, "ok": false, "output": "unknown job" }),
        }
    }
}
