#!/usr/bin/env bash
# Push a message to you: Telegram when the bot is set up, otherwise a desktop notification.
# Usage: notify.sh "text"   or   echo "text" | notify.sh
# Env:   PA_NOTIFY_DESKTOP=1  also show a desktop notification when Telegram is used.
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

desktop() {  # best-effort desktop notification, first ~400 chars
  local short; short="$(printf '%s' "$1" | head -c 400)"
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*)
      PA_MSG="$short" powershell -NoProfile -Command '
        try {
          [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
          [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null
          $xml = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02)
          $t = $xml.GetElementsByTagName("text")
          $t.Item(0).AppendChild($xml.CreateTextNode("pa assistant")) | Out-Null
          $t.Item(1).AppendChild($xml.CreateTextNode($env:PA_MSG)) | Out-Null
          $toast = [Windows.UI.Notifications.ToastNotification]::new($xml)
          $app = "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe"
          [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($app).Show($toast)
        } catch { Write-Output $env:PA_MSG }' >/dev/null 2>&1 ;;
    Darwin)
      osascript -e "display notification \"$(printf '%s' "$short" | sed 's/"/\\"/g')\" with title \"pa assistant\"" >/dev/null 2>&1 ;;
    *)
      command -v notify-send >/dev/null 2>&1 && notify-send "pa assistant" "$short" >/dev/null 2>&1 ;;
  esac
}

if [ -z "${TELEGRAM_BOT_TOKEN:-}" ] || [ -z "${TELEGRAM_CHAT_ID:-}" ]; then
  echo "$MSG"
  desktop "$MSG"
  echo "(notify.sh: Telegram not set up, shown as a desktop notification instead)" >&2
  exit 0
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
[ "${PA_NOTIFY_DESKTOP:-}" = "1" ] && desktop "$MSG"
exit 0
