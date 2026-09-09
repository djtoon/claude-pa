//! pa-tray: keeps the pa-pack phone session (`claude --channels plugin:telegram@…`) running in the
//! background and puts a tray icon up to control it. Lives in `<project>/.pa/bin/`; the project root
//! is two levels up. Windows: the session runs in a hidden console (show = restart visible, resuming
//! the conversation). macOS/Linux: the session runs in a detached tmux session (show = attach in a terminal).
//!
//! Reliability: before every start the 15-minute "recent failure" entry Claude Code keeps for the Telegram
//! server is cleared; after a start the Telegram MCP log is watched and the session is restarted (with
//! backoff) if the channel did not come up; a session that exits on its own is restarted the same way.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

const CHANNEL: &str = "plugin:telegram@claude-plugins-official";
const MCP_SERVER: &str = "plugin:telegram:telegram";
const POLL: Duration = Duration::from_secs(3);
/// How long after a start we wait for "Channel notifications registered" before calling it a failure.
const CHANNEL_GRACE: Duration = Duration::from_secs(90);
const BACKOFF: [u64; 5] = [10, 30, 60, 180, 300];

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

fn claude_config_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| home_dir().join(".claude"))
}

fn which(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
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

fn claude_bin() -> Option<PathBuf> {
    let local = home_dir().join(".local").join("bin").join(if cfg!(windows) { "claude.exe" } else { "claude" });
    if local.is_file() {
        return Some(local);
    }
    which("claude")
}

/// PATH for the session: the real Bun (Telegram server) and Claude live in the user profile and may not be on
/// the login shell's PATH yet (fresh install, autostart at login).
fn session_path() -> std::ffi::OsString {
    let home = home_dir();
    let mut dirs = vec![home.join(".bun").join("bin"), home.join(".local").join("bin")];
    if !cfg!(windows) {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
    }
    if let Some(cur) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&cur));
    }
    std::env::join_paths(dirs).unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
}

/// `<root>/.pa/bin/pa-tray` → `<root>`; falls back to the current directory (or its parent chain) that has `.pa`.
fn root_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe.parent().and_then(|b| b.parent()).and_then(|pa| pa.parent()) {
            if root.join(".pa").is_dir() {
                return root.to_path_buf();
            }
        }
    }
    let mut d = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    loop {
        if d.join(".pa").is_dir() {
            return d;
        }
        if !d.pop() {
            break;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn log(root: &Path, line: &str) {
    use std::io::Write;
    let _ = std::fs::create_dir_all(root.join(".pa").join("log"));
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(root.join(".pa").join("log").join("tray.log")) {
        let secs = SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = writeln!(f, "[{secs}] {line}");
    }
}

fn session_name(root: &Path) -> String {
    let base = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    format!("pa-{}", base.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '-' }).collect::<String>())
}

/// Drop Claude Code's "recent failure, skip for 15 minutes" entry for the Telegram server (no serde: the file is
/// a flat JSON object and we only need to delete one key).
fn clear_failure_cache() -> bool {
    let p = claude_config_dir().join("mcp-needs-auth-cache.json");
    let Ok(text) = std::fs::read_to_string(&p) else { return false };
    let key = format!("\"{MCP_SERVER}\"");
    let Some(start) = text.find(&key) else { return false };
    // value is an object: find its closing brace
    let Some(obj_open) = text[start..].find('{') else { return false };
    let mut depth = 0;
    let mut end = None;
    for (i, ch) in text[start + obj_open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + obj_open + i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(end) = end else { return false };
    // remove "key": {...} plus a neighbouring comma
    let mut out = String::new();
    let before = text[..start].trim_end();
    let after = text[end..].trim_start();
    if before.ends_with(',') && after.starts_with('}') {
        out.push_str(before.trim_end_matches(','));
        out.push_str(after);
    } else if after.starts_with(',') {
        out.push_str(before);
        out.push_str(&after[1..]);
    } else {
        out.push_str(before);
        out.push_str(after);
    }
    std::fs::write(&p, out).is_ok()
}

/// Claude Code's per-project MCP log directory for the Telegram server (Windows: %LOCALAPPDATA%\claude-cli-nodejs\Cache\<key>).
fn mcp_log_dir(root: &Path) -> Option<PathBuf> {
    let key: String = root.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let mut bases = Vec::new();
    if let Some(l) = std::env::var_os("LOCALAPPDATA") {
        bases.push(PathBuf::from(l).join("claude-cli-nodejs").join("Cache"));
    }
    bases.push(home_dir().join(".cache").join("claude-cli-nodejs"));
    bases.push(home_dir().join("Library").join("Caches").join("claude-cli-nodejs"));
    bases.push(claude_config_dir().join("debug"));
    for b in bases {
        let d = b.join(&key).join("mcp-logs-plugin-telegram-telegram");
        if d.is_dir() {
            return Some(d);
        }
    }
    None
}

#[derive(PartialEq, Clone, Copy)]
enum ChannelState {
    Unknown,
    Up,
    Failed,
}

/// Look at the newest Telegram MCP log written after `since`.
fn channel_state(root: &Path, since: SystemTime) -> ChannelState {
    let Some(dir) = mcp_log_dir(root) else { return ChannelState::Unknown };
    let Ok(rd) = std::fs::read_dir(&dir) else { return ChannelState::Unknown };
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    for e in rd.flatten() {
        let Ok(m) = e.metadata() else { continue };
        let Ok(t) = m.modified() else { continue };
        if t >= since && newest.as_ref().map(|(nt, _)| t > *nt).unwrap_or(true) {
            newest = Some((t, e.path()));
        }
    }
    let Some((_, p)) = newest else { return ChannelState::Unknown };
    let Ok(text) = std::fs::read_to_string(&p) else { return ChannelState::Unknown };
    if text.contains("Channel notifications registered") {
        ChannelState::Up
    } else if text.contains("Connection failed") || text.contains("Skipping connection") || text.contains("CONNECTION_CLOSED") {
        ChannelState::Failed
    } else {
        ChannelState::Unknown
    }
}

fn open_terminal(dir: &Path, cmd: &str) -> Result<(), String> {
    let d = dir.to_string_lossy().to_string();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
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
        for (bin, args) in [
            ("x-terminal-emulator", vec!["-e", "bash", "-lc", script.as_str()]),
            ("gnome-terminal", vec!["--", "bash", "-lc", script.as_str()]),
            ("konsole", vec!["-e", "bash", "-lc", script.as_str()]),
            ("xterm", vec!["-e", "bash", "-lc", script.as_str()]),
        ] {
            if which(bin).is_some() {
                return Command::new(bin).args(&args).spawn().map(|_| ()).map_err(|e| e.to_string());
            }
        }
        return Err("no terminal emulator found".into());
    }
    #[allow(unreachable_code)]
    Err("unsupported platform".into())
}

fn open_folder(path: &Path) {
    let p = path.to_string_lossy().to_string();
    let _ = if cfg!(windows) {
        Command::new("explorer").arg(&p).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(&p).spawn()
    } else {
        Command::new("xdg-open").arg(&p).spawn()
    };
}

/// The phone session itself.
struct Session {
    root: PathBuf,
    child: Option<Child>,
    visible: bool,
    last_exit: Option<i32>,
    started_at: Option<SystemTime>,
    started_mono: Option<Instant>,
    channel: ChannelState,
    /// Watchdog: consecutive failures and when the next automatic restart is due.
    failures: usize,
    retry_at: Option<Instant>,
    wanted: bool,
}

impl Session {
    fn new(root: PathBuf) -> Self {
        Session { root, child: None, visible: false, last_exit: None, started_at: None, started_mono: None, channel: ChannelState::Unknown, failures: 0, retry_at: None, wanted: false }
    }

    fn running(&mut self) -> bool {
        #[cfg(windows)]
        {
            if let Some(c) = self.child.as_mut() {
                match c.try_wait() {
                    Ok(None) => return true,
                    Ok(Some(st)) => {
                        self.last_exit = st.code();
                        self.child = None;
                    }
                    Err(_) => self.child = None,
                }
            }
            false
        }
        #[cfg(not(windows))]
        {
            Command::new("tmux").args(["has-session", "-t", &session_name(&self.root)]).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
        }
    }

    fn start(&mut self, visible: bool, resume: bool) -> Result<(), String> {
        self.wanted = true;
        if self.running() {
            return Ok(());
        }
        let claude = claude_bin().ok_or("claude CLI not found (install Claude Code)")?;
        if clear_failure_cache() {
            log(&self.root, "cleared Claude's cached Telegram failure");
        }
        self.visible = visible;
        self.channel = ChannelState::Unknown;
        self.started_at = Some(SystemTime::now());
        self.started_mono = Some(Instant::now());
        self.retry_at = None;
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let mut c = Command::new(&claude);
            // -c continues the previous conversation (a fresh one starts when there is none). The permission mode
            // comes from the folder's .claude/settings.json (permissions.defaultMode), set by the installer.
            let _ = resume;
            c.args(["--channels", CHANNEL, "-c"]);
            c.current_dir(&self.root).env_remove("CLAUDECODE").env("PATH", session_path());
            c.creation_flags(if visible { CREATE_NEW_CONSOLE } else { CREATE_NO_WINDOW });
            let child = c.spawn().map_err(|e| format!("cannot start claude: {e}"))?;
            log(&self.root, &format!("started claude pid {} ({})", child.id(), if visible { "visible" } else { "hidden" }));
            self.child = Some(child);
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = visible;
            let name = session_name(&self.root);
            let _ = resume;
            let cmd = format!("'{}' --channels {} -c", claude.to_string_lossy().replace('\'', "'\\''"), CHANNEL);
            let st = Command::new("tmux")
                .args(["new-session", "-d", "-s", &name, "-c", &self.root.to_string_lossy(), &cmd])
                .env_remove("CLAUDECODE")
                .env("PATH", session_path())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|e| format!("cannot run tmux (install it: brew install tmux / apt install tmux): {e}"))?;
            if !st.success() {
                return Err("tmux could not create the session".into());
            }
            log(&self.root, &format!("started tmux session {name}"));
            Ok(())
        }
    }

    fn stop(&mut self) {
        self.wanted = false;
        self.retry_at = None;
        #[cfg(windows)]
        {
            if let Some(mut c) = self.child.take() {
                let _ = c.kill();
                let _ = c.wait();
                log(&self.root, "stopped claude");
            }
        }
        #[cfg(not(windows))]
        {
            let _ = Command::new("tmux").args(["kill-session", "-t", &session_name(&self.root)]).stdout(Stdio::null()).stderr(Stdio::null()).status();
            log(&self.root, "stopped tmux session");
        }
    }

    /// Windows: restart with a visible console, resuming the conversation. Unix: attach in a terminal.
    fn show(&mut self) -> Result<(), String> {
        #[cfg(windows)]
        {
            let was_running = self.running();
            self.stop();
            self.start(true, was_running)
        }
        #[cfg(not(windows))]
        {
            if !self.running() {
                self.start(false, false)?;
            }
            open_terminal(&self.root, &format!("tmux attach -t {}", session_name(&self.root)))
        }
    }

    fn hide(&mut self) -> Result<(), String> {
        #[cfg(windows)]
        {
            let was_running = self.running();
            self.stop();
            self.start(false, was_running)
        }
        #[cfg(not(windows))]
        {
            Ok(())
        }
    }

    /// Watchdog tick: restart a session that died or whose Telegram channel never came up.
    fn watchdog(&mut self) -> Option<String> {
        if !self.wanted {
            return None;
        }
        let running = self.running();
        let now = Instant::now();
        if running {
            if let (Some(at), Some(mono)) = (self.started_at, self.started_mono) {
                if self.channel != ChannelState::Up {
                    let st = channel_state(&self.root, at);
                    if st == ChannelState::Up {
                        self.channel = ChannelState::Up;
                        self.failures = 0;
                        log(&self.root, "telegram channel is up");
                    } else if st == ChannelState::Failed || (st == ChannelState::Unknown && now.duration_since(mono) > CHANNEL_GRACE && mcp_log_dir(&self.root).is_some()) {
                        self.channel = ChannelState::Failed;
                        self.failures += 1;
                        let wait = BACKOFF[(self.failures - 1).min(BACKOFF.len() - 1)];
                        log(&self.root, &format!("telegram channel did not come up (attempt {}), restarting in {wait}s", self.failures));
                        let resume = true;
                        let visible = self.visible;
                        self.stop();
                        self.wanted = true;
                        self.retry_at = Some(now + Duration::from_secs(wait));
                        let _ = (resume, visible);
                        return Some(format!("channel failed, retry in {wait}s"));
                    }
                }
            }
            return None;
        }
        // not running but wanted: schedule / perform a restart
        match self.retry_at {
            Some(t) if now < t => None,
            _ => {
                if self.retry_at.is_none() {
                    self.failures += 1;
                    let wait = BACKOFF[(self.failures - 1).min(BACKOFF.len() - 1)];
                    log(&self.root, &format!("session exited (code {:?}), restarting in {wait}s", self.last_exit));
                    self.retry_at = Some(now + Duration::from_secs(wait));
                    return Some(format!("session exited, retry in {wait}s"));
                }
                let visible = self.visible;
                match self.start(visible, true) {
                    Ok(_) => Some("restarted".into()),
                    Err(e) => {
                        self.retry_at = Some(now + Duration::from_secs(60));
                        Some(format!("restart failed: {e}"))
                    }
                }
            }
        }
    }
}

fn make_icon() -> Icon {
    // 32x32 orange disc with a black centre dot; black-orange-white like the installer.
    let (w, h) = (32u32, 32u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let (cx, cy, r) = (15.5f32, 15.5f32, 14.5f32);
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            let i = ((y * w + x) * 4) as usize;
            if d <= r {
                let inner = d <= 4.5;
                let (cr, cg, cb) = if inner { (0, 0, 0) } else { (0xff, 0x8b, 0x3d) };
                let alpha = if d > r - 1.0 { ((r - d).max(0.0) * 255.0) as u8 } else { 255 };
                rgba[i] = cr;
                rgba[i + 1] = cg;
                rgba[i + 2] = cb;
                rgba[i + 3] = alpha;
            }
        }
    }
    Icon::from_rgba(rgba, w, h).expect("icon")
}

enum UserEvent {
    Menu(MenuEvent),
    Tray(TrayIconEvent),
}

struct App {
    root: PathBuf,
    session: Session,
    tray: Option<TrayIcon>,
    menu: Menu,
    status: MenuItem,
    start: MenuItem,
    stop: MenuItem,
    show: MenuItem,
    hide: MenuItem,
    brief: MenuItem,
    folder: MenuItem,
    quit: MenuItem,
    next_poll: Instant,
    note: Option<(String, Instant)>,
}

impl App {
    fn refresh(&mut self) {
        let running = self.session.running();
        let mut text = if running {
            let ch = match self.session.channel {
                ChannelState::Up => "Telegram up",
                ChannelState::Failed => "Telegram failed",
                ChannelState::Unknown => "starting Telegram…",
            };
            if cfg!(windows) {
                format!("● assistant running ({}, {})", if self.session.visible { "visible" } else { "hidden" }, ch)
            } else {
                format!("● assistant running (tmux, {ch})")
            }
        } else if self.session.wanted {
            "○ assistant restarting…".to_string()
        } else {
            match self.session.last_exit {
                Some(code) => format!("○ assistant stopped (exit {code})"),
                None => "○ assistant stopped".to_string(),
            }
        };
        if let Some((n, at)) = &self.note {
            if at.elapsed() < Duration::from_secs(20) {
                text = format!("{text} · {n}");
            } else {
                self.note = None;
            }
        }
        self.status.set_text(&text);
        self.start.set_enabled(!running && !self.session.wanted);
        self.stop.set_enabled(running || self.session.wanted);
        self.hide.set_enabled(running && cfg!(windows) && self.session.visible);
        if let Some(t) = &self.tray {
            let _ = t.set_tooltip(Some(format!("pa assistant: {}", if running { "running" } else { "stopped" })));
        }
    }

    fn report(&mut self, r: Result<(), String>) {
        if let Err(e) = r {
            log(&self.root, &format!("error: {e}"));
            self.note = Some((format!("! {e}"), Instant::now()));
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::Init = cause {
            // The tray icon must be created once the event loop runs (macOS requirement).
            let tray = TrayIconBuilder::new()
                .with_menu(Box::new(self.menu.clone()))
                .with_tooltip("pa assistant")
                .with_icon(make_icon())
                .build();
            match tray {
                Ok(t) => self.tray = Some(t),
                Err(e) => log(&self.root, &format!("tray error: {e}")),
            }
            let r = self.session.start(false, false);
            self.report(r);
            self.refresh();
        }
        if Instant::now() >= self.next_poll {
            if let Some(n) = self.session.watchdog() {
                self.note = Some((n, Instant::now()));
            }
            self.refresh();
            self.next_poll = Instant::now() + POLL;
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_poll));
    }

    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        if let UserEvent::Menu(e) = event {
            let id = e.id();
            if id == self.start.id() {
                self.session.failures = 0;
                let r = self.session.start(false, false);
                self.report(r);
            } else if id == self.stop.id() {
                self.session.stop();
            } else if id == self.show.id() {
                let r = self.session.show();
                self.report(r);
            } else if id == self.hide.id() {
                let r = self.session.hide();
                self.report(r);
            } else if id == self.brief.id() {
                let script = self.root.join(".pa").join("bin").join("pa-run.sh");
                let bash = if cfg!(windows) { which("bash").filter(|p| !p.to_string_lossy().to_lowercase().contains("system32")).or_else(|| Some(PathBuf::from(r"C:\Program Files\Git\bin\bash.exe"))) } else { Some(PathBuf::from("bash")) };
                if let Some(bash) = bash {
                    let mut c = Command::new(bash);
                    c.arg(script).arg("morning-brief").current_dir(&self.root).env_remove("CLAUDECODE").env("PATH", session_path()).stdout(Stdio::null()).stderr(Stdio::null());
                    #[cfg(windows)]
                    {
                        use std::os::windows::process::CommandExt;
                        c.creation_flags(0x0800_0000);
                    }
                    let _ = c.spawn();
                    self.note = Some(("morning brief running in the background…".into(), Instant::now()));
                }
            } else if id == self.folder.id() {
                open_folder(&self.root);
            } else if id == self.quit.id() {
                self.session.stop();
                event_loop.exit();
            }
            self.refresh();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_poll));
    }
}

mod guard;

fn main() {
    // `pa-tray guard` is the PreToolUse folder-guard hook (stdin JSON in, exit 2 to block). No tray, no loop.
    if std::env::args().nth(1).as_deref() == Some("guard") {
        let root = std::env::var_os("CLAUDE_PROJECT_DIR").map(PathBuf::from).unwrap_or_else(root_dir);
        std::process::exit(guard::run(&root));
    }
    let root = root_dir();
    let event_loop = EventLoop::<UserEvent>::with_user_event().build().expect("event loop");
    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = proxy.send_event(UserEvent::Menu(e));
    }));
    let proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |e| {
        let _ = proxy.send_event(UserEvent::Tray(e));
    }));

    let menu = Menu::new();
    let status = MenuItem::new("○ assistant stopped", false, None);
    let start = MenuItem::new("Start assistant", true, None);
    let stop = MenuItem::new("Stop assistant", false, None);
    let show = MenuItem::new(if cfg!(windows) { "Show session window (restarts, keeps the chat)" } else { "Open session in a terminal" }, true, None);
    let hide = MenuItem::new("Hide session window (restarts, keeps the chat)", false, None);
    let brief = MenuItem::new("Run the morning brief now", true, None);
    let folder = MenuItem::new("Open assistant folder", true, None);
    let quit = MenuItem::new("Quit (stops the assistant)", true, None);
    let title = MenuItem::new(format!("pa assistant: {}", root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()), false, None);
    let _ = menu.append_items(&[&title, &status, &PredefinedMenuItem::separator(), &start, &stop, &show]);
    if cfg!(windows) {
        let _ = menu.append(&hide);
    }
    let _ = menu.append_items(&[&PredefinedMenuItem::separator(), &brief, &folder, &PredefinedMenuItem::separator(), &quit]);

    let mut app = App {
        session: Session::new(root.clone()),
        root,
        tray: None,
        menu,
        status,
        start,
        stop,
        show,
        hide,
        brief,
        folder,
        quit,
        next_poll: Instant::now() + POLL,
        note: None,
    };
    let _ = event_loop.run_app(&mut app);
}
