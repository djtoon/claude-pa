#!/usr/bin/env bash
# Injects assistant context at session start: preferences, memory, today's log, due follow-ups.
# Project-local layout: <project>/.pa (installed by pa-installer).
ROOT="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/../.." && pwd)}"
PA="$ROOT/.pa"
[ -d "$PA" ] || exit 0
TODAY=$(date +%F)
echo "=== PA CONTEXT ($TODAY $(date +%H:%M)) ==="
echo "State dir: .pa/ (project-local)"
if [ -f "$PA/telegram/.env" ] && grep -q '^TELEGRAM_BOT_TOKEN=.\{10,\}' "$PA/telegram/.env" 2>/dev/null; then
  echo "Phone channel: Telegram is configured for this folder. It is only live in sessions started with the channel flag:"
  echo "  ./pa  (or pa.cmd / claude.cmd from cmd.exe on Windows)  =  claude --channels plugin:telegram@claude-plugins-official"
  echo "  If this session was started as plain 'claude', Telegram is OFF here; tell the user to exit and run ./pa (or pa -c to continue)."
  if grep -q '"dmPolicy": *"pairing"' "$PA/telegram/access.json" 2>/dev/null; then
    echo "  Access: still in pairing mode. Fastest fix: the user DMs the bot once, then runs  bash .pa/bin/pa-telegram-chatid.sh  (allowlists them, no code)."
  fi
fi
[ -f "$PA/preferences.md" ] && { echo "--- preferences.md"; cat "$PA/preferences.md"; }
[ -f "$PA/memory.md" ]      && { echo "--- memory.md"; cat "$PA/memory.md"; }
[ -f "$PA/log/$TODAY.md" ]  && { echo "--- today's log"; cat "$PA/log/$TODAY.md"; }
if [ -f "$PA/followups.json" ]; then
  echo "--- follow-ups due or overdue"
  if command -v jq >/dev/null 2>&1; then
    jq -r --arg t "$TODAY" '
      [ .[] | select((.status // "open") == "open" and (.due // "9999") <= $t) ]
      | if length == 0 then "(none)" else .[] | "- [\(.due)] \(.what)  (who: \(.who // "?"))" end
    ' "$PA/followups.json" 2>/dev/null || cat "$PA/followups.json"
  elif command -v python3 >/dev/null 2>&1; then
    python3 - "$PA/followups.json" "$TODAY" <<'PY'
import json,sys
items=json.load(open(sys.argv[1],encoding="utf-8")); today=sys.argv[2]
due=[i for i in items if i.get("status","open")=="open" and i.get("due","9999")<=today]
if not due: print("(none)")
for i in due: print(f"- [{i.get('due')}] {i['what']}  (who: {i.get('who','?')})")
PY
  else
    cat "$PA/followups.json"
  fi
fi
echo "=== END PA CONTEXT ==="
