---
name: inbox-triage
description: Triage email and Slack into reply-now, decide, waiting, and ignore, and draft the replies. Use when the user says "triage", "clear my inbox", "what needs a reply", "draft replies", "מיילים", "תענה", or pastes an email thread. Also used by the radar job for unseen high-priority mail.
---

# Inbox triage

1. Pull unread gmail (last 48h) and Slack DMs/mentions. Drop newsletters, receipts, automated notifications.
2. Classify each thread:
   - REPLY NOW: a person is waiting on the user and a reply takes under 5 minutes of thought.
   - DECIDE: needs the user's judgment. Summarize the decision in one line with the options.
   - WAITING: user already replied or someone else owes an action. Add to followups.json with a due date if there is one.
   - IGNORE: everything else. Do not list these, just count them.
3. For every REPLY NOW, draft the reply in the user's voice (see preferences.md: tone, sign-off, language). Keep drafts under 120 words unless the thread demands more.
4. Present as:

```
📬 Triage: 14 unread, 3 need you, 6 waiting on others, 5 ignored

REPLY NOW
1. Kalish, "contract redline"  →  draft below

DECIDE
- Proseed wants a call Thu or Fri. Options: Thu 15:00 / Fri 10:00. Which?

Drafts
--- 1 ---
<draft>
```

5. Send only after the user says which numbers to send. In headless mode save drafts to `~/.pa/inbox/drafts-YYYY-MM-DD.md` and report counts only.
