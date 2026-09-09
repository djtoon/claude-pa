//! pa-installer: installs the pa-pack personal-assistant kit into a project's `.claude` folder
//! through a local web UI (default) or a plain CLI (`install` subcommand), and provisions what
//! surrounds it (trust, Telegram plugin, Bun, schedule) so the assistant works right after install.

mod install;
mod pack;
mod personas;
mod provision;

use serde_json::{json, Value};
use std::hash::{BuildHasher, Hasher};
use std::path::Path;
use tiny_http::{Header, Method, Request, Response, Server};

fn random_token() -> String {
    let mut t = String::new();
    for _ in 0..2 {
        let h = std::collections::hash_map::RandomState::new().build_hasher().finish();
        t.push_str(&format!("{h:016x}"));
    }
    t
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = &s[i + 1..i + 3];
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn query_param(url: &str, key: &str) -> Option<String> {
    let (_, q) = url.split_once('?')?;
    for pair in q.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k == key {
            return Some(percent_decode(v));
        }
    }
    None
}

fn header(req: &Request, name: &str) -> Option<String> {
    req.headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str().to_string())
}

fn json_response(status: u16, v: &Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(v.to_string())
        .with_status_code(status)
        .with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap())
        .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
}

fn html_response(body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(body)
        .with_header(Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap())
        .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
}

fn browse(path: &str) -> Value {
    let hidden = |name: &str| name.starts_with('.');
    if path.trim().is_empty() {
        if cfg!(windows) {
            let mut roots = Vec::new();
            for c in b'A'..=b'Z' {
                let p = format!("{}:\\", c as char);
                if Path::new(&p).is_dir() {
                    roots.push(json!({ "name": p, "path": p, "hidden": false }));
                }
            }
            return json!({ "path": "", "parent": null, "exists": true, "dirs": roots, "roots": true });
        }
        return browse("/");
    }
    let p = install::normalize(path);
    if !p.is_dir() {
        return json!({ "path": p.to_string_lossy(), "parent": p.parent().map(|x| x.to_string_lossy().to_string()), "exists": false, "dirs": [] });
    }
    let mut dirs: Vec<(String, String, bool)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&p) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let full = e.path();
            if full.is_dir() {
                dirs.push((name.clone(), full.to_string_lossy().to_string(), hidden(&name)));
            }
        }
    }
    dirs.sort_by_key(|(n, _, h)| (*h, n.to_lowercase()));
    let parent = match p.parent() {
        Some(x) if !x.as_os_str().is_empty() => Some(x.to_string_lossy().to_string()),
        _ => {
            if cfg!(windows) {
                Some(String::new())
            } else {
                None
            }
        }
    };
    let mut v = install::inspect(&p.to_string_lossy());
    v["parent"] = json!(parent);
    v["dirs"] = json!(dirs
        .into_iter()
        .map(|(name, path, hidden)| json!({ "name": name, "path": path, "hidden": hidden }))
        .collect::<Vec<_>>());
    v
}

fn info(target_hint: &Option<String>) -> Value {
    let cwd = std::env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    let home = provision::home_dir().to_string_lossy().to_string();
    let suggested = target_hint.clone().unwrap_or_else(|| cwd.clone());
    json!({
        "version": pack::VERSION,
        "os": provision::os_name(),
        "home": home,
        "cwd": cwd,
        "suggested": suggested,
        "skills": pack::skills().into_iter().map(|mut s| { s.description = install::localize(&s.description); s }).collect::<Vec<_>>(),
        "agents": pack::agents().into_iter().map(|(f, _)| f.trim_end_matches(".md").to_string()).collect::<Vec<_>>(),
        "personas": personas::PERSONAS,
        "tools": {
            "bash": provision::find_bash().map(|p| p.to_string_lossy().to_string()),
            "claude": provision::claude_bin().map(|p| p.to_string_lossy().to_string()),
            "bun": provision::bun_bin().map(|p| p.to_string_lossy().to_string()),
            "jq": provision::which("jq").is_some(),
            "python3": provision::which("python3").is_some() || provision::which("python").is_some(),
            "tmux": provision::which("tmux").is_some(),
            "curl": provision::which("curl").is_some(),
            "marketplace": provision::marketplace_known(provision::OFFICIAL_MARKETPLACE),
        },
        "auth": provision::auth_status(),
    })
}

/// Synchronous one-shot actions from the Done page.
fn run_action(target: &str, what: &str, name: &str) -> Value {
    let root = install::normalize(target);
    let res = |r: Result<(bool, String), String>| match r {
        Ok((ok, out)) => json!({ "ok": ok, "output": out }),
        Err(e) => json!({ "ok": false, "output": e }),
    };
    match what {
        // claude.ai connectors: `claude mcp login --no-browser` prints a claude.ai authorization URL and exits;
        // the approval happens on claude.ai and applies to the next session. We open that URL ourselves.
        "authorize" => {
            if name.trim().is_empty() {
                return json!({ "ok": false, "output": "no server name" });
            }
            match provision::run_claude(Some(&root), &["mcp", "login", name, "--no-browser"]) {
                Ok((_, out)) => {
                    let url = out.split_whitespace().find(|w| w.starts_with("https://") || w.starts_with("http://")).map(|s| s.trim_end_matches(['.', ',', ')']).to_string());
                    match url {
                        Some(u) => {
                            let opened = provision::open_in_browser(&u).is_ok();
                            let msg = if opened {
                                format!("Opened the claude.ai authorization page for \"{name}\" in your browser. Approve it there, then re-check services. Connectors become available in the next Claude Code session.")
                            } else {
                                format!("Could not open a browser automatically. Open this URL yourself to authorize \"{name}\":\n{u}")
                            };
                            json!({ "ok": true, "url": u, "output": msg })
                        }
                        None => json!({ "ok": false, "output": out }),
                    }
                }
                Err(e) => json!({ "ok": false, "output": e }),
            }
        }
        "open" => match provision::open_folder(&root.to_string_lossy()) {
            Ok(_) => json!({ "ok": true, "output": "opened" }),
            Err(e) => json!({ "ok": false, "output": e }),
        },
        "terminal" => match provision::open_terminal(&root, "claude") {
            Ok(_) => json!({ "ok": true, "output": "Opened a terminal running claude in the folder. Use /mcp there to authenticate services." }),
            Err(e) => json!({ "ok": false, "output": e }),
        },
        "login" => match provision::open_terminal(&root, "claude auth login") {
            Ok(_) => json!({ "ok": true, "output": "Opened a terminal running `claude auth login`. Finish the sign-in in the browser, then refresh the status here." }),
            Err(e) => json!({ "ok": false, "output": e }),
        },
        "detect" => {
            let Some(tok) = install::stored_token(target) else {
                return json!({ "ok": false, "output": "No bot token in .pa/telegram/.env. Paste it in Setup and reinstall." });
            };
            let d = provision::telegram_detect(&tok);
            if d.get("ok").and_then(|b| b.as_bool()) != Some(true) {
                return json!({ "ok": false, "output": d.get("error").and_then(|e| e.as_str()).unwrap_or("detect failed") });
            }
            let uid = d.get("user_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let chat = d.get("chat_id").and_then(|v| v.as_str()).unwrap_or(&uid).to_string();
            match install::apply_telegram_user(target, &uid, &chat) {
                Ok(msg) => json!({ "ok": true, "output": format!("{} ({}) — {msg}. Restart the phone session (./pa) for the allowlist to apply to a running bot.", d.get("first_name").and_then(|v| v.as_str()).unwrap_or(""), d.get("username").and_then(|v| v.as_str()).map(|u| format!("@{u}")).unwrap_or_default()) }),
                Err(e) => json!({ "ok": false, "output": e }),
            }
        }
        "hook" => res(provision::run_bash(&root, &root.join(".claude/hooks/session-start.sh"), &[], &[("CLAUDE_PROJECT_DIR", root.to_string_lossy().to_string())])),
        "notify" => res(provision::run_bash(&root, &root.join(".pa/bin/notify.sh"), &["✅ pa-installer test message"], &[])),
        "up" => res(provision::run_bash(&root, &root.join(".pa/bin/pa-up.sh"), &[], &[])),
        "tray" => {
            let exe = root.join(".pa").join("bin").join(pack::TRAY_NAME.trim());
            if !exe.is_file() {
                return json!({ "ok": false, "output": "tray app not installed (this installer build may lack it)" });
            }
            let mut c = std::process::Command::new(&exe);
            c.current_dir(&root).env_remove("CLAUDECODE").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
            match c.spawn() {
                Ok(_) => json!({ "ok": true, "output": "Tray app started: look for the orange dot in your tray / menu bar. It starts the phone session hidden; use its menu to show, stop or quit." }),
                Err(e) => json!({ "ok": false, "output": format!("could not start the tray app: {e}") }),
            }
        }
        "schedule" => res(provision::schedule_script(&root, "", false)),
        "schedule-dry" => res(provision::schedule_script(&root, "", true)),
        "schedule-status" => res(provision::schedule_script(&root, "status", false)),
        "schedule-remove" => res(provision::schedule_script(&root, "remove", false)),
        "trust" => match provision::set_trust(&root) {
            Ok(changed) => json!({ "ok": true, "output": if changed { "folder marked trusted" } else { "already trusted" } }),
            Err(e) => json!({ "ok": false, "output": e }),
        },
        "plugins" => {
            let m = provision::ensure_marketplace(&root);
            let p = if m.is_ok() { provision::install_plugin(&root, provision::TELEGRAM_PLUGIN) } else { Err("marketplace failed".into()) };
            json!({ "ok": m.is_ok() && p.is_ok(), "output": format!("marketplace: {}\nplugin: {}", fmt(m), fmt(p)) })
        }
        "install-claude" => match provision::install_claude() {
            Ok(_) => json!({ "ok": true, "output": "Claude Code installed to ~/.local/bin. Next: Sign in." }),
            Err(e) => json!({ "ok": false, "output": e }),
        },
        "install-git" => match provision::install_git_windows() {
            Ok(_) => json!({ "ok": true, "output": "Git for Windows installed (provides bash for the hooks and scripts). Open a new terminal for PATH to update." }),
            Err(e) => json!({ "ok": false, "output": e }),
        },
        "install-tmux" => match provision::install_tmux() {
            Ok(out) => json!({ "ok": true, "output": format!("tmux ready. {}", out.lines().last().unwrap_or("")) }),
            Err(e) => json!({ "ok": false, "output": e }),
        },
        "uninstall" => {
            let report = install::uninstall(target, name == "purge");
            let text = report.steps.iter().map(|st| format!("[{}] {}{}", st.action, st.path, if st.note.is_empty() { String::new() } else { format!(": {}", st.note) })).collect::<Vec<_>>().join("\n");
            json!({ "ok": report.ok, "output": text })
        }
        "sync-telegram" => match provision::sync_telegram_state(&root) {
            Ok(msg) => json!({ "ok": true, "output": msg }),
            Err(e) => json!({ "ok": false, "output": e }),
        },
        "bun" => match provision::install_bun() {
            Ok(out) => json!({ "ok": true, "output": format!("Bun installed.\n{}", out.lines().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")) }),
            Err(e) => json!({ "ok": false, "output": e }),
        },
        _ => json!({ "ok": false, "output": "unknown action" }),
    }
}

fn fmt(r: Result<(bool, String), String>) -> String {
    match r {
        Ok((true, s)) => format!("done ({})", s.lines().last().unwrap_or("").trim()),
        Ok((false, s)) => s,
        Err(e) => format!("error: {e}"),
    }
}

/// Slow actions run in the background; the page polls /api/job.
fn start_job(jobs: &provision::Jobs, target: &str, what: &str, name: &str) -> Value {
    let root = install::normalize(target);
    match what {
        // Plain HTTP/SSE MCP servers (e.g. atlassian from .mcp.json): claude opens the browser itself and
        // waits for the OAuth callback, so this runs until the user finishes the consent screen.
        "authorize" => {
            let name = name.to_string();
            let id = jobs.spawn("authorize", move || match provision::run_claude(Some(&root), &["mcp", "login", &name]) {
                Ok((ok, out)) => (ok, if out.trim().is_empty() { format!("\"{name}\" authorized.") } else { out }),
                Err(e) => (false, e),
            });
            json!({ "ok": true, "job": id })
        }
        "services" => {
            let id = jobs.spawn("services", move || match provision::run_claude(Some(&root), &["mcp", "list"]) {
                Ok((_, out)) => (true, json!({ "servers": provision::parse_mcp_list(&out), "raw": out }).to_string()),
                Err(e) => (false, e),
            });
            json!({ "ok": true, "job": id })
        }
        "brief" => {
            let id = jobs.spawn("brief", move || {
                match provision::run_bash(&root, &root.join(".pa/bin/pa-run.sh"), &["morning-brief"], &[]) {
                    Ok((ok, out)) => {
                        // pa-run.sh writes the message to .pa/runs/<skill>-<stamp>.md; show the newest one.
                        let newest = std::fs::read_dir(root.join(".pa/runs"))
                            .ok()
                            .and_then(|d| {
                                let mut v: Vec<_> = d.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("morning-brief-") && e.path().extension().map(|x| x == "md").unwrap_or(false)).collect();
                                v.sort_by_key(|e| e.file_name());
                                v.pop()
                            })
                            .and_then(|e| std::fs::read_to_string(e.path()).ok())
                            .unwrap_or_default();
                        let text = if newest.trim().is_empty() { out } else { format!("{newest}\n\n[runner]\n{out}") };
                        (ok, text)
                    }
                    Err(e) => (false, e),
                }
            });
            json!({ "ok": true, "job": id })
        }
        _ => json!({ "ok": false, "output": "unknown job" }),
    }
}

struct Args {
    port: u16,
    no_browser: bool,
    target: Option<String>,
    cli_install: Option<install::Options>,
}

fn usage() {
    println!(
        "pa-installer {}\n\nUsage:\n  pa-installer [--target DIR] [--port N] [--no-browser]\n      Launch the web installer (default).\n  pa-installer install DIR [--dry-run] [--no-overwrite] [--no-mcp] [--no-provision]\n                           [--persona ID] [--name NAME] [--signoff TEXT] [--tz ZONE]\n                           [--telegram-token T] [--telegram-chat C] [--telegram-user U]\n      Install from the command line with defaults (all skills, hooks, state, scripts, trust, plugins, schedule).\n  pa-installer status DIR\n      Show what is installed and working in DIR.\n  pa-installer uninstall DIR [--purge]\n      Remove sessions, schedule, autostart, plugin scope, trust and launchers; --purge also deletes .pa and .claude.\n  pa-installer list\n      Show the bundled skills, agents and personas.\n",
        pack::VERSION
    );
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args { port: 0, no_browser: false, target: None, cli_install: None };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    match argv.first().map(|s| s.as_str()) {
        Some("install") => {
            let mut o = install::Options::default();
            i = 1;
            while i < argv.len() {
                let s = argv[i].as_str();
                let mut next = || -> Result<String, String> {
                    i += 1;
                    argv.get(i).cloned().ok_or(format!("{s} needs a value"))
                };
                match s {
                    "--dry-run" => o.dry_run = true,
                    "--no-overwrite" => o.overwrite = false,
                    "--no-mcp" => o.mcp = false,
                    "--no-provision" => {
                        o.auto_prereqs = false;
                        o.auto_trust = false;
                        o.auto_plugins = false;
                        o.auto_bun = false;
                        o.auto_schedule = false;
                        o.autostart = false;
                    }
                    "--no-autostart" => o.autostart = false,
                    "--persona" => o.persona = next()?,
                    "--name" => o.assistant_name = next()?,
                    "--signoff" => o.signoff = next()?,
                    "--tz" => o.timezone = next()?,
                    "--telegram-token" => o.telegram_token = next()?,
                    "--telegram-chat" => o.telegram_chat_id = next()?,
                    "--telegram-user" => o.telegram_user_id = next()?,
                    x if x.starts_with("--") => return Err(format!("unknown flag {x}")),
                    x => {
                        if !o.target.is_empty() {
                            return Err("only one target directory".into());
                        }
                        o.target = x.to_string();
                    }
                }
                i += 1;
            }
            if o.target.is_empty() {
                return Err("install needs a target directory".into());
            }
            a.cli_install = Some(o);
            return Ok(a);
        }
        Some("uninstall") => {
            let dir = argv.get(1).ok_or("uninstall needs a directory")?;
            let purge = argv.iter().any(|a| a == "--purge");
            let report = install::uninstall(dir, purge);
            for st in &report.steps {
                println!("{:<7} {}{}", st.action, st.path, if st.note.is_empty() { String::new() } else { format!("   ({})", st.note) });
            }
            println!("\nuninstalled{} -> {}", if purge { " (purged)" } else { "" }, report.target);
            std::process::exit(if report.ok { 0 } else { 1 });
        }
        Some("status") => {
            let dir = argv.get(1).ok_or("status needs a directory")?;
            println!("{}", serde_json::to_string_pretty(&install::status(dir)).unwrap_or_default());
            std::process::exit(0);
        }
        Some("list") => {
            for s in pack::skills() {
                println!("skill   {:<16} {}", s.name, s.description.chars().take(90).collect::<String>());
            }
            for (f, _) in pack::agents() {
                println!("agent   {}", f);
            }
            for p in personas::PERSONAS {
                println!("persona {:<20} {}", p.id, p.tagline);
            }
            std::process::exit(0);
        }
        _ => {}
    }
    while i < argv.len() {
        match argv[i].as_str() {
            "--port" => {
                i += 1;
                a.port = argv.get(i).and_then(|v| v.parse().ok()).ok_or("--port needs a number")?;
            }
            "--no-browser" => a.no_browser = true,
            "--target" => {
                i += 1;
                a.target = Some(argv.get(i).cloned().ok_or("--target needs a directory")?);
            }
            "-h" | "--help" => {
                usage();
                std::process::exit(0);
            }
            x => return Err(format!("unknown argument {x}")),
        }
        i += 1;
    }
    Ok(a)
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}\n");
            usage();
            std::process::exit(2);
        }
    };

    if let Some(o) = args.cli_install {
        let report = install::run(&o);
        for s in &report.steps {
            println!("{:<7} {}{}", s.action, s.path, if s.note.is_empty() { String::new() } else { format!("   ({})", s.note) });
        }
        println!(
            "\n{} {} -> {}",
            if report.dry_run { "dry run" } else { "installed" },
            report.counts.iter().map(|(k, v)| format!("{v} {k}")).collect::<Vec<_>>().join(", "),
            report.target
        );
        std::process::exit(if report.ok { 0 } else { 1 });
    }

    let server = match Server::http(("127.0.0.1", args.port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot start local server: {e}");
            std::process::exit(1);
        }
    };
    let port = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(args.port);
    let token = random_token();
    let url = format!("http://127.0.0.1:{port}/?t={token}");
    println!("pa-installer {}\n\nOpen the installer in your browser:\n  {url}\n\nPress Ctrl+C to quit.", pack::VERSION);
    if !args.no_browser {
        if provision::open_in_browser(&url).is_err() {
            eprintln!("Could not open a browser automatically. Open the URL above yourself.");
        }
    }
    let jobs = provision::Jobs::new();

    for mut req in server.incoming_requests() {
        let url = req.url().to_string();
        let path = url.split('?').next().unwrap_or("/").to_string();
        let method = req.method().clone();

        // Only answer requests addressed to this loopback server (blocks DNS-rebinding style calls).
        let host_ok = header(&req, "Host")
            .map(|h| h.starts_with("127.0.0.1") || h.starts_with("localhost"))
            .unwrap_or(false);
        if !host_ok {
            let _ = req.respond(json_response(403, &json!({ "error": "bad host" })));
            continue;
        }

        if path == "/" && method == Method::Get {
            let _ = req.respond(html_response(pack::INDEX_HTML));
            continue;
        }
        if path == "/favicon.ico" {
            let _ = req.respond(Response::empty(204));
            continue;
        }
        if !path.starts_with("/api/") {
            let _ = req.respond(json_response(404, &json!({ "error": "not found" })));
            continue;
        }

        let sent = header(&req, "X-PA-Token").or_else(|| query_param(&url, "t"));
        if sent.as_deref() != Some(token.as_str()) {
            let _ = req.respond(json_response(403, &json!({ "error": "bad token: open the URL printed by the installer" })));
            continue;
        }

        let mut body = String::new();
        let _ = req.as_reader().read_to_string(&mut body);
        let body_json: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        let field = |k: &str| body_json.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();

        let mut quit = false;
        let resp = match (method, path.as_str()) {
            (Method::Get, "/api/info") => json_response(200, &info(&args.target)),
            (Method::Get, "/api/browse") => json_response(200, &browse(&query_param(&url, "path").unwrap_or_default())),
            (Method::Get, "/api/inspect") => json_response(200, &install::inspect(&query_param(&url, "path").unwrap_or_default())),
            (Method::Get, "/api/status") => json_response(200, &install::status(&query_param(&url, "target").unwrap_or_default())),
            (Method::Get, "/api/job") => {
                let id = query_param(&url, "id").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
                json_response(200, &jobs.get(id))
            }
            (Method::Post, "/api/mkdir") => {
                let p = field("path");
                if p.trim().is_empty() {
                    json_response(400, &json!({ "ok": false, "error": "no path" }))
                } else {
                    let full = install::normalize(&p);
                    match std::fs::create_dir_all(&full) {
                        Ok(_) => json_response(200, &json!({ "ok": true, "path": full.to_string_lossy() })),
                        Err(e) => json_response(500, &json!({ "ok": false, "error": e.to_string() })),
                    }
                }
            }
            (Method::Post, "/api/telegram") => {
                let token = field("token");
                let v = match field("action").as_str() {
                    "me" => provision::telegram_me(&token),
                    "detect" => provision::telegram_detect(&token),
                    _ => json!({ "ok": false, "error": "unknown action" }),
                };
                json_response(200, &v)
            }
            (Method::Post, "/api/install") => match serde_json::from_value::<install::Options>(body_json.clone()) {
                Ok(o) => {
                    let report = install::run(&o);
                    json_response(200, &serde_json::to_value(&report).unwrap_or(Value::Null))
                }
                Err(e) => json_response(400, &json!({ "ok": false, "error": format!("bad options: {e}") })),
            },
            (Method::Post, "/api/run") => {
                let target = field("target");
                let what = field("what");
                let name = field("name");
                let background = what == "services" || what == "brief" || (what == "authorize" && !name.starts_with("claude.ai "));
                if background {
                    json_response(200, &start_job(&jobs, &target, &what, &name))
                } else {
                    json_response(200, &run_action(&target, &what, &name))
                }
            }
            (Method::Post, "/api/quit") => {
                quit = true;
                json_response(200, &json!({ "ok": true }))
            }
            _ => json_response(404, &json!({ "error": "not found" })),
        };
        let _ = req.respond(resp);
        if quit {
            println!("Installer closed.");
            break;
        }
    }
}
