//! Plan and apply an installation into `<target>/.claude` and `<target>/.pa`, then provision the
//! surroundings (workspace trust, Telegram plugin, Bun, schedule). Every action is recorded as a
//! Step so the UI can show a review (dry run) and a result.

use crate::{pack, personas, provision};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(default)]
pub struct Options {
    pub target: String,
    pub skills: Vec<String>,
    pub agents: bool,
    pub hooks: bool,
    pub mcp: bool,
    pub claude_md: bool,
    pub state: bool,
    pub scripts: bool,
    /// Overwrite pack-managed files (skills, agents, hooks, bin scripts) that already exist.
    pub overwrite: bool,
    pub persona: String,
    pub persona_custom: String,
    pub assistant_name: String,
    pub signoff: String,
    pub timezone: String,
    /// 0 = Sunday .. 6 = Saturday
    pub workdays: Vec<u8>,
    pub hours_start: String,
    pub hours_end: String,
    pub telegram_token: String,
    pub telegram_chat_id: String,
    /// Numeric Telegram user id to allowlist for the channel plugin (usually equals the chat id of a DM).
    pub telegram_user_id: String,
    /// Provisioning switches: mark the folder trusted, install the Telegram plugin, install Bun,
    /// install the proactive schedule, start the phone session at login.
    pub auto_trust: bool,
    pub auto_plugins: bool,
    pub auto_bun: bool,
    pub auto_schedule: bool,
    pub autostart: bool,
    pub dry_run: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            target: String::new(),
            skills: pack::skills().into_iter().map(|s| s.name).collect(),
            agents: true,
            hooks: true,
            mcp: true,
            claude_md: true,
            state: true,
            scripts: true,
            overwrite: true,
            persona: "serious".into(),
            persona_custom: String::new(),
            assistant_name: "Pa".into(),
            signoff: String::new(),
            timezone: String::new(),
            workdays: vec![0, 1, 2, 3, 4],
            hours_start: "09:00".into(),
            hours_end: "18:30".into(),
            telegram_token: String::new(),
            telegram_chat_id: String::new(),
            telegram_user_id: String::new(),
            auto_trust: true,
            auto_plugins: true,
            auto_bun: true,
            auto_schedule: true,
            autostart: true,
            dry_run: false,
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct Step {
    pub path: String,
    pub action: String,
    pub note: String,
}

#[derive(Serialize, Debug)]
pub struct Report {
    pub ok: bool,
    pub dry_run: bool,
    pub target: String,
    pub steps: Vec<Step>,
    pub counts: BTreeMap<String, usize>,
}

const MARK_START: &str = "<!-- pa-pack -->";
const MARK_END: &str = "<!-- /pa-pack -->";
const HOOK_SESSION_START: &str = r#"bash "$CLAUDE_PROJECT_DIR/.claude/hooks/session-start.sh""#;
const HOOK_NOTIFY: &str = r#"bash "$CLAUDE_PROJECT_DIR/.claude/hooks/notify-phone.sh""#;
const PERMISSIONS: &[&str] = &["Read(./.pa/**)", "Edit(./.pa/**)", "Write(./.pa/**)"];
const AUTOSTART_LINE: &str = "session       @login                        pa-up.sh";

fn norm(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// Resolve `~`, make absolute, clean separators.
pub fn normalize(path: &str) -> PathBuf {
    let mut p = path.trim().to_string();
    if p == "~" || p.starts_with("~/") || p.starts_with("~\\") {
        let home = provision::home_dir();
        p = format!("{}{}", home.display(), &p[1..]);
    }
    let pb = PathBuf::from(&p);
    let abs = std::path::absolute(&pb).unwrap_or(pb);
    let mut out = PathBuf::new();
    for c in abs.components() {
        out.push(c.as_os_str());
    }
    let s = out.to_string_lossy().to_string();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        return PathBuf::from(stripped);
    }
    out
}

struct Fs {
    root: PathBuf,
    dry: bool,
    overwrite: bool,
    steps: Vec<Step>,
}

impl Fs {
    fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.root)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| p.to_string_lossy().to_string())
    }
    fn step(&mut self, p: &Path, action: &str, note: impl Into<String>) {
        self.steps.push(Step { path: self.rel(p), action: action.into(), note: note.into() });
    }
    fn label(&mut self, label: &str, action: &str, note: impl Into<String>) {
        self.steps.push(Step { path: label.into(), action: action.into(), note: note.into() });
    }
    fn mkdir(&mut self, p: &Path) {
        if p.is_dir() {
            return;
        }
        if !self.dry {
            if let Err(e) = fs::create_dir_all(p) {
                self.step(p, "error", format!("cannot create directory: {e}"));
                return;
            }
        }
        self.step(p, "mkdir", "");
    }
    fn raw_write(&mut self, p: &Path, content: &str) -> bool {
        if self.dry {
            return true;
        }
        if let Some(parent) = p.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                self.step(p, "error", format!("cannot create parent: {e}"));
                return false;
            }
        }
        match fs::write(p, content.as_bytes()) {
            Ok(_) => true,
            Err(e) => {
                self.step(p, "error", format!("write failed: {e}"));
                false
            }
        }
    }
    /// Write a whole file. `managed` files follow the overwrite setting; others are never replaced.
    fn put(&mut self, p: &Path, content: &str, managed: bool) {
        let content = norm(content);
        match fs::read_to_string(p) {
            Ok(existing) if norm(&existing) == content => self.step(p, "same", "already up to date"),
            Ok(_) => {
                if managed && self.overwrite {
                    if self.raw_write(p, &content) {
                        self.step(p, "update", "replaced with the pack version");
                    }
                } else {
                    self.step(p, "skip", if managed { "exists, overwrite is off" } else { "exists, your copy is kept" });
                }
            }
            Err(_) => {
                if self.raw_write(p, &content) {
                    self.step(p, "create", "");
                }
            }
        }
    }
    /// Write merged content: reports `merge_action` when the file existed and changed.
    fn put_merged(&mut self, p: &Path, content: &str, merge_action: &str, note: impl Into<String>) {
        let content = norm(content);
        match fs::read_to_string(p) {
            Ok(existing) if norm(&existing) == content => self.step(p, "same", "already contains the pa-pack entries"),
            Ok(_) => {
                if self.raw_write(p, &content) {
                    self.step(p, merge_action, note);
                }
            }
            Err(_) => {
                if self.raw_write(p, &content) {
                    self.step(p, "create", note);
                }
            }
        }
    }
    /// Binary file (the tray app). Managed: replaced when the bytes differ and overwrite is on.
    fn put_bytes(&mut self, p: &Path, bytes: &[u8], note: &str) {
        match fs::read(p) {
            Ok(existing) if existing == bytes => self.step(p, "same", "already up to date"),
            Ok(_) if !self.overwrite => self.step(p, "skip", "exists, overwrite is off"),
            Ok(_) => {
                if self.raw_write_bytes(p, bytes) {
                    self.step(p, "update", note);
                }
            }
            Err(_) => {
                if self.raw_write_bytes(p, bytes) {
                    self.step(p, "create", note);
                }
            }
        }
    }
    fn raw_write_bytes(&mut self, p: &Path, bytes: &[u8]) -> bool {
        if self.dry {
            return true;
        }
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        // A running tray app locks its exe on Windows; write beside it and swap.
        let tmp = p.with_extension("new");
        if let Err(e) = fs::write(&tmp, bytes) {
            self.step(p, "error", format!("write failed: {e}"));
            return false;
        }
        if fs::rename(&tmp, p).is_err() {
            let _ = fs::remove_file(&tmp);
            match fs::write(p, bytes) {
                Ok(_) => {}
                Err(e) => {
                    self.step(p, "error", format!("write failed (is the tray app running? quit it and reinstall): {e}"));
                    return false;
                }
            }
        }
        true
    }
    fn chmod_x(&mut self, p: &Path) {
        #[cfg(unix)]
        {
            if self.dry || !p.exists() {
                return;
            }
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = fs::metadata(p) {
                let mut perm = meta.permissions();
                perm.set_mode(perm.mode() | 0o111);
                let _ = fs::set_permissions(p, perm);
            }
        }
        #[cfg(not(unix))]
        {
            let _ = p;
        }
    }
}

/// Skill and agent text was written for the plugin layout (`~/.pa`, `/pa:skill`); rewrite for the project layout.
pub fn localize(src: &str) -> String {
    src.replace("~/.pa", ".pa").replace("/pa:", "/")
}

const DAY_NAMES: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

fn clean_days(days: &[u8]) -> Vec<u8> {
    let mut d: Vec<u8> = days.iter().copied().filter(|d| *d < 7).collect();
    d.sort();
    d.dedup();
    d
}

fn workdays_label(days: &[u8]) -> String {
    let d = clean_days(days);
    if d.is_empty() {
        return "Monday to Friday".into();
    }
    let contiguous = d.windows(2).all(|w| w[1] == w[0] + 1);
    if contiguous && d.len() > 2 {
        format!("{} to {}", DAY_NAMES[d[0] as usize], DAY_NAMES[*d.last().unwrap() as usize])
    } else {
        d.iter().map(|x| DAY_NAMES[*x as usize]).collect::<Vec<_>>().join(", ")
    }
}

fn workdays_cron(days: &[u8]) -> (String, String) {
    let mut d = clean_days(days);
    if d.is_empty() {
        d = vec![1, 2, 3, 4, 5];
    }
    let list = d.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
    (list, d[0].to_string())
}

fn persona_texts(o: &Options) -> (String, String) {
    let name = if o.assistant_name.trim().is_empty() { "Pa" } else { o.assistant_name.trim() };
    match personas::find(&o.persona) {
        Some(p) if p.id != "custom" => (p.charter.replace("{{NAME}}", name), p.voice.to_string()),
        _ => {
            let custom = o.persona_custom.trim();
            let charter = if custom.is_empty() {
                format!("You are {name}, the user's personal assistant.")
            } else {
                custom.replace("{{NAME}}", name)
            };
            (charter, String::new())
        }
    }
}

fn claude_md_block(o: &Options) -> String {
    let (charter, _) = persona_texts(o);
    let base = norm(pack::asset("CLAUDE.md"));
    let base = base.trim_end();
    format!("{base}\n\n## Persona\n{charter}\n{MARK_END}\n")
}

fn merge_claude_md(existing: Option<String>, block: &str) -> String {
    match existing {
        None => block.to_string(),
        Some(text) => {
            let text = norm(&text);
            if let Some(start) = text.find(MARK_START) {
                // Replace the managed region (with or without an end marker from older installs).
                let after = &text[start..];
                let end = after.find(MARK_END).map(|i| start + i + MARK_END.len()).unwrap_or(text.len());
                let mut out = String::new();
                out.push_str(&text[..start]);
                out.push_str(block.trim_end());
                let rest = text[end..].trim_start_matches('\n');
                if !rest.is_empty() {
                    out.push_str("\n\n");
                    out.push_str(rest);
                } else {
                    out.push('\n');
                }
                out
            } else {
                let mut out = text.trim_end().to_string();
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                out.push_str(block);
                out
            }
        }
    }
}

fn merge_settings(existing: Option<String>, telegram_state_dir: &str) -> Result<(String, Vec<String>), String> {
    let mut notes = Vec::new();
    let mut root: Value = match existing {
        Some(t) if !t.trim().is_empty() => serde_json::from_str(&t).map_err(|e| format!("existing settings.json is not valid JSON: {e}"))?,
        _ => json!({}),
    };
    let obj = root.as_object_mut().ok_or("existing settings.json is not a JSON object")?;

    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks.as_object_mut().ok_or("settings.hooks is not an object")?;
    for (event, cmd, marker) in [
        ("SessionStart", HOOK_SESSION_START, "hooks/session-start.sh"),
        ("Notification", HOOK_NOTIFY, "hooks/notify-phone.sh"),
    ] {
        let arr = hooks.entry(event).or_insert_with(|| json!([]));
        let arr = arr.as_array_mut().ok_or(format!("settings.hooks.{event} is not an array"))?;
        let present = arr.iter().any(|entry| {
            entry
                .get("hooks")
                .and_then(|h| h.as_array())
                .map(|h| h.iter().any(|x| x.get("command").and_then(|c| c.as_str()).map(|c| c.contains(marker)).unwrap_or(false)))
                .unwrap_or(false)
        });
        if present {
            notes.push(format!("{event} hook present"));
        } else {
            arr.push(json!({ "hooks": [ { "type": "command", "command": cmd } ] }));
            notes.push(format!("added {event} hook"));
        }
    }

    let perms = obj.entry("permissions").or_insert_with(|| json!({}));
    let perms = perms.as_object_mut().ok_or("settings.permissions is not an object")?;
    let allow = perms.entry("allow").or_insert_with(|| json!([]));
    let allow = allow.as_array_mut().ok_or("settings.permissions.allow is not an array")?;
    let mut added = 0;
    for rule in PERMISSIONS {
        if !allow.iter().any(|v| v.as_str() == Some(rule)) {
            allow.push(Value::String(rule.to_string()));
            added += 1;
        }
    }
    if added > 0 {
        notes.push(format!("allowed {added} .pa/ rules"));
    }

    // Project .mcp.json servers are approved without the dialog.
    if obj.get("enableAllProjectMcpServers").and_then(|b| b.as_bool()) != Some(true) {
        obj.insert("enableAllProjectMcpServers".into(), json!(true));
        notes.push("auto-approve .mcp.json servers".into());
    }

    // The Telegram channel plugin keeps its token and allowlist inside the project.
    let env = obj.entry("env").or_insert_with(|| json!({}));
    let env = env.as_object_mut().ok_or("settings.env is not an object")?;
    if env.get("TELEGRAM_STATE_DIR").and_then(|v| v.as_str()) != Some(telegram_state_dir) {
        env.insert("TELEGRAM_STATE_DIR".into(), json!(telegram_state_dir));
        notes.push("TELEGRAM_STATE_DIR -> .pa/telegram".into());
    }

    let mut text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    text.push('\n');
    Ok((text, notes))
}

fn merge_mcp(existing: Option<String>) -> Result<(String, Vec<String>), String> {
    let mut notes = Vec::new();
    let pack_v: Value = serde_json::from_str(pack::MCP_JSON).map_err(|e| format!("bundled .mcp.json invalid: {e}"))?;
    let pack_servers = pack_v.get("mcpServers").and_then(|v| v.as_object()).cloned().unwrap_or_default();
    let mut root: Value = match existing {
        Some(t) if !t.trim().is_empty() => serde_json::from_str(&t).map_err(|e| format!("existing .mcp.json is not valid JSON: {e}"))?,
        _ => json!({}),
    };
    let obj = root.as_object_mut().ok_or("existing .mcp.json is not a JSON object")?;
    let servers = obj.entry("mcpServers").or_insert_with(|| json!({}));
    let servers = servers.as_object_mut().ok_or(".mcp.json mcpServers is not an object")?;
    let mut added = Vec::new();
    let mut kept = Vec::new();
    for (k, v) in pack_servers {
        if servers.contains_key(&k) {
            kept.push(k);
        } else {
            servers.insert(k.clone(), v);
            added.push(k);
        }
    }
    if !added.is_empty() {
        notes.push(format!("added {}", added.join(", ")));
    }
    if !kept.is_empty() {
        notes.push(format!("kept your {}", kept.join(", ")));
    }
    let mut text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    text.push('\n');
    Ok((text, notes))
}

fn merge_env(existing: Option<String>, pairs: &[(&str, &str)]) -> String {
    let mut lines: Vec<String> = existing.map(|t| norm(&t).lines().map(|l| l.to_string()).collect()).unwrap_or_default();
    for (k, v) in pairs {
        if v.trim().is_empty() {
            continue;
        }
        let line = format!("{k}={}", v.trim());
        if let Some(l) = lines.iter_mut().find(|l| l.starts_with(&format!("{k}="))) {
            *l = line;
        } else {
            lines.push(line);
        }
    }
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// access.json for the Telegram channel plugin: allowlist the discovered user, otherwise leave pairing on.
fn merge_access(existing: Option<String>, user_id: &str) -> Result<String, String> {
    let mut v: Value = match existing {
        Some(t) if !t.trim().is_empty() => serde_json::from_str(&t).map_err(|e| format!("existing access.json is not valid JSON: {e}"))?,
        _ => json!({ "dmPolicy": "pairing", "allowFrom": [] }),
    };
    let obj = v.as_object_mut().ok_or("access.json is not an object")?;
    let uid = user_id.trim();
    if !uid.is_empty() {
        let allow = obj.entry("allowFrom").or_insert_with(|| json!([]));
        let allow = allow.as_array_mut().ok_or("access.json allowFrom is not an array")?;
        if !allow.iter().any(|x| x.as_str() == Some(uid) || x.as_i64().map(|i| i.to_string()) == Some(uid.to_string())) {
            allow.push(Value::String(uid.to_string()));
        }
        obj.insert("dmPolicy".into(), json!("allowlist"));
    }
    let mut text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    text.push('\n');
    Ok(text)
}

/// schedule.txt: keep the user's lines, but make the autostart line follow the option.
fn merge_schedule(existing: Option<String>, generated: &str, autostart: bool) -> String {
    let base = existing.map(|t| norm(&t)).unwrap_or_else(|| generated.to_string());
    let mut lines: Vec<String> = base.lines().filter(|l| !l.trim_start().starts_with("session ")).map(|l| l.to_string()).collect();
    if autostart {
        lines.push(AUTOSTART_LINE.to_string());
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

fn read_opt(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok()
}

pub fn run(o: &Options) -> Report {
    let root = normalize(&o.target);
    let mut f = Fs { root: root.clone(), dry: o.dry_run, overwrite: o.overwrite, steps: Vec::new() };

    if o.target.trim().is_empty() {
        f.steps.push(Step { path: String::new(), action: "error".into(), note: "no target folder".into() });
        return finish(f, o, root);
    }
    if root.is_file() {
        f.step(&root, "error", "target is a file");
        return finish(f, o, root);
    }
    f.mkdir(&root);
    let claude = root.join(".claude");
    let pa = root.join(".pa");
    let tg_dir = pa.join("telegram");

    // Skills: the charter always ships.
    let mut names: Vec<String> = vec!["pa".to_string()];
    for s in &o.skills {
        if s != "pa" && !names.contains(s) {
            names.push(s.clone());
        }
    }
    for name in &names {
        let dest = claude.join("skills").join(name).join("SKILL.md");
        match pack::skill_source(name) {
            Some(src) => f.put(&dest, &localize(src), true),
            None => f.step(&dest, "error", "unknown skill"),
        }
    }

    if o.agents {
        for (file, src) in pack::agents() {
            f.put(&claude.join("agents").join(file), &localize(src), true);
        }
    }

    if o.hooks {
        for (file, src) in pack::asset_files("hooks") {
            let dest = claude.join("hooks").join(&file);
            f.put(&dest, src, true);
            f.chmod_x(&dest);
        }
        let settings = claude.join("settings.json");
        match merge_settings(read_opt(&settings), &tg_dir.to_string_lossy()) {
            Ok((text, notes)) => f.put_merged(&settings, &text, "merge", notes.join("; ")),
            Err(e) => f.step(&settings, "error", e),
        }
    }

    if o.mcp {
        let mcp = root.join(".mcp.json");
        match merge_mcp(read_opt(&mcp)) {
            Ok((text, notes)) => f.put_merged(&mcp, &text, "merge", notes.join("; ")),
            Err(e) => f.step(&mcp, "error", e),
        }
    }

    if o.claude_md {
        let cm = root.join("CLAUDE.md");
        let block = claude_md_block(o);
        let merged = merge_claude_md(read_opt(&cm), &block);
        let note = format!("persona: {}", personas::find(&o.persona).map(|p| p.name).unwrap_or("custom"));
        f.put_merged(&cm, &merged, "update", note);
    }

    if o.state {
        for d in ["log", "inbox", "briefs", "runs", "bin", "telegram"] {
            f.mkdir(&pa.join(d));
        }
        let (_, voice) = persona_texts(o);
        let signoff = if o.signoff.trim().is_empty() { "Me" } else { o.signoff.trim() };
        let tz = if o.timezone.trim().is_empty() { "local time" } else { o.timezone.trim() };
        let mut prefs = pack::asset("state/preferences.md")
            .replace("{{SIGNOFF}}", signoff)
            .replace("{{WORKDAYS}}", &workdays_label(&o.workdays))
            .replace("{{HOURS_START}}", if o.hours_start.is_empty() { "09:00" } else { &o.hours_start })
            .replace("{{HOURS_END}}", if o.hours_end.is_empty() { "18:30" } else { &o.hours_end })
            .replace("{{TIMEZONE}}", tz);
        if !voice.trim().is_empty() {
            prefs = prefs.replacen("- Email sign-off:", &format!("{voice}\n- Email sign-off:"), 1);
        }
        f.put(&pa.join("preferences.md"), &prefs, false);
        f.put(&pa.join("memory.md"), pack::MEMORY_MD, false);
        f.put(&pa.join("followups.json"), "[]\n", false);
        let (dow, first) = workdays_cron(&o.workdays);
        let generated = pack::asset("state/schedule.txt").replace("{{DOW}}", &dow).replace("{{FIRST_DOW}}", &first);
        let sched_path = pa.join("schedule.txt");
        let sched = merge_schedule(read_opt(&sched_path), &generated, o.autostart);
        f.put_merged(&sched_path, &sched, "update", if o.autostart { "phone session starts at login" } else { "no autostart line" });
        f.put(&pa.join(".gitignore"), ".env\nruns/\ntelegram/\n", false);

        // Telegram: chat id for pushes (.pa/.env), token + allowlist for the channel plugin (.pa/telegram/).
        let env_path = pa.join(".env");
        if !o.telegram_chat_id.trim().is_empty() {
            let text = merge_env(read_opt(&env_path), &[("TELEGRAM_CHAT_ID", &o.telegram_chat_id)]);
            f.put_merged(&env_path, &text, "merge", "Telegram chat id for pushes");
        } else if !env_path.exists() {
            f.put(&env_path, "", false);
        }
        if !o.telegram_token.trim().is_empty() {
            let tg_env = tg_dir.join(".env");
            let text = merge_env(read_opt(&tg_env), &[("TELEGRAM_BOT_TOKEN", &o.telegram_token)]);
            f.put_merged(&tg_env, &text, "merge", "bot token for the channel plugin");
        }
        let access = tg_dir.join("access.json");
        match merge_access(read_opt(&access), &o.telegram_user_id) {
            Ok(text) => {
                let note = if o.telegram_user_id.trim().is_empty() { "pairing mode (no user detected yet)".to_string() } else { format!("allowlist: user {}", o.telegram_user_id.trim()) };
                f.put_merged(&access, &text, "merge", note)
            }
            Err(e) => f.step(&access, "error", e),
        }
    }

    if o.scripts {
        let claude_path = provision::claude_bin().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| "claude".into());
        for (file, src) in pack::asset_files("bin") {
            if file.ends_with(".cmd") && !cfg!(windows) {
                continue;
            }
            let dest = pa.join("bin").join(&file);
            f.put(&dest, &src.replace("{{CLAUDE}}", &claude_path), true);
            f.chmod_x(&dest);
        }
        // The tray app (always-on phone session with a tray icon), when it was embedded at build time.
        if !pack::TRAY_BIN.is_empty() {
            let dest = pa.join("bin").join(pack::TRAY_NAME.trim());
            f.put_bytes(&dest, pack::TRAY_BIN, "tray app: keeps the phone session running in the background");
            f.chmod_x(&dest);
        } else {
            f.label("tray app", "skip", "this installer build has no pa-tray binary embedded");
        }
        // Launchers in the project root: `pa` adds the Telegram channel flag that plain `claude` lacks.
        for (file, src) in pack::asset_files("launch") {
            if file.ends_with(".cmd") && !cfg!(windows) {
                continue;
            }
            if file == "pa-tray" && pack::TRAY_BIN.is_empty() {
                continue;
            }
            if file == "pa-tray.cmd" && pack::TRAY_BIN.is_empty() {
                continue;
            }
            let dest = root.join(&file);
            f.put(&dest, &src.replace("{{CLAUDE}}", &claude_path), true);
            f.chmod_x(&dest);
        }
    }

    provision(&mut f, o, &root);
    finish(f, o, root)
}

/// Allowlist a Telegram user in an existing install (Done page "Detect me now"): access.json + .pa/.env.
pub fn apply_telegram_user(path: &str, user_id: &str, chat_id: &str) -> Result<String, String> {
    let root = normalize(path);
    let pa = root.join(".pa");
    if !pa.is_dir() {
        return Err("not installed here yet".into());
    }
    let access = pa.join("telegram").join("access.json");
    let text = merge_access(read_opt(&access), user_id)?;
    fs::create_dir_all(access.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(&access, text).map_err(|e| e.to_string())?;
    let env_path = pa.join(".env");
    let env = merge_env(read_opt(&env_path), &[("TELEGRAM_CHAT_ID", chat_id)]);
    fs::write(&env_path, env).map_err(|e| e.to_string())?;
    Ok(format!("user {user_id} allowlisted (policy allowlist), chat id {chat_id} saved for pushes"))
}

/// Bot token of an existing install, from .pa/telegram/.env.
pub fn stored_token(path: &str) -> Option<String> {
    let p = normalize(path).join(".pa").join("telegram").join(".env");
    read_opt(&p)?.lines().find_map(|l| l.strip_prefix("TELEGRAM_BOT_TOKEN=").map(|v| v.trim().to_string())).filter(|t| !t.is_empty())
}

/// Post-file steps. In a dry run they are listed as `todo`.
fn provision(f: &mut Fs, o: &Options, root: &Path) {
    let dry = o.dry_run;
    if o.auto_trust {
        if dry {
            let already = provision::is_trusted(root);
            f.label("workspace trust", if already { "same" } else { "todo" }, "mark the folder trusted in ~/.claude.json (skips the trust dialog, enables the .pa/ permission rules)");
        } else {
            match provision::set_trust(root) {
                Ok(true) => f.label("workspace trust", "ok", "folder marked trusted in ~/.claude.json"),
                Ok(false) => f.label("workspace trust", "same", "already trusted"),
                Err(e) => f.label("workspace trust", "error", e),
            }
        }
    }
    if o.auto_plugins {
        if dry {
            f.label("marketplace", if provision::marketplace_known(provision::OFFICIAL_MARKETPLACE) { "same" } else { "todo" }, "register anthropics/claude-plugins-official");
            f.label("plugin telegram", if provision::plugin_enabled(root, provision::TELEGRAM_PLUGIN) { "same" } else { "todo" }, "claude plugin install telegram@claude-plugins-official --scope project");
        } else {
            let mut ok = true;
            match provision::ensure_marketplace(root) {
                Ok((true, out)) => f.label("marketplace", "ok", last_line(&out)),
                Ok((false, note)) => f.label("marketplace", "same", note),
                Err(e) => {
                    ok = false;
                    f.label("marketplace", "error", e)
                }
            }
            if ok {
                match provision::install_plugin(root, provision::TELEGRAM_PLUGIN) {
                    Ok((true, out)) => f.label("plugin telegram", "ok", last_line(&out)),
                    Ok((false, note)) => f.label("plugin telegram", "same", note),
                    Err(e) => f.label("plugin telegram", "error", e),
                }
            }
        }
    }
    if o.auto_bun {
        if provision::bun_bin().is_some() {
            f.label("bun", "same", "already installed");
        } else if dry {
            f.label("bun", "todo", "install Bun (the Telegram channel runs on it)");
        } else {
            match provision::install_bun() {
                Ok(_) => f.label("bun", "ok", "installed; new terminals pick it up from PATH"),
                Err(e) => f.label("bun", "error", e),
            }
        }
    }
    if o.auto_schedule && o.scripts && o.state {
        if dry {
            f.label("schedule", if provision::schedule_installed(root) { "same" } else { "todo" }, if cfg!(windows) { "Task Scheduler tasks from .pa/schedule.txt" } else if cfg!(target_os = "macos") { "launchd jobs from .pa/schedule.txt" } else { "crontab lines from .pa/schedule.txt" });
        } else {
            match provision::schedule_script(root, "", false) {
                Ok((true, out)) => f.label("schedule", "ok", out.lines().filter(|l| !l.starts_with("Tip:")).collect::<Vec<_>>().join(" | ")),
                Ok((false, out)) => f.label("schedule", "error", out),
                Err(e) => f.label("schedule", "error", e),
            }
        }
    }
}

fn last_line(s: &str) -> String {
    s.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string()
}

fn finish(f: Fs, o: &Options, root: PathBuf) -> Report {
    let mut counts = BTreeMap::new();
    for s in &f.steps {
        *counts.entry(s.action.clone()).or_insert(0) += 1;
    }
    let ok = !f.steps.iter().any(|s| s.action == "error");
    Report { ok, dry_run: o.dry_run, target: root.to_string_lossy().to_string(), steps: f.steps, counts }
}

/// Facts about a candidate target folder, for the UI.
pub fn inspect(path: &str) -> Value {
    let p = normalize(path);
    let mut m = Map::new();
    m.insert("path".into(), json!(p.to_string_lossy()));
    m.insert("exists".into(), json!(p.is_dir()));
    m.insert("is_file".into(), json!(p.is_file()));
    m.insert("has_claude_dir".into(), json!(p.join(".claude").is_dir()));
    m.insert("has_claude_md".into(), json!(p.join("CLAUDE.md").is_file()));
    m.insert("has_pa".into(), json!(p.join(".pa").is_dir()));
    m.insert("has_settings".into(), json!(p.join(".claude").join("settings.json").is_file()));
    m.insert("has_mcp".into(), json!(p.join(".mcp.json").is_file()));
    m.insert("is_git".into(), json!(p.join(".git").exists()));
    let empty = p.is_dir() && fs::read_dir(&p).map(|mut d| d.next().is_none()).unwrap_or(false);
    m.insert("empty".into(), json!(empty));
    Value::Object(m)
}

/// Live status of an installed folder, for the Done board.
pub fn status(path: &str) -> Value {
    let root = normalize(path);
    let pa = root.join(".pa");
    let tg_env = read_opt(&pa.join("telegram").join(".env")).unwrap_or_default();
    let has_token = tg_env.lines().any(|l| l.starts_with("TELEGRAM_BOT_TOKEN=") && l.len() > 20);
    let access: Value = read_opt(&pa.join("telegram").join("access.json")).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(json!({}));
    let allow = access.get("allowFrom").and_then(|a| a.as_array()).map(|a| a.len()).unwrap_or(0);
    let sched = read_opt(&pa.join("schedule.txt")).unwrap_or_default();
    let skills = fs::read_dir(root.join(".claude").join("skills")).map(|d| d.flatten().count()).unwrap_or(0);
    json!({
        "target": root.to_string_lossy(),
        "installed": root.join(".claude").join("skills").join("pa").join("SKILL.md").is_file(),
        "skills": skills,
        "hooks": root.join(".claude").join("hooks").join("session-start.sh").is_file(),
        "launcher": root.join("pa").is_file() || root.join("pa.cmd").is_file(),
        "tray": pa.join("bin").join(pack::TRAY_NAME.trim()).is_file(),
        "claude_md": read_opt(&root.join("CLAUDE.md")).map(|t| t.contains(MARK_START)).unwrap_or(false),
        "trusted": provision::is_trusted(&root),
        "marketplace": provision::marketplace_known(provision::OFFICIAL_MARKETPLACE),
        "plugin": provision::plugin_enabled(&root, provision::TELEGRAM_PLUGIN),
        "bun": provision::bun_bin().map(|p| p.to_string_lossy().to_string()),
        "telegram_token": has_token,
        "telegram_users": allow,
        "telegram_policy": access.get("dmPolicy").cloned().unwrap_or(json!("pairing")),
        "chat_id": read_opt(&pa.join(".env")).map(|t| t.lines().any(|l| l.starts_with("TELEGRAM_CHAT_ID=") && l.len() > 17)).unwrap_or(false),
        "schedule": provision::schedule_installed(&root),
        "autostart": sched.lines().any(|l| l.trim_start().starts_with("session ")),
        "auth": provision::auth_status(),
        "claude": provision::claude_bin().map(|p| p.to_string_lossy().to_string()),
        "bash": provision::find_bash().map(|p| p.to_string_lossy().to_string()),
    })
}
