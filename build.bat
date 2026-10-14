@echo off
rem Builds Shiki.exe. Uses 2 parallel jobs to keep CPU load gentle.
cd /d "%~dp0src-tauri"
if "%CARGO_TARGET_DIR%"=="" set CARGO_TARGET_DIR=%TEMP%\shiki-target
cargo build --release -j 2 || exit /b 1
copy /y "%CARGO_TARGET_DIR%\release\shiki.exe" "%~dp0Shiki.exe" >nul
echo Built %~dp0Shiki.exe
