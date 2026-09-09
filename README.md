# pa-pack: Claude Code as a personal assistant

Turns a clean Claude Code install into a proactive, phone-reachable assistant connected to Gmail, Google Calendar, Google Drive, Slack and Jira.

## What you get

| Layer | What | How |
|---|---|---|
| Brain | `pa` plugin: 9 skills + 2 subagents + hooks | `/plugin install pa@pa-pack` |
| Services | Gmail, Calendar, Drive, Slack, Atlassian | remote MCP servers in the plugin's `.mcp.json`, OAuth via `/mcp` |
| Phone (chat) | Telegram bot in a persistent tmux session | official `telegram` channel plugin, `claude --channels` |
| Phone (full session) | Claude app, Code tab | `/remote-control` inside the tmux session |
| Proactive | morning brief, radar every 2h, end of day, week ahead | launchd / cron running `claude -p` headless, output pushed to Telegram |
| Memory | `~/.pa/` : preferences, memory, follow-ups, daily logs | plain files, injected at session start by a hook |

## Install (5 minutes of typing, most of it OAuth clicks)

```bash
git clone <this repo> ~/pa-pack   # or unzip
bash ~/pa-pack/install.sh
```
Then follow the "Next steps" it prints. Windows: run inside WSL2.

## Install, option B: the binary installer (project-local, no WSL needed)

Repo: https://github.com/djtoon/claude-pa (release binaries for Windows, macOS arm64/x64 and Linux are built by
GitHub Actions on every tag `v*`; see `.github/workflows/build.yml`).

`installer/` is a Rust program that embeds this whole kit and installs it into **one folder** of your choice
(`<folder>/.claude/` + `<folder>/.pa/`), leaving `~/.claude` alone. It opens a black/orange terminal-style wizard
in your browser: personality, folder, phone (optional), install, done. After the files it provisions everything:
marks the folder trusted, installs the Telegram channel plugin at project scope, installs Bun if missing,
verifies your bot token and allowlists you (no pairing code), installs the proactive schedule, and adds a login
autostart. The Done page is a live status board with one-click fixes and per-service `[ authorize ]` buttons for
the claude.ai connectors.

It also ships **pa-tray** (`pa-tray/`): a tray-icon app that keeps the phone session running in the background
(hidden console on Windows, tmux on macOS/Linux) with start / stop / show / quit in its menu. Launchers land in the
folder: `pa-tray`, `pa` (= `claude --channels …`), and on Windows a `claude.cmd` shim so plain `claude` typed in the
folder gets the flag.

```bash
./build.sh                                # builds pa-tray, then the installer (which embeds it)
installer/target/release/pa-installer     # or pa-installer.exe on Windows
```
See `installer/README.md`.

Note on services: Gmail, Calendar, Drive and Slack are claude.ai connectors (their endpoints refuse dynamic
client registration, so they cannot be raw `.mcp.json` servers). The plugin's `.mcp.json` therefore carries
only Atlassian.

## Skills

`/pa:morning-brief` `/pa:inbox-triage` `/pa:calendar-guard` `/pa:followups` `/pa:radar` `/pa:end-of-day` `/pa:research-brief` `/pa:remember`, plus the `pa` charter skill that governs all of them. You rarely type these; the descriptions are trigger classifiers, so "what's today", "triage", "remind me", "who is X" route on their own. Hebrew triggers included.

## Proactivity model

Three layers, from cheapest to richest:
1. Scheduled headless jobs (`pa-run.sh`). Read-only tool allowlist by default. Drafts land in `~/.pa/inbox/`, never sent on their own. Quiet runs are suppressed.
2. The always-on interactive session (`pa-up.sh`). Two-way chat from Telegram, full control from the Claude app via Remote Control, permission requests pushed to your phone by the `Notification` hook.
3. In-conversation: the charter makes every task end with a check of due follow-ups and auto-captures commitments.

To let jobs act autonomously (send the nudge, create the prep block), widen `ALLOWED` in `~/.pa/bin/pa-run.sh` one tool at a time, and whitelist the action in `preferences.md`.

## Files

```
~/.pa/
  preferences.md   your rules (voice, hours, whitelisted actions)
  memory.md        durable facts
  followups.json   open loops
  log/YYYY-MM-DD.md
  inbox/           drafts and drops from headless runs
  briefs/          research briefs
  schedule.txt     cron-style cadence, rerun pa-schedule.sh after editing
  bin/             pa-run.sh, pa-up.sh, pa-schedule.sh, notify.sh, pa-telegram-chatid.sh
```

## Gotchas

- Channels and Remote Control need a claude.ai login (Pro/Max/Team), not an API key. Channels need Bun.
- Messages sent to the bot while the tmux session is down are lost. Autostart `pa-up.sh` at login if you want true always-on.
- Laptop asleep = no scheduled jobs. Use a Mac mini, a desktop, or a small VPS if this needs to be reliable.
- In-session `/loop` scheduling expires and dies with the session; that is why the pack uses system cron/launchd instead. Claude Desktop scheduled tasks are a fine alternative if you prefer a GUI.
- Headless runs inherit the same OAuth sessions. If a job reports a 401, open the interactive session and `/login` again.
