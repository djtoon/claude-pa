#!/usr/bin/env bash
# Installs the proactive schedule from .pa/schedule.txt:
#   macOS   -> launchd jobs (~/Library/LaunchAgents/com.pa.<project>.<name>.plist)
#   Linux   -> crontab lines
#   Windows -> Task Scheduler tasks (folder \pa-<project>\), run through Git Bash
# Lines: "<name> <min> <hour> <dom> <mon> <dow> <skill> [extra prompt]"  or  "<name> @login <script>"
# Usage: pa-schedule.sh            install / refresh
#        pa-schedule.sh remove     remove everything this project installed
#        pa-schedule.sh status     show what is installed
# PA_SCHEDULE_DRY=1 prints what would be done without changing anything.
set -eu
BIN="$(cd "$(dirname "$0")" && pwd)"; PA="$(dirname "$BIN")"; ROOT="$(dirname "$PA")"
SCHED="$PA/schedule.txt"
RUN="$BIN/pa-run.sh"
PROJ="$(basename "$ROOT" | tr -c 'A-Za-z0-9_\n' '-')"
DRY="${PA_SCHEDULE_DRY:-}"
MODE="${1:-install}"
[ -f "$SCHED" ] || { echo "No $SCHED"; exit 1; }

lines() { grep -v '^#' "$SCHED" | grep -v '^[[:space:]]*$'; }
names() { lines | awk '{print $1}'; }

OS="$(uname -s)"
case "$OS" in
Darwin)
  LA="$HOME/Library/LaunchAgents"
  if [ "$MODE" = "status" ]; then ls "$LA" 2>/dev/null | grep "^com.pa.$PROJ\." || echo "(nothing installed)"; exit 0; fi
  if [ "$MODE" = "remove" ]; then
    for P in "$LA"/com.pa."$PROJ".*.plist; do [ -f "$P" ] || continue; [ -n "$DRY" ] && { echo "would remove $P"; continue; }; launchctl unload "$P" 2>/dev/null || true; rm -f "$P"; echo "removed $P"; done
    exit 0
  fi
  mkdir -p "$LA"
  lines | while read -r name c1 c2 c3 c4 c5 skill; do
    P="$LA/com.pa.$PROJ.$name.plist"
    if [ "$c1" = "@login" ]; then
      # Prefer the tray app (menu-bar icon + tmux session); else the bash script.
      if [ "$c2" = "pa-up.sh" ] && [ -x "$BIN/pa-tray" ]; then SCRIPT="$BIN/pa-tray"; DIRECT=1; else SCRIPT="$BIN/$c2"; DIRECT=0; fi
      if [ -n "$DRY" ]; then echo "would write $P (RunAtLoad) -> $SCRIPT"; continue; fi
      launchctl unload "$P" 2>/dev/null || true
      python3 - "$P" "$PROJ.$name" "$SCRIPT" "$DIRECT" <<'PY'
import sys,plistlib
p,name,script,direct=sys.argv[1:5]
args=[script] if direct=="1" else ['/bin/bash',script]
plistlib.dump({'Label':f'com.pa.{name}','ProgramArguments':args,'RunAtLoad':True,
 'StandardOutPath':f'/tmp/pa-{name}.log','StandardErrorPath':f'/tmp/pa-{name}.err'},open(p,'wb'))
PY
      launchctl load "$P"; echo "loaded com.pa.$PROJ.$name (at login)"; continue
    fi
    if [ -n "$DRY" ]; then echo "would write $P and launchctl load it: $c1 $c2 $c3 $c4 $c5 -> pa-run.sh $skill"; continue; fi
    launchctl unload "$P" 2>/dev/null || true
    python3 - "$P" "$PROJ.$name" "$c1" "$c2" "$c3" "$c4" "$c5" "$RUN" "$skill" <<'PY'
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
    launchctl load "$P"; echo "loaded com.pa.$PROJ.$name"
  done
  echo "Tip: keep the Mac awake for jobs: System Settings > Battery/Energy > Prevent sleeping, or run 'caffeinate -i'."
  ;;
MINGW*|MSYS*|CYGWIN*)
  # Windows Task Scheduler. Flags must not be path-converted by MSYS, hence the two env vars.
  export MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*'
  FOLDER="pa-$PROJ"
  # Login autostart: a .cmd in the per-user Startup folder (schtasks ONLOGON needs admin rights).
  STARTUP="$(cygpath -u "$APPDATA")/Microsoft/Windows/Start Menu/Programs/Startup"
  LOGIN_CMD="$STARTUP/pa-$PROJ.cmd"
  if [ "$MODE" = "status" ]; then
    schtasks /Query /FO LIST /TN "$FOLDER"'\' 2>/dev/null | grep -E "TaskName|Next Run Time|Status" || echo "(no scheduled tasks)"
    [ -f "$LOGIN_CMD" ] && echo "login autostart: $LOGIN_CMD" || echo "login autostart: none"
    exit 0
  fi
  if [ "$MODE" = "remove" ]; then
    for name in $(names); do
      TN="$FOLDER"'\'"$name"
      if [ -n "$DRY" ]; then echo "would delete task $TN"; continue; fi
      schtasks /Delete /TN "$TN" /F >/dev/null 2>&1 && echo "deleted $TN" || true
    done
    if [ -f "$LOGIN_CMD" ]; then [ -n "$DRY" ] && echo "would remove $LOGIN_CMD" || { rm -f "$LOGIN_CMD"; echo "removed $LOGIN_CMD"; }; fi
    exit 0
  fi
  BASHW=$(cygpath -w "$(command -v bash)")
  RUNW=$(cygpath -w "$RUN")
  DAYS=(SUN MON TUE WED THU FRI SAT)
  rm -f "$LOGIN_CMD"
  lines | while read -r name c1 c2 c3 c4 c5 skill; do
    TN="$FOLDER"'\'"$name"
    if [ "$c1" = "@login" ]; then
      # Prefer the tray app (hidden session + tray icon); else pa-up.cmd (console); else bash + script.
      if [ "$c2" = "pa-up.sh" ] && [ -f "$BIN/pa-tray.exe" ]; then
        TARGETW=$(cygpath -w "$BIN/pa-tray.exe")
        LINE="start \"\" \"$TARGETW\""
      elif [ "$c2" = "pa-up.sh" ] && [ -f "$BIN/pa-up.cmd" ]; then
        TARGETW=$(cygpath -w "$BIN/pa-up.cmd")
        LINE="start \"pa assistant\" \"$TARGETW\""
      else
        SCRIPTW=$(cygpath -w "$BIN/$c2")
        LINE="start \"\" /min \"$BASHW\" \"$SCRIPTW\""
      fi
      if [ -n "$DRY" ]; then echo "would write $LOGIN_CMD -> $LINE"; continue; fi
      mkdir -p "$STARTUP"
      printf '@echo off\r\n%s\r\n' "$LINE" > "$LOGIN_CMD"
      echo "login autostart: $LOGIN_CMD"
      continue
    fi
    # Supported cron subset: minute N; hour N or A-B[/S]; day-of-week * , list, or A-B. (day-of-month/month ignored.)
    hr="${c2%%/*}"; hr="${hr%%-*}"
    ST=$(printf '%02d:%02d' "$hr" "$c1")
    if [ "$c5" = "*" ]; then dl="SUN,MON,TUE,WED,THU,FRI,SAT"; else
      dl=""
      for part in ${c5//,/ }; do
        if [[ "$part" == *-* ]]; then a=${part%-*}; b=${part#*-}; else a=$part; b=$part; fi
        d=$a; while [ "$d" -le "$b" ]; do dl="$dl,${DAYS[$d]}"; d=$((d+1)); done
      done
      dl=${dl#,}
    fi
    rep=()
    if [[ "$c2" == *-* ]]; then
      range=${c2%%/*}; step=${c2#*/}; [ "$step" = "$c2" ] && step=1
      a=${range%-*}; b=${range#*-}
      DU=$(printf '%02d:00' $((b-a+1)))
      rep=(/RI $((step*60)) /DU "$DU")
    fi
    TR="\"$BASHW\" \"$RUNW\" $skill"
    if [ -n "$DRY" ]; then echo "would run: schtasks /Create /F /SC WEEKLY /D $dl /ST $ST ${rep[*]:-} /TN \"$TN\" /TR '$TR'"; continue; fi
    schtasks /Delete /TN "$TN" /F >/dev/null 2>&1 || true
    schtasks /Create /F /SC WEEKLY /D "$dl" /ST "$ST" "${rep[@]}" /TN "$TN" /TR "$TR" >/dev/null && echo "scheduled $TN ($dl at $ST${rep:+, repeating})"
  done
  echo "Tip: tasks appear in Task Scheduler under the '$FOLDER' folder. They only run while you are logged in and the machine is awake."
  ;;
*)
  TAG="# pa:$ROOT"
  DESKTOP="${XDG_CONFIG_HOME:-$HOME/.config}/autostart/pa-$PROJ.desktop"
  if [ "$MODE" = "status" ]; then
    crontab -l 2>/dev/null | grep "$TAG" || echo "(no cron lines)"
    [ -f "$DESKTOP" ] && echo "login autostart: $DESKTOP" || echo "login autostart: none (tray app not installed or no @login line)"
    exit 0
  fi
  TMP=$(mktemp); crontab -l 2>/dev/null | grep -v "$TAG" > "$TMP" || true
  if [ "$MODE" = "remove" ]; then
    if [ -n "$DRY" ]; then echo "would remove crontab lines tagged $TAG and $DESKTOP"; rm "$TMP"; exit 0; fi
    crontab "$TMP"; rm "$TMP"; rm -f "$DESKTOP"; echo "removed crontab lines tagged $TAG and $DESKTOP"; exit 0
  fi
  rm -f "$DESKTOP"
  lines | while read -r name c1 c2 c3 c4 c5 skill; do
    if [ "$c1" = "@login" ]; then
      # The tray app needs a desktop session (XDG autostart); a plain script goes through cron @reboot.
      if [ "$c2" = "pa-up.sh" ] && [ -x "$BIN/pa-tray" ]; then
        if [ -n "$DRY" ]; then echo "would write $DESKTOP -> $BIN/pa-tray"; continue; fi
        mkdir -p "$(dirname "$DESKTOP")"
        printf '[Desktop Entry]\nType=Application\nName=pa assistant (%s)\nExec=%s\nPath=%s\nX-GNOME-Autostart-enabled=true\n' "$PROJ" "$BIN/pa-tray" "$ROOT" > "$DESKTOP"
        echo "login autostart: $DESKTOP"
      else
        echo "@reboot /bin/bash '$BIN/$c2' >> /tmp/pa-$PROJ-$name.log 2>&1 $TAG" >> "$TMP"
      fi
    else
      echo "$c1 $c2 $c3 $c4 $c5 /bin/bash '$RUN' $skill >> /tmp/pa-$PROJ-$name.log 2>&1 $TAG" >> "$TMP"
    fi
  done
  if [ -n "$DRY" ]; then echo "would install crontab:"; grep "$TAG" "$TMP"; rm "$TMP"; exit 0; fi
  crontab "$TMP"; rm "$TMP"; echo "crontab installed:"; crontab -l | grep "$TAG"
  ;;
esac
