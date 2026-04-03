#requires -Version 7.0
[CmdletBinding()]
param(
    [int]$Days = 0
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

try {
    $result = Rotate-Logs -Context $context -Days $Days
    Write-Host ("Removed {0} file(s) older than {1} day(s)." -f $result.RemovedCount, $result.Days) -ForegroundColor Green
    exit 0
}
catch {
    Write-Host 'Log rotation failed.' -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Yellow
    exit 1
}
