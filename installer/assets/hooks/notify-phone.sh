#!/usr/bin/env bash
# Forwards Claude Code notifications (permission requests, idle prompts) to Telegram so the phone knows the session needs you.
ROOT="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/../.." && pwd)}"
PA="$ROOT/.pa"
INPUT=$(cat)
MSG=""
if command -v jq >/dev/null 2>&1; then
  MSG=$(printf '%s' "$INPUT" | jq -r '.message // .title // empty' 2>/dev/null)
elif command -v python3 >/dev/null 2>&1; then
  MSG=$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("message") or d.get("title") or "")' 2>/dev/null)
fi
[ -n "$MSG" ] || MSG="Claude Code needs you"
[ -f "$PA/bin/notify.sh" ] && bash "$PA/bin/notify.sh" "🔔 $MSG" >/dev/null 2>&1
exit 0
