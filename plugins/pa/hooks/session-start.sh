#!/usr/bin/env bash
# Injects assistant context at session start: preferences, memory, today's log, due follow-ups.
PA="${PA_HOME:-$HOME/.pa}"
[ -d "$PA" ] || exit 0
TODAY=$(date +%F)
echo "=== PA CONTEXT ($TODAY $(date +%H:%M)) ==="
[ -f "$PA/preferences.md" ] && { echo "--- preferences.md"; cat "$PA/preferences.md"; }
[ -f "$PA/memory.md" ]      && { echo "--- memory.md"; cat "$PA/memory.md"; }
[ -f "$PA/log/$TODAY.md" ]  && { echo "--- today's log"; cat "$PA/log/$TODAY.md"; }
if [ -f "$PA/followups.json" ] && command -v python3 >/dev/null; then
  echo "--- follow-ups due or overdue"
  python3 - "$PA/followups.json" "$TODAY" <<'PY'
import json,sys
items=json.load(open(sys.argv[1])); today=sys.argv[2]
due=[i for i in items if i.get("status","open")=="open" and i.get("due","9999")<=today]
if not due: print("(none)")
for i in due: print(f"- [{i.get('due')}] {i['what']}  (who: {i.get('who','?')})")
PY
fi
echo "=== END PA CONTEXT ==="
