---
name: end-of-day
description: Close the day. Use when the user says "end of day", "wrap up", "סיכום יום", "what did I do today", or when run by the evening scheduled job. Summarizes the day's log, carries open items forward, and previews tomorrow.
---

# End of day

1. Read `~/.pa/log/YYYY-MM-DD.md`, followups.json, and tomorrow's calendar.
2. Write to the log under `## Wrap`: done today (from log + closed follow-ups), still open, decisions made.
3. Anything promised today with no follow-up entry: create one now.
4. Output, under 15 lines:

```
🌙 Wrap, Wed 9 Sep
✅ Done: 3 (contract redline sent, Proseed deck, standup notes)
⏳ Open: 2 (Bank Jerusalem prep, invoice to Kalish)
📅 Tomorrow: first meeting 10:00, 4 meetings, deep-work block 13:00-15:00 free
💡 One thing: reply to the Idomoo design thread before 10:00
```

In headless mode, keep it to the message plus the log write. No questions.
