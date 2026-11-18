@echo off
setlocal
cd /d "%~dp0"
set "previewUrl=http://127.0.0.1:5173/"

rem Reopen an existing AgenTeam development preview on the same port.
powershell.exe -NoProfile -Command "try { $r = Invoke-WebRequest '%previewUrl%' -UseBasicParsing -TimeoutSec 2; if ($r.Content.Contains('<title>AgenTeam</title>') -and $r.Content.Contains('/@vite/client')) { exit 0 } } catch {} exit 1"
if not errorlevel 1 (
    start "" "%previewUrl%"
    exit /b 0
)

if not exist "node_modules\.bin\vite.cmd" (
    echo Install the project dependencies with npm ci, then run this file again.
    goto :failed
)
node --run predev:web
if errorlevel 1 goto :failed
echo Keep this window open while editing. Press Ctrl+C to stop the preview.
node --run dev:web -- --host 127.0.0.1 --port 5173 --strictPort --open
if not errorlevel 1 exit /b 0

:failed
pause
exit /b 1
