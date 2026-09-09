#!/usr/bin/env bash
# One-shot installer for pa-pack on a clean machine (macOS, Linux, or Windows via WSL).
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
PA="${PA_HOME:-$HOME/.pa}"
say(){ printf '\n\033[1m%s\033[0m\n' "$*"; }

say "1/6 Prerequisites"
command -v node  >/dev/null || { echo "Node.js 18+ is required. Install it, then rerun."; exit 1; }
command -v claude >/dev/null || { echo "Installing Claude Code"; npm install -g @anthropic-ai/claude-code; }
command -v bun   >/dev/null || { echo "Installing Bun (needed by channel plugins)"; curl -fsSL https://bun.sh/install | bash; export PATH="$HOME/.bun/bin:$PATH"; }
command -v tmux  >/dev/null || echo "tmux not found. Install it (brew install tmux / apt install tmux) for the always-on session."
command -v python3 >/dev/null || { echo "python3 is required."; exit 1; }

say "2/6 Assistant state in $PA"
mkdir -p "$PA/bin" "$PA/log" "$PA/inbox" "$PA/briefs" "$PA/runs"
for f in preferences.md memory.md followups.json; do [ -f "$PA/$f" ] || cp "$HERE/templates/state/$f" "$PA/$f"; done
cp "$HERE/scripts/"*.sh "$PA/bin/"; chmod +x "$PA/bin/"*.sh
[ -f "$PA/schedule.txt" ] || cp "$HERE/scripts/schedule.txt" "$PA/schedule.txt"
[ -f "$PA/CLAUDE.md" ] || cp "$HERE/templates/CLAUDE.md" "$PA/CLAUDE.md"
touch "$PA/.env"

say "3/6 Global CLAUDE.md"
mkdir -p "$HOME/.claude"
if ! grep -q "pa-pack" "$HOME/.claude/CLAUDE.md" 2>/dev/null; then
  { echo; echo "# pa-pack"; cat "$HERE/templates/CLAUDE.md"; } >> "$HOME/.claude/CLAUDE.md"
fi

say "4/6 Plugins"
claude plugin marketplace add "$HERE" 2>/dev/null || true
claude plugin install pa@pa-pack --scope user 2>/dev/null || echo "If that failed, inside Claude Code run: /plugin marketplace add $HERE   then   /plugin install pa@pa-pack"
claude plugin marketplace add anthropics/claude-plugins-official 2>/dev/null || true
claude plugin install telegram@claude-plugins-official --scope user 2>/dev/null || echo "Inside Claude Code run: /plugin install telegram@claude-plugins-official"

say "5/6 Proactive schedule"
bash "$PA/bin/pa-schedule.sh" || echo "Schedule install skipped; rerun ~/.pa/bin/pa-schedule.sh later."

say "6/6 Next steps (manual, one time)"
cat <<TXT
  a) cd $PA && claude
     /login               (claude.ai account; channels and Remote Control need it)
     /mcp                 authenticate gmail, gcal, gdrive, slack, atlassian
     /telegram:configure <token from @BotFather>
     exit
  b) $PA/bin/pa-up.sh    starts the always-on session (tmux) with the Telegram channel
     DM your bot on Telegram, paste the pairing code in the session, then run:
     /telegram:access allow-only-me   (or the equivalent shown by the plugin)
     /remote-control                   exposes this session in the Claude app > Code tab
  c) $PA/bin/pa-telegram-chatid.sh    saves your chat id so scheduled jobs can push to your phone
  d) Test: $PA/bin/pa-run.sh morning-brief
Edit $PA/preferences.md to teach it your rules. Edit $PA/schedule.txt then rerun pa-schedule.sh to change cadence.
TXT
