#!/usr/bin/env bash
# Push a message to your phone via the same Telegram bot the channel plugin uses.
# Usage: notify.sh "text"   or   echo "text" | notify.sh
# Token: .pa/telegram/.env (project) wins, then ~/.claude/channels/telegram/.env (the plugin's own dir).
# TELEGRAM_CHAT_ID: .pa/.env (written by the installer or pa-telegram-chatid.sh).
BIN="$(cd "$(dirname "$0")" && pwd)"; PA="$(dirname "$BIN")"
set -a
[ -f "$PA/.env" ] && . "$PA/.env"
[ -f "$HOME/.claude/channels/telegram/.env" ] && . "$HOME/.claude/channels/telegram/.env"
[ -f "$PA/telegram/.env" ] && . "$PA/telegram/.env"
set +a
if [ $# -gt 0 ]; then MSG="$*"; else MSG="$(cat)"; fi
[ -z "$MSG" ] && exit 0
if [ -z "${TELEGRAM_BOT_TOKEN:-}" ] || [ -z "${TELEGRAM_CHAT_ID:-}" ]; then
  echo "$MSG"; echo "(notify.sh: TELEGRAM_BOT_TOKEN or TELEGRAM_CHAT_ID missing, printed instead)" >&2; exit 0
fi
# Telegram caps messages at 4096 chars; split on that boundary.
i=0; n=${#MSG}
while [ "$i" -lt "$n" ]; do
  curl -sS -o /dev/null -X POST "https://api.telegram.org/bot$TELEGRAM_BOT_TOKEN/sendMessage" \
    --data-urlencode "chat_id=$TELEGRAM_CHAT_ID" \
    --data-urlencode "text=${MSG:$i:4000}" \
    --data-urlencode "disable_web_page_preview=true"
  i=$((i+4000))
done
