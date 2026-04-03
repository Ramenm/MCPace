#requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$SkipSmoke
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
    $result = Invoke-Install -Context $context -RunSmoke:(-not $SkipSmoke)
    Write-Host ("ABP ready:  {0}" -f $result.ABPReady)
    Write-Host ("Hub ready:  {0}" -f $result.HubReady)
    Write-Host ("Logs pruned: {0}" -f $result.RotatedCount)
    Write-Host ("Smoke:      {0}" -f $result.SmokeMessage)

    if ($result.Success) {
        Write-Host 'Install completed successfully.' -ForegroundColor Green
        exit 0
    }

    Write-Host 'Install completed with warnings.' -ForegroundColor Yellow
    exit 1
}
catch {
    Write-Host 'Install failed.' -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Yellow
    exit 1
}
