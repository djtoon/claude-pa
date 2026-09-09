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

## The wizard (black / orange / white, terminal style, with a waving robot)

1. **Personality** — Serious, Fun, Weird, Warm, Sarcastic, Zen, Hype, or custom text (tone only; the
   operating rules are the same for all); assistant name; your sign-off.
2. **Folder** — built-in directory browser, creates folders, shows whether it is an upgrade.
3. **Permissions** — allow everything (default) or ask; allowed folders, never-touched paths, folder guard, OS sandbox.
4. **Phone (optional)** — paste the BotFather token → *Verify*; the wizard then watches for your first
   message to the bot and allowlists you (no pairing code). *Skip phone* is always available.
   Everything else (timezone, workdays, hours, skills, wiring, automation toggles) sits under
   **Advanced**, collapsed, with defaults that are fine.
5. **Ready to install** — a plain-language summary (what is written, what happens automatically);
   the full file list is one click away.
6. **Done** — a live status board (files, CLI, signed in, trusted, plugin, Bun, token, access, chat
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

## Permissions and the folder fence (step 3)

The wizard asks how the assistant may act, with **Allow everything** as the default: no permission prompts
in the folder (`permissions.defaultMode: bypassPermissions`, connectors allowed), because a background session
that answers from your phone cannot stop to ask. **Ask me first** keeps prompts (the Telegram plugin relays
them to your phone). Either way three things fence it in:

- **Allowed folders**: the install folder plus any you add (Documents, a drive root such as the C: drive to allow
  everything, ...). They become `permissions.additionalDirectories` and `.pa/allowed-folders.txt`.
- **Never touched**: `~/.ssh`, `~/.aws`, `~/.gnupg`, `~/.claude.json`, `~/.claude`, gcloud/kube/docker configs,
  plus your own entries. They become Read/Edit deny rules (deny wins in every mode, bypass included), the
  destructive shell commands (`rm -rf`, `format`, `diskpart`, `mkfs`, `dd`) are denied too, and the list is
  written to `.pa/protected-folders.txt`.
- **Folder guard**: a PreToolUse hook, `pa-tray guard`, runs before every Read / Edit / Write / Glob / Grep / LS /
  Bash call and blocks (exit 2, reason shown to Claude) anything that targets a protected path or a path outside
  the allowed folders. Works on every OS and in every permission mode; the Bash check is by literal paths in the
  command (absolute, `~/`, MSYS `/c/...`, and `..` climbs). Edit the two `.pa/*.txt` lists and restart.
- **OS sandbox** (macOS seatbelt, Linux/WSL bubblewrap): enabled by the installer where it exists, confining shell
  commands' writes to the allowed folders and network to a few domains. Windows has no OS sandbox in Claude Code;
  there the folder guard is the fence. The assistant's own rules still make it confirm sends and posts in chat.

## Proactive loop

Step 3 of the wizard has **Proactive updates**: check every off / 15 min / 30 min / 1 h / 2 h / 4 h, plus
"also when nothing is urgent". The installer turns that into the `radar` line of `.pa/schedule.txt`
(`*/30 9-17 * * 0,1,2,3,4 radar` or `7 9-17/2 …`), bounded by your day start/end and workdays, and
installs it with the morning brief, end-of-day and week-ahead jobs (Task Scheduler / launchd / cron).
Each run is a headless `claude -p` in the folder: it reads `.pa/` (preferences, memory, follow-ups,
last-run stamp), checks mail, calendar, Slack and Jira for what changed, and pushes one message through
`notify.sh`: Telegram when the bot is set up, otherwise a desktop notification (Windows toast, macOS
notification, `notify-send`). Quiet runs are suppressed unless "also when nothing is urgent" is on.
`pa-installer install DIR --loop 30 --loop-always` from the CLI; "Run the radar now" on the Done page.

## Reliability (a bot you talk to once a day)

- The tray app is a watchdog: it clears Claude Code's 15-minute "recent failure" cache for the Telegram server
  before every start (`~/.claude/mcp-needs-auth-cache.json`), watches the Telegram MCP log for
  "Channel notifications registered", and restarts the session with backoff (10s → 5 min) if the channel did
  not come up or the session exited. The `pa` / `pa-up` launchers clear that cache too.
- Every launcher and the tray start with `-c`, so closing and reopening continues the same conversation.
- One tray per folder and one phone session per machine: a second pa-tray exits at once (it cannot take the
  folder's loopback lock), the tray refuses to start its session while another `claude --channels` runs, and
  `pa` / `pa.cmd` warn and ask before starting a second session.
- The Telegram reply / react / edit tools are pre-approved in `.claude/settings.json`; a hidden session must
  never wait on a permission prompt for its own replies. Other tools ask, and the Telegram plugin relays the
  question to your phone (approve with the code it sends).
- Messages sent while the assistant is down wait in Telegram's queue (24 h) and are delivered on the next start.
- No prompts is the default (Permissions step); the bypass acknowledgement is recorded in ~/.claude.json so the
  hidden session never hangs on that dialog. The pa charter still confirms sends/posts in chat.

## Personality in every message

The CLAUDE.md block says the personality applies to every message (Telegram replies included) and each
personality carries concrete habits (openers, sign-offs, emoji rules). The session hook repeats it at start.

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
