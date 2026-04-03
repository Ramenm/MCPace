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
    $result = New-DataBackup -Context $context
    Write-Host ("Backup created: {0}" -f $result.BackupPath) -ForegroundColor Green
    Write-Host ("Purged old backups: {0}" -f $result.PurgedCount)
    exit 0
}
catch {
    Write-Host 'Backup failed.' -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Yellow
    exit 1
}
