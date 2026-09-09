#!/usr/bin/env bash
# Local build: the tray app first (the installer embeds it), then the installer.
# Result: installer/target/release/pa-installer[.exe]   (self-contained, ships pa-tray inside)
set -eu
cd "$(dirname "$0")"
cargo build --release --manifest-path pa-tray/Cargo.toml
cargo build --release --manifest-path installer/Cargo.toml
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) EXT=.exe ;; *) EXT= ;; esac
echo
echo "installer: installer/target/release/pa-installer$EXT"
echo "tray app:  pa-tray/target/release/pa-tray$EXT   (also embedded in the installer)"
