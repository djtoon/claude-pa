---
name: research-brief
description: Produce a decision-ready research brief on a person, company, tool, or topic. Use when the user says "look into", "research", "prep me on", "who is", "what do we know about", "before my meeting with", "תחקור", or when a calendar event with an external party is coming up and no brief exists in memory.md.
---

# Research brief

Use the `researcher` agent for gathering and the `critic` agent to challenge the draft before you show it.

Sources in order: memory.md and the daily logs (what we already know), gmail and Slack history with the subject, Drive docs mentioning them, then the web.

Output, max 20 lines:
```
🔎 <subject>
Who/what: one sentence.
Why it matters to me: one sentence tied to a real project or meeting.
Last contact: date + one line.
Open threads: bullets.
Talking points: 3 bullets.
Risks / things to not say: 1-2 bullets.
Sources: short list.
```

Save the brief to `~/.pa/briefs/<slug>.md` and add a one-line pointer to memory.md.
