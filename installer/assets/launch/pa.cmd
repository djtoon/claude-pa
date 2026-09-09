@echo off
rem Start the assistant with the phone channel. Plain `claude` has no Telegram; this adds the flag every time.
rem Usage: pa            continue the last conversation (a fresh one starts when there is none)
rem        pa <args...>  any other claude arguments
cd /d "%~dp0"
rem Bun (Telegram server) and Claude live in the user profile; make sure they are on PATH even in a fresh terminal.
set "PATH=%USERPROFILE%\.bun\bin;%USERPROFILE%\.local\bin;%PATH%"
rem One phone session per machine: a second one fights the first over the Telegram bot.
set "RUNNING=0"
for /f %%c in ('powershell -NoProfile -Command "(Get-CimInstance Win32_Process -Filter \"Name='claude.exe'\" | Where-Object { $_.CommandLine -like '*--channels plugin:telegram*' } | Measure-Object).Count"') do set "RUNNING=%%c"
if not "%RUNNING%"=="0" (
  echo [pa] A phone session is already running ^(the tray app, or another pa window^).
  echo [pa] Two sessions fight over the Telegram bot. Use the tray icon's "Show session window" instead.
  choice /c YN /n /m "[pa] Start a second session anyway? [Y/N] "
  if errorlevel 2 exit /b 0
)
rem Forget Claude's cached "recent failure" for the Telegram server so this session retries it.
powershell -NoProfile -Command "$p=\"$env:USERPROFILE\.claude\mcp-needs-auth-cache.json\"; if (Test-Path $p) { try { $j = Get-Content $p -Raw | ConvertFrom-Json; if ($j.PSObject.Properties['plugin:telegram:telegram']) { $j.PSObject.Properties.Remove('plugin:telegram:telegram'); $j | ConvertTo-Json -Compress | Set-Content $p } } catch {} }" >nul 2>&1
rem -c continues your last conversation in this folder (a fresh one starts when there is none).
"{{CLAUDE}}" -c --channels plugin:telegram@claude-plugins-official %*
