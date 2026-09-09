# pa-tray

Tray-icon app that keeps the pa-pack phone session running in the background. Installed by
`pa-installer` into `<folder>/.pa/bin/pa-tray[.exe]`, started with `pa-tray` / `pa-tray.cmd` from the
folder or by the login autostart.

- **Windows**: runs `claude --channels plugin:telegram@claude-plugins-official --permission-mode acceptEdits`
  in a hidden console. "Show session window" restarts it visibly with `-c` (the chat continues); "Hide"
  does the reverse.
- **macOS / Linux**: runs the same command in a detached tmux session `pa-<folder>`; "Open session in a
  terminal" attaches to it.

Menu: status line, Start / Stop assistant, Show / Hide, Run the morning brief now, Open assistant folder,
Quit (stops the assistant). Icon: orange disc with a black centre. Log: `.pa/log/tray.log`.

Build: `cargo build --release` (Linux needs `libgtk-3-dev libayatana-appindicator3-dev libxdo-dev`).
The installer's `build.rs` embeds the resulting binary; see `../build.sh`.
