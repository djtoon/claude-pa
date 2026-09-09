@echo off
rem Phone session launcher (Windows). Written by pa-installer; pa-up.sh, the Startup entry and the
rem installer's "Start a console session" button all run this. Keeps the window open if claude exits.
title pa assistant
cd /d "%~dp0..\.."
set "PATH=%USERPROFILE%\.bun\bin;%USERPROFILE%\.local\bin;%PATH%"
echo [pa] starting the assistant with the Telegram channel in %cd%
rem Forget Claude's cached "recent failure" for the Telegram server so this session retries it.
powershell -NoProfile -Command "$p=\"$env:USERPROFILE\.claude\mcp-needs-auth-cache.json\"; if (Test-Path $p) { try { $j = Get-Content $p -Raw | ConvertFrom-Json; if ($j.PSObject.Properties['plugin:telegram:telegram']) { $j.PSObject.Properties.Remove('plugin:telegram:telegram'); $j | ConvertTo-Json -Compress | Set-Content $p } } catch {} }" >nul 2>&1
rem The permission mode chosen in the installer lives in .claude\settings.json. It is passed explicitly because
rem -c (continue) would otherwise restore whatever mode the previous session ran in.
set "MODEFLAG="
findstr /C:"\"defaultMode\": \"bypassPermissions\"" ".claude\settings.json" >nul 2>&1 && set "MODEFLAG=--permission-mode bypassPermissions"
findstr /C:"\"defaultMode\": \"acceptEdits\"" ".claude\settings.json" >nul 2>&1 && set "MODEFLAG=--permission-mode acceptEdits"
"{{CLAUDE}}" -c %MODEFLAG% --channels plugin:telegram@claude-plugins-official %*
echo.
echo [pa] claude exited with code %errorlevel%. Press any key to close this window.
pause >nul
