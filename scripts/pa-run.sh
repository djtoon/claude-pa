#!/usr/bin/env bash
# Headless proactive run. Usage: pa-run.sh <skill> [extra prompt]
# Runs one pa skill non-interactively, logs the output, and pushes it to your phone.
set -u
PA="${PA_HOME:-$HOME/.pa}"
SKILL="${1:?skill name, e.g. morning-brief}"; shift || true
EXTRA="${*:-}"
export PATH="$HOME/.local/bin:$HOME/.bun/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
mkdir -p "$PA/log" "$PA/inbox" "$PA/runs"
LOCK="$PA/runs/.lock-$SKILL"
if [ -e "$LOCK" ] && kill -0 "$(cat "$LOCK" 2>/dev/null)" 2>/dev/null; then exit 0; fi
echo $$ > "$LOCK"; trap 'rm -f "$LOCK"' EXIT

STAMP=$(date +%F-%H%M)
OUT="$PA/runs/$SKILL-$STAMP.md"
PROMPT="You are running headless as a scheduled job. Run the /pa:$SKILL skill now. $EXTRA Output only the final message for the user, nothing else."

# Read-mostly tool allowlist: MCP reads, local state, no sends. Widen deliberately if you want autonomous actions.
ALLOWED='Read,Glob,Grep,Write(~/.pa/**),Edit(~/.pa/**),Bash(date:*),Bash(python3:*),WebSearch,WebFetch,mcp__gmail__*,mcp__gcal__*,mcp__gdrive__*,mcp__slack__*,mcp__atlassian__*'

cd "$PA"
claude -p "$PROMPT" \
  --output-format text \
  --allowedTools "$ALLOWED" \
  --max-turns 40 \
  > "$OUT" 2>"$OUT.err" || { echo "pa-run: $SKILL failed, see $OUT.err" | "$PA/bin/notify.sh"; exit 1; }

RESULT=$(cat "$OUT")
case "$RESULT" in
  *RADAR_QUIET*|"") exit 0 ;;
esac
printf '%s' "$RESULT" | "$PA/bin/notify.sh"
