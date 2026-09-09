#!/usr/bin/env bash
# Forwards Claude Code notifications (permission requests, idle prompts) to Telegram so the phone knows the session needs you.
PA="${PA_HOME:-$HOME/.pa}"
INPUT=$(cat)
MSG=$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("message") or d.get("title") or "Claude Code needs you")' 2>/dev/null)
[ -x "$PA/bin/notify.sh" ] && "$PA/bin/notify.sh" "🔔 $MSG" >/dev/null 2>&1
exit 0
