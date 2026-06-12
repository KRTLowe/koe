@echo off
set "ROOT=%USERPROFILE%\Desktop\kaya-beam"

if exist "%ROOT%\client\package.json" (
    cd /d "%ROOT%\client"
) else if exist "%ROOT%\package.json" (
    cd /d "%ROOT%"
) else (
    echo Cannot find kaya-beam client package.json under "%ROOT%".
    exit /b 1
)

call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
set PATH=%USERPROFILE%\.cargo\bin;%PATH%

echo === Installing npm dependencies ===
call npm install
if %ERRORLEVEL% neq 0 (
    echo npm install failed, exiting.
    exit /b %ERRORLEVEL%
)

echo === Building Tauri app ===
npm run tauri build 2>&1
