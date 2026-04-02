#requires -Version 7.0
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
    Assert-Prerequisites -Context $context
    $result = Ensure-StackRunning -Context $context
    if ($result.ABPReady -and $result.HubReady) {
        Write-Host 'Boot start completed. Services are ready.' -ForegroundColor Green
        exit 0
    }

    Write-Host 'Boot start completed with warnings. Run .\check.ps1.' -ForegroundColor Yellow
    exit 1
}
catch {
    Write-Host 'Boot start failed.' -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Yellow
    exit 1
}
