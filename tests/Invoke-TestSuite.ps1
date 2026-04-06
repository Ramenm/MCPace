#requires -Version 7.0
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (Get-Command Install-PSResource -ErrorAction SilentlyContinue) {
    Install-PSResource Pester -Version '[5.0.0,6.0.0)' -Scope CurrentUser -TrustRepository -Quiet -ErrorAction Stop
}
elseif (Get-Command Install-Module -ErrorAction SilentlyContinue) {
    Set-PSRepository PSGallery -InstallationPolicy Trusted
    Install-Module Pester -MinimumVersion 5.0.0 -MaximumVersion 5.999.999 -Scope CurrentUser -Force -SkipPublisherCheck -ErrorAction Stop
}

$available = @(Get-Module -ListAvailable -Name Pester | Sort-Object Version -Descending)
if ($available.Count -eq 0) {
    throw 'Pester 5+ is required but no installable module was found.'
}

$selected = $available | Where-Object { $_.Version.Major -ge 5 } | Select-Object -First 1
if (-not $selected) {
    throw ("Pester 5+ is required. Available versions: {0}" -f (($available | ForEach-Object { $_.Version.ToString() }) -join ', '))
}

Import-Module (Join-Path $selected.ModuleBase 'Pester.psd1') -Force
$oldStateRoot = [Environment]::GetEnvironmentVariable('MCPACE_STATE_ROOT')
$testStateRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-pester-state-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $testStateRoot | Out-Null
[Environment]::SetEnvironmentVariable('MCPACE_STATE_ROOT', $testStateRoot)

try {
    Invoke-Pester -Path (Join-Path $PSScriptRoot '*.Tests.ps1') -CI
}
finally {
    [Environment]::SetEnvironmentVariable('MCPACE_STATE_ROOT', $oldStateRoot)
    Remove-Item -LiteralPath $testStateRoot -Recurse -Force -ErrorAction SilentlyContinue
}
