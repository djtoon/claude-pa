---
name: followups
description: Track open loops and commitments. Use when the user says "remind me", "follow up", "waiting on", "I owe", "did X ever reply", "what am I waiting for", "מעקב", "תזכיר לי", or any time a commitment or a promise is stated in conversation. Manages ~/.pa/followups.json.
---

# Follow-ups

File: `~/.pa/followups.json` (array). Create `[]` if missing.

Item: `{ "id": "fu-<epoch>", "what": "...", "who": "person or me", "due": "YYYY-MM-DD", "source": "slack|email|chat|calendar", "status": "open", "created": "YYYY-MM-DD" }`

Commands (natural language, no syntax required):
- add: infer `who` and `due`. If no due date is stated, use +3 days for "waiting on someone", +1 day for "I owe". Say the inferred date so the user can correct it.
- list: group as OVERDUE, TODAY, THIS WEEK, LATER. One line each. Count only for LATER if more than 5.
- done / close: mark status done, add `closed: date`, and write one line to the daily log.
- nudge: for items where `who` is not me and the item is overdue, draft a short polite nudge (email or Slack, whichever the source was). Do not send without confirmation.

Every time this skill runs, prune items closed more than 14 days ago into the log and remove them from the file.
