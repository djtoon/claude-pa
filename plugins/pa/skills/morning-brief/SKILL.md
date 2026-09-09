---
name: morning-brief
description: Build the daily morning brief. Use when the user says "morning", "brief me", "what's today", "start my day", "בוקר", "מה יש היום", or when invoked by the scheduled job. Pulls calendar, unread priority email, Slack mentions, Jira assigned, and due follow-ups into one phone-sized message.
---

# Morning brief

Read `~/.pa/preferences.md` and `~/.pa/followups.json` first.

Gather in parallel where possible:
1. gcal: today's events, plus tomorrow's first event. Flag gaps under 15 min and back-to-back blocks over 3 hours.
2. gmail: unread from the last 18 hours. Rank: people in memory.md > direct asks > everything else. Skip newsletters and notifications.
3. slack: mentions and DMs since last brief.
4. atlassian: issues assigned to me due within 7 days, sorted by due.
5. followups.json: due today or overdue.

Write the brief in this shape (omit empty sections):

```
☀️ Wed 9 Sep

📅 Calendar
- 09:30 Standup (30m)
- 14:00 Bank Jerusalem review (1h)  ⚠ no prep block

📬 Needs a reply (3)
- Kalish: contract redline, asked for answer by noon

🔁 Follow-ups due
- Send deck to Proseed (due today)

🎯 Suggested top 3
1. ...
2. ...
3. ...
```

Hard limits: under 25 lines. Suggested top 3 must be concrete, doable today, and reference a real item above. If nothing is urgent, say so in one line instead of inventing urgency.

Append a copy to `~/.pa/log/YYYY-MM-DD.md` under `## Morning brief`.
