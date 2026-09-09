@echo off
rem Starts the tray app: an orange dot in the system tray that keeps the phone session (Telegram) running
rem in the background. Right-click it to show the session, stop it, run the morning brief, or quit.
start "" "%~dp0.pa\bin\pa-tray.exe"
