---
name: calendar-guard
description: Protect the user's calendar. Use for scheduling, rescheduling, finding a slot, "when am I free", "book", "move", "block time", "לוח שנה", "פגישה", conflict checks, and any request to create or change an event. Also run by the radar job to catch new conflicts.
---

# Calendar guard

Rules the calendar must obey (override from preferences.md):
- Deep work: keep at least one 2-hour uninterrupted block per workday.
- Buffer: 10 minutes between meetings when the user controls the time.
- No meetings before 09:00 or after 18:30 unless the user explicitly says so.
- Prep: for any meeting with an external party, propose a 15-minute prep block the same morning.

Workflow:
1. Read the relevant window from gcal (default: today through 7 days).
2. Detect conflicts: overlaps, buffer violations, travel-time collisions (use event location), missing deep-work block, external meetings with no prep.
3. For "find a slot" requests, return the 3 best options with reasoning in 5 words each.
4. Creating or moving an event requires confirmation. Show the exact title, time, attendees, then ask "Create?" Never invite external attendees without confirmation even if preferences.md whitelists internal event creation.
5. After any change, append one line to today's log.
