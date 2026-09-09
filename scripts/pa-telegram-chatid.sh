#!/usr/bin/env bash
# Discover your Telegram chat id after you send the bot one message. Saves to ~/.pa/.env for notify.sh.
PA="${PA_HOME:-$HOME/.pa}"
. "$HOME/.claude/channels/telegram/.env" 2>/dev/null || { echo "Run /telegram:configure <token> in Claude Code first."; exit 1; }
ID=$(curl -s "https://api.telegram.org/bot$TELEGRAM_BOT_TOKEN/getUpdates" | python3 -c 'import sys,json; u=json.load(sys.stdin)["result"]; print(u[-1]["message"]["chat"]["id"] if u else "")')
[ -z "$ID" ] && { echo "No messages yet. Send your bot any message on Telegram, then rerun."; exit 1; }
grep -q TELEGRAM_CHAT_ID "$PA/.env" 2>/dev/null && sed -i.bak "s/^TELEGRAM_CHAT_ID=.*/TELEGRAM_CHAT_ID=$ID/" "$PA/.env" || echo "TELEGRAM_CHAT_ID=$ID" >> "$PA/.env"
echo "Saved TELEGRAM_CHAT_ID=$ID to $PA/.env"
"$PA/bin/notify.sh" "✅ PA notifications connected."
