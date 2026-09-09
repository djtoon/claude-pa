#!/usr/bin/env bash
# Discover your Telegram user/chat id after you send the bot one message.
# Saves TELEGRAM_CHAT_ID to .pa/.env (for notify.sh) and allowlists you for the channel plugin in BOTH
# .pa/telegram/access.json (project copy) and ~/.claude/channels/telegram/access.json (what the plugin reads).
BIN="$(cd "$(dirname "$0")" && pwd)"; PA="$(dirname "$BIN")"
GLOBAL="${CLAUDE_CONFIG_DIR:-$HOME/.claude}/channels/telegram"
set -a
[ -f "$PA/.env" ] && . "$PA/.env"
[ -f "$GLOBAL/.env" ] && . "$GLOBAL/.env"
[ -f "$PA/telegram/.env" ] && . "$PA/telegram/.env"
set +a
[ -n "${TELEGRAM_BOT_TOKEN:-}" ] || { echo "No bot token. Put TELEGRAM_BOT_TOKEN=... in $PA/telegram/.env (the installer does this) or run /telegram:configure <token>."; exit 1; }
RAW=$(curl -s "https://api.telegram.org/bot$TELEGRAM_BOT_TOKEN/getUpdates")
if command -v jq >/dev/null 2>&1; then
  ID=$(printf '%s' "$RAW" | jq -r '.result | if length==0 then "" else .[-1].message.from.id end' 2>/dev/null)
else
  ID=$(printf '%s' "$RAW" | python3 -c 'import sys,json; u=json.load(sys.stdin)["result"]; print(u[-1]["message"]["from"]["id"] if u else "")' 2>/dev/null)
fi
if [ -z "$ID" ] || [ "$ID" = "null" ]; then echo "No messages yet. Send your bot any message on Telegram, then rerun."; exit 1; fi
touch "$PA/.env"
if grep -q '^TELEGRAM_CHAT_ID=' "$PA/.env"; then
  sed -i.bak "s/^TELEGRAM_CHAT_ID=.*/TELEGRAM_CHAT_ID=$ID/" "$PA/.env" && rm -f "$PA/.env.bak"
else
  echo "TELEGRAM_CHAT_ID=$ID" >> "$PA/.env"
fi
echo "Saved TELEGRAM_CHAT_ID=$ID to $PA/.env"
allowlist() {  # $1 = access.json path
  local ACC="$1"; mkdir -p "$(dirname "$ACC")"
  if command -v jq >/dev/null 2>&1; then
    [ -f "$ACC" ] || echo '{"dmPolicy":"allowlist","allowFrom":[]}' > "$ACC"
    jq --arg id "$ID" '.dmPolicy="allowlist" | .allowFrom=((.allowFrom // []) + [$id] | unique)' "$ACC" > "$ACC.tmp" && mv "$ACC.tmp" "$ACC"
  elif command -v python3 >/dev/null 2>&1; then
    python3 - "$ACC" "$ID" <<'PY'
import json,sys,os
p,uid=sys.argv[1],sys.argv[2]
d=json.load(open(p,encoding="utf-8")) if os.path.exists(p) else {}
d["dmPolicy"]="allowlist"; a=d.get("allowFrom") or []
if uid not in a: a.append(uid)
d["allowFrom"]=a
json.dump(d,open(p,"w",encoding="utf-8"),indent=2)
PY
  fi
  echo "Allowlisted user $ID in $ACC"
}
allowlist "$PA/telegram/access.json"
allowlist "$GLOBAL/access.json"
# The plugin reads its token from the global dir too; keep it in sync with the project copy.
[ -f "$PA/telegram/.env" ] && cp "$PA/telegram/.env" "$GLOBAL/.env"
bash "$BIN/notify.sh" "✅ PA notifications connected."
