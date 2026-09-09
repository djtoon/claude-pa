#!/usr/bin/env bash
# Installs the proactive schedule as launchd jobs (macOS) or crontab lines (Linux/WSL).
set -eu
PA="${PA_HOME:-$HOME/.pa}"
SCHED="$PA/schedule.txt"
RUN="$PA/bin/pa-run.sh"
if [ "$(uname)" = "Darwin" ]; then
  mkdir -p "$HOME/Library/LaunchAgents"
  grep -v '^#' "$SCHED" | grep -v '^\s*$' | while read -r name c1 c2 c3 c4 c5 skill; do
    P="$HOME/Library/LaunchAgents/com.pa.$name.plist"
    launchctl unload "$P" 2>/dev/null || true
    python3 - "$P" "$name" "$c1" "$c2" "$c3" "$c4" "$c5" "$RUN" "$skill" <<'PY'
import sys,plistlib
p,name,mi,ho,dom,mon,dow,run,skill=sys.argv[1:10]
def expand(f,lo,hi):
    if f=='*': return None
    out=[]
    for part in f.split(','):
        step=1
        if '/' in part: part,step=part.split('/'); step=int(step)
        if part=='*': a,b=lo,hi
        elif '-' in part: a,b=map(int,part.split('-'))
        else: a=b=int(part)
        out+=list(range(a,b+1,step))
    return out
cal=[]
for m in (expand(mi,0,59) or [None]):
  for h in (expand(ho,0,23) or [None]):
    for d in (expand(dow,0,6) or [None]):
      e={}
      if m is not None: e['Minute']=m
      if h is not None: e['Hour']=h
      if d is not None: e['Weekday']=d
      cal.append(e)
plistlib.dump({'Label':f'com.pa.{name}','ProgramArguments':['/bin/bash',run]+skill.split(' ',1),
 'StartCalendarInterval':cal,'StandardOutPath':f'/tmp/pa-{name}.log','StandardErrorPath':f'/tmp/pa-{name}.err','RunAtLoad':False},open(p,'wb'))
PY
    launchctl load "$P"; echo "loaded com.pa.$name"
  done
  echo "Tip: keep the Mac awake for jobs: System Settings > Battery/Energy > Prevent sleeping, or run 'caffeinate -i' in tmux."
else
  TMP=$(mktemp); crontab -l 2>/dev/null | grep -v 'pa-run.sh' > "$TMP" || true
  grep -v '^#' "$SCHED" | grep -v '^\s*$' | while read -r name c1 c2 c3 c4 c5 skill; do
    echo "$c1 $c2 $c3 $c4 $c5 /bin/bash $RUN $skill >> /tmp/pa-$name.log 2>&1" >> "$TMP"
  done
  crontab "$TMP"; rm "$TMP"; echo "crontab installed:"; crontab -l | grep pa-run
fi
