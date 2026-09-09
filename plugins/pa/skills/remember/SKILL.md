---
name: remember
description: Persist facts, preferences and corrections to the assistant's memory. Use when the user says "remember", "note that", "from now on", "always", "never", "my X is", "תזכור", or corrects the assistant's behavior. Writes to ~/.pa/memory.md or ~/.pa/preferences.md.
---

# Remember

- Behavior instructions ("always", "never", "from now on", tone, format, whitelisted actions) go to `~/.pa/preferences.md`.
- Facts about people, projects, accounts, routines go to `~/.pa/memory.md` under the matching heading (People, Projects, Routines, Accounts, Misc). Create headings as needed.
- One line per fact, dated: `- (2026-09-09) Kalish prefers WhatsApp over email for urgent things.`
- If the new fact contradicts an existing line, replace the old line and note "(updated)".
- Never store passwords, card numbers, IDs. If the user tries, refuse in one line and suggest a password manager.
- Confirm in one short line what you saved and where.
