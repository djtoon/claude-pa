#!/usr/bin/env bash
# Push a message to your phone via the same Telegram bot the channel plugin uses.
# Usage: notify.sh "text"   or   echo "text" | notify.sh
PA="${PA_HOME:-$HOME/.pa}"
[ -f "$HOME/.claude/channels/telegram/.env" ] && . "$HOME/.claude/channels/telegram/.env"
[ -f "$PA/.env" ] && . "$PA/.env"
MSG="${1:-$(cat)}"
[ -z "$MSG" ] && exit 0
if [ -z "$TELEGRAM_BOT_TOKEN" ] || [ -z "$TELEGRAM_CHAT_ID" ]; then
  echo "$MSG"; echo "(notify.sh: TELEGRAM_BOT_TOKEN or TELEGRAM_CHAT_ID missing, printed instead)" >&2; exit 0
fi
# Telegram caps messages at 4096 chars; split on that boundary.
printf '%s' "$MSG" | python3 -c '
import sys,os,json,urllib.request,urllib.parse
tok=os.environ["TELEGRAM_BOT_TOKEN"]; chat=os.environ["TELEGRAM_CHAT_ID"]; text=sys.stdin.read()
for i in range(0,len(text),4000):
    data=urllib.parse.urlencode({"chat_id":chat,"text":text[i:i+4000],"disable_web_page_preview":"true"}).encode()
    urllib.request.urlopen(f"https://api.telegram.org/bot{tok}/sendMessage",data=data,timeout=20).read()
'
