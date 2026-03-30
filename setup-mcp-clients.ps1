#requires -Version 7.0
param(
    [switch]$Overwrite
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = $PSScriptRoot
$vscodeDir = Join-Path $root '.vscode'
$target = Join-Path $vscodeDir 'mcp.json'
. (Join-Path $root 'lib/runtime.ps1')

$context = New-McpAceContext -RootPath $root
Write-ClientLauncher -Context $context | Out-Null
$template = Get-VscodeClientConfigJson -LauncherPath (Get-ClientLauncherPath -Context $context)

New-Item -ItemType Directory -Path $vscodeDir -Force | Out-Null

if ((Test-Path -LiteralPath $target) -and -not $Overwrite) {
    Write-Host '[warn] .vscode/mcp.json already exists.' -ForegroundColor Yellow
    Write-Host 'Run ./setup-mcp-clients.ps1 -Overwrite to replace it with the default template.' -ForegroundColor Cyan
    exit 1
}

Set-Content -LiteralPath $target -Value $template -Encoding UTF8
Write-Host "Created: $target" -ForegroundColor Green
Write-Host ("Generated launcher-based client profile. Manual launcher: {0}" -f (Get-ClientLauncherLabel -Context $context)) -ForegroundColor Cyan
