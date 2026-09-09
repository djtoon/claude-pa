# pa-installer

A single-binary installer for pa-pack. It embeds the whole kit (skills, agents, hooks, state
templates, runtime scripts, the `pa-tray` app) and writes it into **one project folder**:
`<folder>/.claude/` and `<folder>/.pa/`. Then it provisions everything around it so the assistant
works right away: workspace trust, the Telegram channel plugin (project scope), Bun, the proactive
schedule, and a login autostart for the phone session (via the tray app).

## Run

```
pa-installer                        # opens the wizard in your browser (127.0.0.1, random port)
pa-installer --target C:\assistant  # prefill the folder
pa-installer --port 47821 --no-browser
pa-installer install DIR [--dry-run] [--persona fun] [--name Nova] [--signoff Dan] [--tz Asia/Jerusalem]
                         [--telegram-token T --telegram-chat C --telegram-user U] [--no-provision] [--no-autostart]
pa-installer status DIR             # JSON: what is installed and working
pa-installer uninstall DIR [--purge] # stop sessions, remove schedule + autostart, plugin scope, trust, plugin-dir
                                    # Telegram state and launchers; --purge also deletes .pa/, .claude/, the CLAUDE.md block
pa-installer list                   # bundled skills, agents, personalities
```

## The wizard (black / orange / white, terminal style)

1. **Personality** — Serious, Fun, Weird, Warm, Sarcastic, Zen, Hype, or custom text (tone only; the
   operating rules are the same for all); assistant name; your sign-off.
2. **Folder** — built-in directory browser, creates folders, shows whether it is an upgrade.
3. **Phone (optional)** — paste the BotFather token → *Verify*; the wizard then watches for your first
   message to the bot and allowlists you (no pairing code). *Skip phone* is always available.
   Everything else (timezone, workdays, hours, skills, wiring, automation toggles) sits under
   **Advanced**, collapsed, with defaults that are fine.
4. **Ready to install** — a plain-language summary (what is written, what happens automatically);
   the full file list is one click away.
5. **Done** — a live status board (files, CLI, signed in, trusted, plugin, Bun, token, access, chat
   id, tray app, launcher, schedule) with one-click fixes; a services board from `claude mcp list`
   with an **[ authorize ]** button per service; buttons to start the tray app, start a console
   session, run the morning brief, test the push, test the hook, open the folder.

## Launching

The Telegram channel only attaches to sessions started with `--channels plugin:telegram@…`;
Claude Code has no settings key for it. So the installer drops these into the project root:

| File | What it does |
|---|---|
| `pa-tray` / `pa-tray.cmd` | Starts the **tray app**: an orange dot in the tray / menu bar that keeps the phone session running in the background. Menu: start / stop, show the session (Windows: restarts it visibly, keeping the chat; macOS/Linux: attaches the tmux session in a terminal), run the morning brief, open the folder, quit. |
| `pa` / `pa.cmd` | `claude --channels …` in a terminal, in the folder. `pa -c` continues the last session. |
| `claude.cmd` (Windows) | cmd.exe looks in the current folder before PATH, so plain `claude` typed inside the folder gets the flag too. Subcommands pass through. PowerShell never runs commands from the current folder: use `.\pa` there. |

The login autostart runs the tray app (Startup folder on Windows, RunAtLoad agent on macOS, XDG
autostart on Linux). The SessionStart hook tells the assistant that the channel is only live in
`pa` / tray sessions, so a plain `claude` session answers "run ./pa" instead of guessing.

The bot answers only while a session is running. Scheduled pushes do not need one.

**Where the plugin reads its state.** Claude Code does not pass `settings.json` `env` to MCP server
processes, so the Telegram plugin ignores `TELEGRAM_STATE_DIR` and reads `~/.claude/channels/telegram/`.
The installer therefore mirrors the token and allowlist from `.pa/telegram/` into that directory (and
`Detect me now` / `pa-telegram-chatid.sh` keep both in sync). The project copy stays the source of truth.

**Bun must be the real runtime.** Claude spawns `bun` as a plain process; the npm package's `bun` / `bun.cmd`
shims cannot be spawned that way and the server dies with "Connection closed". The installer only accepts a
real `bun.exe` / `bun` binary (official installer, `~/.bun/bin`) and every launcher puts `~/.bun/bin` and
`~/.local/bin` first on PATH.

## Clean machine

With "Install what is missing" on (default), the installer brings the prerequisites itself, using the official
installers only: Claude Code (`claude.ai/install`), Git for Windows via winget (bash for hooks and scripts),
the Bun runtime (`bun.sh`), tmux on macOS (Homebrew) / Linux (apt, dnf or pacman when passwordless sudo works,
otherwise it prints the command). The Done board shows each one with an install button, plus Sign in.

## Services

Gmail, Google Calendar, Google Drive and Slack are **claude.ai connectors** (claude.ai → Settings →
Connectors). Their endpoints refuse dynamic client registration, so they cannot be plain
`.mcp.json` entries; they appear in Claude Code once you are signed in with claude.ai. Each needs a
one-time authorization: the Done page's **[ authorize ]** runs `claude mcp login "<name>"
--no-browser`, opens the claude.ai approval page it returns, and the connector is live from the
next session. Plain MCP servers (Atlassian in `.mcp.json`) go through Claude's own OAuth in the
browser instead.

## What it writes

```
<folder>/
  CLAUDE.md                 pa block between <!-- pa-pack --> markers, includes the personality
  .mcp.json                 atlassian (merged, existing keys kept)
  pa  pa.cmd  claude.cmd  pa-tray  pa-tray.cmd      launchers (see Launching)
  .claude/
    settings.json           hooks, permissions for .pa/**, enableAllProjectMcpServers,
                            env.TELEGRAM_STATE_DIR -> .pa/telegram, enabledPlugins (written by claude plugin install)
    skills/<name>/SKILL.md  the 9 pa skills, rewritten for the local layout (.pa/, /skill)
    agents/{researcher,critic}.md
    hooks/{session-start,notify-phone}.sh
  .pa/
    preferences.md          generated from your answers, never overwritten afterwards
    memory.md  followups.json  schedule.txt  .env (TELEGRAM_CHAT_ID)  .gitignore
    telegram/.env           TELEGRAM_BOT_TOKEN for the channel plugin
    telegram/access.json    dmPolicy allowlist + your user id
    log/ inbox/ briefs/ runs/
    bin/pa-tray[.exe]       the tray app
    bin/pa-up.cmd           Windows console launcher (used by pa-up.sh)
    bin/{pa-run,pa-schedule,pa-up,notify,pa-telegram-chatid}.sh
```

Outside the folder: `~/.claude.json` gets `projects["<folder>"].hasTrustDialogAccepted` (what the
trust dialog would write), the official plugin marketplace is registered once, scheduled jobs live
in Task Scheduler / launchd / crontab under a per-project name, and the login autostart entry.
`pa-schedule.sh remove` deletes the jobs and the autostart.

Re-running is safe: state files are kept, pack-managed files are updated (or skipped if "overwrite"
is off), JSON files are merged, the CLAUDE.md block is replaced in place, provisioning steps report
`same`. Quit the tray app before reinstalling on Windows (its exe is locked while running).

## Build

```
./build.sh                     # pa-tray first, then the installer (which embeds it)
```

or by hand: `cargo build --release --manifest-path pa-tray/Cargo.toml`, then
`cargo build --release --manifest-path installer/Cargo.toml`. `installer/build.rs` looks for the tray
binary in `pa-tray/target/<triple>/release/` or `$PA_TRAY_BIN`; without it the installer still
builds, minus the tray app. The GitHub workflow (`.github/workflows/build.yml`) does this for
Windows x64, macOS arm64 + x64 and Linux x64 and attaches the binaries to tagged releases (`v*`).

Linux builds of the tray need `libgtk-3-dev libayatana-appindicator3-dev libxdo-dev`.

## Security

The server binds to 127.0.0.1 only, every `/api/*` call needs the random token from the URL the
installer prints, and requests with a foreign `Host` header are refused. The bot token is sent only
to `api.telegram.org` (via curl) and written to `.pa/telegram/.env`, which `.pa/.gitignore` excludes.
