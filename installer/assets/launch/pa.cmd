@echo off
rem Start the assistant with the phone channel. Plain `claude` has no Telegram; this adds the flag every time.
rem Usage: pa            new session
rem        pa -c         continue the last session
rem        pa <args...>  any other claude arguments
cd /d "%~dp0"
"{{CLAUDE}}" --channels plugin:telegram@claude-plugins-official %*
