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
    $result = Invoke-SmokeTest -Context $context
    if ($result.Success) {
        Write-Host $result.Message -ForegroundColor Green
        Write-Host ("Session: {0}" -f $result.SessionId)
        exit 0
    }

    Write-Host $result.Message -ForegroundColor Yellow
    exit 1
}
catch {
    Write-Host 'Smoke test failed.' -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Yellow
    exit 1
}
