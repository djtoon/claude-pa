@echo off
rem Windows shim: cmd.exe looks in the current folder before PATH, so `claude` typed inside this folder
rem gets the Telegram channel flag automatically. Subcommands (claude mcp ..., claude plugin ...) pass through
rem untouched. PowerShell does not run commands from the current folder; use .\pa there.
setlocal
set "first=%~1"
if "%first%"=="" goto withchannel
if "%first:~0,1%"=="-" goto withchannel
"{{CLAUDE}}" %*
exit /b %errorlevel%
:withchannel
"{{CLAUDE}}" --channels plugin:telegram@claude-plugins-official %*
exit /b %errorlevel%
