@echo off
rem Phone session launcher (Windows). Written by pa-installer; pa-up.sh, the Startup entry and the
rem installer's "Start a console session" button all run this. Keeps the window open if claude exits.
title pa assistant
cd /d "%~dp0..\.."
set "PATH=%USERPROFILE%\.bun\bin;%USERPROFILE%\.local\bin;%PATH%"
echo [pa] starting the assistant with the Telegram channel in %cd%
rem Forget Claude's cached "recent failure" for the Telegram server so this session retries it.
powershell -NoProfile -Command "$p=\"$env:USERPROFILE\.claude\mcp-needs-auth-cache.json\"; if (Test-Path $p) { try { $j = Get-Content $p -Raw | ConvertFrom-Json; if ($j.PSObject.Properties['plugin:telegram:telegram']) { $j.PSObject.Properties.Remove('plugin:telegram:telegram'); $j | ConvertTo-Json -Compress | Set-Content $p } } catch {} }" >nul 2>&1
"{{CLAUDE}}" -c --channels plugin:telegram@claude-plugins-official --permission-mode acceptEdits %*
echo.
echo [pa] claude exited with code %errorlevel%. Press any key to close this window.
pause >nul
