---
name: radar
description: Proactive scan run every couple of hours by the scheduler. Use when invoked as /pa:radar or when the user asks "anything new", "anything I missed", "what changed", "מה חדש". Looks only at what changed since the last run and reports only if something needs the user.
---

# Radar (proactive scan)

State: `~/.pa/radar-state.json` `{ "last_run": ISO8601 }`. If missing, look back 3 hours.

Check, since last_run:
1. gmail: new mail from people in memory.md, or with words like urgent, today, deadline, ASAP, invoice, contract, signed, דחוף, היום.
2. gcal: events added, moved, or cancelled; any new conflict per calendar-guard rules; any meeting starting within 45 minutes with an external party and no prep.
3. slack: direct mentions and DMs containing a question.
4. atlassian: issues newly assigned or moved to blocked.
5. followups.json: anything that became due since last_run.

Decision rule: if nothing scores as "needs the user in the next 3 hours", output exactly `RADAR_QUIET` and nothing else. The runner suppresses that message.

Otherwise output one phone message, max 8 lines, most urgent first, each line: emoji, source, what, what you suggest. Example:
```
📡 14:10
📬 Kalish asks for the redline by 16:00. Draft ready in inbox/.
📅 New meeting 15:30 with Bank Jerusalem, no prep block. Add 15:10 prep?
🔁 Follow-up due: Proseed deck.
```

Always update radar-state.json.last_run at the end, even on RADAR_QUIET.
