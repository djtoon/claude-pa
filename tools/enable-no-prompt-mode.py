#!/usr/bin/env python3
"""Opt-in: make the phone session run with NO permission prompts at all, confined to its folder.

What this changes in the source (run it once, then ./build.sh):
  * installer: the folder's .claude/settings.json gets permissions.defaultMode = "bypassPermissions",
    an allow list for the connectors (Gmail, Calendar, Drive, Slack, Atlassian) and a deny list that keeps
    the assistant out of ~/.ssh, ~/.aws, ~/.gnupg, ~/.claude* and away from destructive shell commands;
    provisioning also records Claude Code's bypass acknowledgement in ~/.claude.json (a hidden session
    cannot answer that dialog).
  * pa-tray / pa-up: the phone session starts with --permission-mode bypassPermissions.

Without this script the phone session runs in acceptEdits mode: the Telegram reply tools and read-only
file access are pre-approved, everything else asks (the Telegram plugin relays the question to your phone).
The pa charter still confirms every send/post/create with you in chat either way.
"""
import os, sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
inst = os.path.join(root, "installer")


def rw(path, fn):
    with open(path, encoding="utf-8") as f:
        s = f.read()
    s2 = fn(s)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(s2)
    print("patched", os.path.relpath(path, root))


def patch_install(s):
    a = 'const PERMISSIONS: &[&str] = &["Read(./.pa/**)", "Edit(./.pa/**)", "Write(./.pa/**)", "mcp__plugin_telegram_telegram", "mcp__plugin_telegram_telegram__reply", "mcp__plugin_telegram_telegram__react", "mcp__plugin_telegram_telegram__edit_message"];'
    if a not in s:
        print("install.rs already patched or changed; skipping permissions block")
        return s
    s = s.replace(a, '''// No-prompt mode (tools/enable-no-prompt-mode.py): everything is allowed inside the folder, the deny list keeps
// the assistant out of credentials and away from destructive commands. External actions (send, post, create)
// are still confirmed in chat by the pa charter.
const PERMISSIONS: &[&str] = &[
    "Read(./**)", "Edit(./**)", "Write(./**)", "Glob", "Grep", "WebSearch", "WebFetch",
    "mcp__plugin_telegram_telegram", "mcp__claude_ai_Gmail", "mcp__claude_ai_Google_Calendar", "mcp__claude_ai_Google_Drive",
    "mcp__claude_ai_Slack", "mcp__claude_ai_Atlassian_Rovo", "mcp__atlassian",
];
const DENIED: &[&str] = &[
    "Read(~/.ssh/**)", "Read(~/.aws/**)", "Read(~/.gnupg/**)", "Read(~/.claude.json)", "Read(~/.claude/**)",
    "Edit(~/.ssh/**)", "Edit(~/.aws/**)", "Edit(~/.claude.json)", "Edit(~/.claude/**)",
    "Bash(rm -rf:*)", "Bash(format:*)", "Bash(diskpart:*)",
];''', 1)
    b = '''    if added > 0 {
        notes.push(format!("allowed {added} .pa/ rules"));
    }
'''
    assert b in s
    s = s.replace(b, '''    if added > 0 {
        notes.push(format!("allowed {added} rules"));
    }
    let deny = perms.entry("deny").or_insert_with(|| json!([]));
    let deny = deny.as_array_mut().ok_or("settings.permissions.deny is not an array")?;
    let mut denied = 0;
    for rule in DENIED {
        if !deny.iter().any(|v| v.as_str() == Some(rule)) {
            deny.push(Value::String(rule.to_string()));
            denied += 1;
        }
    }
    if denied > 0 {
        notes.push(format!("denied {denied} sensitive paths"));
    }
    if perms.get("defaultMode").and_then(|v| v.as_str()) != Some("bypassPermissions") {
        perms.insert("defaultMode".into(), json!("bypassPermissions"));
        notes.push("no permission prompts in this folder".into());
    }
''', 1)
    c = '''            match provision::set_trust(root) {
                Ok(true) => f.label("workspace trust", "ok", "folder marked trusted in ~/.claude.json"),
                Ok(false) => f.label("workspace trust", "same", "already trusted"),
                Err(e) => f.label("workspace trust", "error", e),
            }'''
    assert c in s
    s = s.replace(c, c + '''
            match provision::accept_bypass_mode() {
                Ok(true) => f.label("no-prompt mode", "ok", "bypass-permissions acknowledgement recorded"),
                Ok(false) => f.label("no-prompt mode", "same", "already acknowledged"),
                Err(e) => f.label("no-prompt mode", "error", e),
            }''', 1)
    return s


def swap_mode(s):
    return s.replace('"--permission-mode", "acceptEdits"', '"--permission-mode", "bypassPermissions"').replace("--permission-mode acceptEdits", "--permission-mode bypassPermissions")


rw(os.path.join(inst, "src", "install.rs"), patch_install)
rw(os.path.join(root, "pa-tray", "src", "main.rs"), swap_mode)
rw(os.path.join(inst, "assets", "bin", "pa-up.cmd"), swap_mode)
rw(os.path.join(inst, "assets", "bin", "pa-up.sh"), swap_mode)
print("\nNow build: ./build.sh   (then reinstall into your folder)")
