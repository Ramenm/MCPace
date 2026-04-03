#requires -Version 7.0
[CmdletBinding()]
param(
    [ValidateSet('status', 'enable', 'disable')]
    [string]$Mode = 'status'
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
    switch ($Mode) {
        'status' {
            $status = Get-AutostartStatus -Context $context
            Write-Host ("Task:    {0}" -f $status.TaskName)
            Write-Host ("Exists:  {0}" -f $status.Exists)
            Write-Host ("Enabled: {0}" -f $status.Enabled)
            Write-Host ("State:   {0}" -f $status.State)
            exit 0
        }
        'enable' {
            $context = Enable-Autostart -Context $context
            Write-Host ("Autostart enabled: {0}" -f $context.AutostartTaskName) -ForegroundColor Green
            exit 0
        }
        'disable' {
            $context = Disable-Autostart -Context $context
            Write-Host ("Autostart disabled: {0}" -f $context.AutostartTaskName) -ForegroundColor Yellow
            exit 0
        }
    }
}
catch {
    Write-Host 'Autostart command failed.' -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Yellow
    exit 1
}
