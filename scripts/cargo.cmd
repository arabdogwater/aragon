@echo off
rem Runs cargo inside an MSVC environment (Build Tools are often not on PATH).
rem Usage: scripts\cargo.cmd build --release
setlocal
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if defined VSCMD_VER goto run
if not exist "%VSWHERE%" goto run
for /f "usebackq delims=" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSROOT=%%i"
if defined VSROOT if exist "%VSROOT%\VC\Auxiliary\Build\vcvars64.bat" call "%VSROOT%\VC\Auxiliary\Build\vcvars64.bat" >nul 2>nul
:run
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
cargo %*
