@echo off
cd /d %USERPROFILE%\Desktop\kaya-beam\client
call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
npm run tauri build 2>&1
