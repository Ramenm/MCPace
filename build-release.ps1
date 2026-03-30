#requires -Version 7.0
[CmdletBinding()]
param(
    [string]$OutputDir = (Join-Path $PSScriptRoot 'dist')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'lib/modules/release.ps1')

try {
    $result = New-ReleaseBundle -RootPath $PSScriptRoot -OutputDir $OutputDir
    Write-Host ("Created release bundle: {0}" -f $result.ArchivePath) -ForegroundColor Green
    Write-Host ("Version: {0}" -f $result.Version) -ForegroundColor Cyan
    exit 0
}
catch {
    Write-Host 'Release build failed.' -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Yellow
    exit 1
}
