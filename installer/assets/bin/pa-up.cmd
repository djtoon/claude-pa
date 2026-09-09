@echo off
rem Phone session launcher (Windows). Written by pa-installer; pa-up.sh, the Startup entry and the
rem installer's "Start a console session" button all run this. Keeps the window open if claude exits.
title pa assistant
cd /d "%~dp0..\.."
set "PATH=%USERPROFILE%\.bun\bin;%USERPROFILE%\.local\bin;%PATH%"
echo [pa] starting the assistant with the Telegram channel in %cd%
"{{CLAUDE}}" --channels plugin:telegram@claude-plugins-official --permission-mode acceptEdits %*
echo.
echo [pa] claude exited with code %errorlevel%. Press any key to close this window.
pause >nul
