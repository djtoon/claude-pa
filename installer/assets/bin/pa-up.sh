#!/usr/bin/env bash
# Always-on interactive assistant: Telegram channel (chat from phone) + Remote Control (full session in the Claude app).
# macOS/Linux: runs inside tmux.  Windows (Git Bash): opens a dedicated console window.
# Usage: pa-up.sh          start or attach
#        pa-up.sh restart  kill and start fresh
set -u
BIN="$(cd "$(dirname "$0")" && pwd)"; PA="$(dirname "$BIN")"; ROOT="$(dirname "$PA")"
export PATH="$HOME/.local/bin:$HOME/.bun/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
# Forget Claude's cached "recent failure" for the Telegram server so this session retries it.
F="${CLAUDE_CONFIG_DIR:-$HOME/.claude}/mcp-needs-auth-cache.json"
if [ -f "$F" ]; then
  if command -v jq >/dev/null 2>&1; then jq 'del(."plugin:telegram:telegram")' "$F" > "$F.tmp" 2>/dev/null && mv "$F.tmp" "$F"
  elif command -v python3 >/dev/null 2>&1; then python3 -c 'import json,sys; p=sys.argv[1]; d=json.load(open(p)); d.pop("plugin:telegram:telegram",None); json.dump(d,open(p,"w"))' "$F" 2>/dev/null
  fi
fi
SESSION=pa
# The permission mode comes from the folder's .claude/settings.json (set by the installer's Permissions step).
CMD="cd '$ROOT' && claude -c --channels plugin:telegram@claude-plugins-official"
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    # Git Bash rewrites bare /flags into paths, so keep cmd's arguments out of bash: everything lives in pa-up.cmd.
    export MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*'
    [ -f "$BIN/pa-up.cmd" ] || { echo "pa-up.cmd missing: rerun the installer (scripts on)."; exit 1; }
    cmd /c start "pa assistant" "$(cygpath -w "$BIN/pa-up.cmd")"
    echo "Started in a new console window. Inside it, run /remote-control once to expose the session to the Claude app (Code tab)."
    exit 0 ;;
esac
command -v tmux >/dev/null || { echo "tmux not found. Install it (brew install tmux / apt install tmux), or run:"; echo "  $CMD"; exit 1; }
if [ "${1:-}" = "restart" ]; then tmux kill-session -t $SESSION 2>/dev/null || true; fi
if tmux has-session -t $SESSION 2>/dev/null; then
  echo "pa session running. Attach: tmux attach -t $SESSION"
else
  tmux new-session -d -s $SESSION "$CMD"
  echo "Started. Attach with: tmux attach -t $SESSION"
  echo "Inside the session, run /remote-control once to expose it to the Claude app (Code tab)."
fi
