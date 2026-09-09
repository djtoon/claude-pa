#!/usr/bin/env bash
# Discover your Telegram user/chat id after you send the bot one message.
# Saves TELEGRAM_CHAT_ID to .pa/.env (for notify.sh) and allowlists you in .pa/telegram/access.json (for the channel plugin).
BIN="$(cd "$(dirname "$0")" && pwd)"; PA="$(dirname "$BIN")"
set -a
[ -f "$HOME/.claude/channels/telegram/.env" ] && . "$HOME/.claude/channels/telegram/.env"
[ -f "$PA/telegram/.env" ] && . "$PA/telegram/.env"
[ -f "$PA/.env" ] && . "$PA/.env"
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
# Allowlist the same user for the channel plugin so no pairing code is needed.
mkdir -p "$PA/telegram"
ACC="$PA/telegram/access.json"
if command -v jq >/dev/null 2>&1; then
  [ -f "$ACC" ] || echo '{"dmPolicy":"allowlist","allowFrom":[]}' > "$ACC"
  jq --arg id "$ID" '.dmPolicy="allowlist" | .allowFrom=((.allowFrom // []) + [$id] | unique)' "$ACC" > "$ACC.tmp" && mv "$ACC.tmp" "$ACC"
  echo "Allowlisted user $ID in $ACC"
elif command -v python3 >/dev/null 2>&1; then
  python3 - "$ACC" "$ID" <<'PY'
import json,sys,os
p,uid=sys.argv[1],sys.argv[2]
d=json.load(open(p,encoding="utf-8")) if os.path.exists(p) else {}
d["dmPolicy"]="allowlist"; a=d.get("allowFrom") or []
if uid not in a: a.append(uid)
d["allowFrom"]=a
json.dump(d,open(p,"w",encoding="utf-8"),indent=2)
print(f"Allowlisted user {uid} in {p}")
PY
fi
bash "$BIN/notify.sh" "✅ PA notifications connected."
