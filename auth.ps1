#requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$Show,
    [switch]$Reset,
    [switch]$PrintBearerToken
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'lib/runtime.ps1')

try {
    if ($Reset) {
        $stateRoot = Resolve-ManagerStateRootPath -RootPath $PSScriptRoot
        $authStatePath = Join-Path (Join-Path $stateRoot 'data\server-state') 'auth-state.json'
        Reset-LocalAuthState -Path $authStatePath
    }

    $context = New-McpAceContext -RootPath $PSScriptRoot
    Write-ClientLauncher -Context $context | Out-Null

    if ($Reset) {
        $abpState = Get-ABPState -Context $context
        $hubState = Get-HubState -Context $context
        if ($abpState.State -eq 'running' -or $hubState.State -ne 'offline') {
            $stackResult = Ensure-StackRunning -Context $context
            $context = $stackResult.Context
        }
    }

    if ($PrintBearerToken) {
        Write-Output $context.BearerToken
        exit 0
    }

    $authState = Read-LocalAuthState -Path $context.AuthStatePath
    $passwordAvailable = (
        $context.AdminPasswordKnown -and
        -not [string]::IsNullOrWhiteSpace([string]$authState.adminPassword)
    )

    if (-not $Show -and -not $Reset) {
        Write-Host 'Usage:' -ForegroundColor Cyan
        Write-Host '  .\auth.ps1 -Show'
        Write-Host '  .\auth.ps1 -Reset'
        exit 0
    }

    if ($Reset) {
        Write-Host 'Local auth state was regenerated.' -ForegroundColor Green
    }

    Write-Host ("Auth state:   {0}" -f $context.AuthStatePath)
    Write-Host ("Bearer source: {0}" -f $context.BearerTokenSource)
    Write-Host ("Admin source:  {0}" -f $context.AdminPasswordSource)
    Write-Host ("Username:      {0}" -f $context.AdminUsername)
    Write-Host ("Password:      {0}" -f $(if ($passwordAvailable) { [string]$authState.adminPassword } else { '<unavailable: env override active>' }))
    Write-Host ("Bearer token:  {0}" -f $context.BearerToken)
    Write-Host ("Launcher:      {0}" -f (Get-ClientLauncherLabel -Context $context))
    exit 0
}
catch {
    Write-Host 'Auth command failed.' -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Yellow
    exit 1
}
