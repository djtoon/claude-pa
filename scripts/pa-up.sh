#!/usr/bin/env bash
# Always-on interactive assistant in tmux: Telegram channel (chat from phone) + Remote Control (full session in the Claude app).
# Usage: pa-up.sh          start or attach
#        pa-up.sh restart  kill and start fresh
set -u
PA="${PA_HOME:-$HOME/.pa}"
export PATH="$HOME/.local/bin:$HOME/.bun/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
SESSION=pa
CMD="cd '$PA' && claude --channels plugin:telegram@claude-plugins-official --permission-mode acceptEdits"
if [ "${1:-}" = "restart" ]; then tmux kill-session -t $SESSION 2>/dev/null || true; fi
if tmux has-session -t $SESSION 2>/dev/null; then
  echo "pa session running. Attach: tmux attach -t $SESSION"
else
  tmux new-session -d -s $SESSION "$CMD"
  echo "Started. Attach with: tmux attach -t $SESSION"
  echo "Inside the session, run /remote-control once to expose it to the Claude app (Code tab)."
fi
