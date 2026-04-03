#requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$ResetHubData
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'lib/runtime.ps1')

try {
    $context = New-McpAceContext -RootPath $PSScriptRoot
}
catch {
    Write-Host 'MCPace configuration is invalid.' -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Yellow
    exit 1
}
Assert-Prerequisites -Context $context

try { Stop-Hub -Context $context } catch {}
try { Stop-ABP -Context $context } catch {}
try { Remove-Hub -Context $context } catch {}

if ($ResetHubData) {
    if (Test-Path -LiteralPath $context.DataDir) {
        Get-ChildItem -LiteralPath $context.DataDir -Force -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
    }
}

$result = Start-Stack -Context $context

if ($result.ABPReady -and $result.HubReady) {
    Write-Host 'Repair completed. Both services are ready.' -ForegroundColor Green
    exit 0
}

Write-Host 'Repair completed with warnings. Run .\check.ps1 and inspect .\logs.' -ForegroundColor Yellow
exit 1
