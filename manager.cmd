@echo off
setlocal EnableExtensions

set "COMMAND_NAME=%~1"
if not defined COMMAND_NAME set "COMMAND_NAME=help"
if not "%~1"=="" shift

set "SCRIPT_NAME="
if /I "%COMMAND_NAME%"=="help" goto :help
if /I "%COMMAND_NAME%"=="-h" goto :help
if /I "%COMMAND_NAME%"=="--help" goto :help
if /I "%COMMAND_NAME%"=="install" set "SCRIPT_NAME=install.ps1"
if /I "%COMMAND_NAME%"=="boot" set "SCRIPT_NAME=boot.ps1"
if /I "%COMMAND_NAME%"=="check" set "SCRIPT_NAME=check.ps1"
if /I "%COMMAND_NAME%"=="smoke" set "SCRIPT_NAME=smoke-test.ps1"
if /I "%COMMAND_NAME%"=="readiness" set "SCRIPT_NAME=validate-readiness.ps1"
if /I "%COMMAND_NAME%"=="verify" set "SCRIPT_NAME=verify-manager.ps1"
if /I "%COMMAND_NAME%"=="repair" set "SCRIPT_NAME=repair.ps1"
if /I "%COMMAND_NAME%"=="build-release" set "SCRIPT_NAME=build-release.ps1"
if /I "%COMMAND_NAME%"=="auth" set "SCRIPT_NAME=auth.ps1"
if /I "%COMMAND_NAME%"=="autostart" set "SCRIPT_NAME=autostart.ps1"
if /I "%COMMAND_NAME%"=="backup" set "SCRIPT_NAME=backup.ps1"
if /I "%COMMAND_NAME%"=="rotate-logs" set "SCRIPT_NAME=rotate-logs.ps1"
if /I "%COMMAND_NAME%"=="setup-clients" set "SCRIPT_NAME=setup-mcp-clients.ps1"

if not defined SCRIPT_NAME (
  >&2 echo Unknown command: %COMMAND_NAME%
  >&2 echo Run manager.cmd help for usage.
  exit /b 1
)

where pwsh >nul 2>nul || (
  >&2 echo PowerShell 7 ^(pwsh^) is required.
  exit /b 1
)

pwsh -NoProfile -ExecutionPolicy Bypass -File "%~dp0%SCRIPT_NAME%" %*
exit /b %ERRORLEVEL%

:help
echo Usage:
echo   manager.cmd ^<command^> [PowerShell script args...]
echo.
echo Commands:
echo   install         -^> install.ps1
echo   boot            -^> boot.ps1
echo   check           -^> check.ps1
echo   smoke           -^> smoke-test.ps1
echo   readiness       -^> validate-readiness.ps1
echo   verify          -^> verify-manager.ps1
echo   repair          -^> repair.ps1
echo   build-release   -^> build-release.ps1
echo   auth            -^> auth.ps1
echo   autostart       -^> autostart.ps1
echo   backup          -^> backup.ps1
echo   rotate-logs     -^> rotate-logs.ps1
echo   setup-clients   -^> setup-mcp-clients.ps1
exit /b 0
