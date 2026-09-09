//! pa-tray: keeps the pa-pack phone session (`claude --channels plugin:telegram@…`) running in the
//! background and puts a tray icon up to control it. Lives in `<project>/.pa/bin/`; the project root
//! is two levels up. Windows: the session runs in a hidden console (show = restart visible, resuming
//! the conversation). macOS/Linux: the session runs in a detached tmux session (show = attach in a terminal).

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

const CHANNEL: &str = "plugin:telegram@claude-plugins-official";
const POLL: Duration = Duration::from_secs(3);

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
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
    which("claude").or_else(|| {
        let p = home_dir().join(".local").join("bin").join(if cfg!(windows) { "claude.exe" } else { "claude" });
        p.is_file().then_some(p)
    })
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
        let _ = writeln!(f, "{} {line}", now_stamp());
    }
}

fn now_stamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("[{secs}]")
}

fn session_name(root: &Path) -> String {
    let base = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    format!("pa-{}", base.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '-' }).collect::<String>())
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
}

impl Session {
    fn new(root: PathBuf) -> Self {
        Session { root, child: None, visible: false, last_exit: None }
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
            let _ = &self.last_exit;
            Command::new("tmux").args(["has-session", "-t", &session_name(&self.root)]).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
        }
    }

    fn start(&mut self, visible: bool, resume: bool) -> Result<(), String> {
        if self.running() {
            return Ok(());
        }
        let claude = claude_bin().ok_or("claude CLI not found on PATH (install Claude Code)")?;
        self.visible = visible;
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let mut c = Command::new(&claude);
            c.args(["--channels", CHANNEL, "--permission-mode", "acceptEdits"]);
            if resume {
                c.arg("-c");
            }
            c.current_dir(&self.root).env_remove("CLAUDECODE");
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
            let cmd = format!("'{}' --channels {} --permission-mode acceptEdits{}", claude.to_string_lossy().replace('\'', "'\\''"), CHANNEL, if resume { " -c" } else { "" });
            let st = Command::new("tmux")
                .args(["new-session", "-d", "-s", &name, "-c", &self.root.to_string_lossy(), &cmd])
                .env_remove("CLAUDECODE")
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
}

impl App {
    fn refresh(&mut self) {
        let running = self.session.running();
        let text = if running {
            if cfg!(windows) {
                format!("● assistant running ({})", if self.session.visible { "visible" } else { "hidden" })
            } else {
                "● assistant running (tmux)".to_string()
            }
        } else {
            match self.session.last_exit {
                Some(code) => format!("○ assistant stopped (exit {code})"),
                None => "○ assistant stopped".to_string(),
            }
        };
        self.status.set_text(&text);
        self.start.set_enabled(!running);
        self.stop.set_enabled(running);
        self.hide.set_enabled(running && cfg!(windows) && self.session.visible);
        if let Some(t) = &self.tray {
            let _ = t.set_tooltip(Some(format!("pa assistant: {}", if running { "running" } else { "stopped" })));
        }
    }

    fn report(&self, r: Result<(), String>) {
        if let Err(e) = r {
            log(&self.root, &format!("error: {e}"));
            self.status.set_text(&format!("! {e}"));
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
                    c.arg(script).arg("morning-brief").current_dir(&self.root).env_remove("CLAUDECODE").stdout(Stdio::null()).stderr(Stdio::null());
                    #[cfg(windows)]
                    {
                        use std::os::windows::process::CommandExt;
                        c.creation_flags(0x0800_0000);
                    }
                    let _ = c.spawn();
                    self.status.set_text("● morning brief running in the background…");
                    self.next_poll = Instant::now() + Duration::from_secs(8);
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

fn main() {
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
    };
    let _ = event_loop.run_app(&mut app);
}
