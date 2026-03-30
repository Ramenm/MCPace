function Get-ClientBridgeArgs {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$McpRemotePackage,
        [Parameter(Mandatory = $true)]
        [int]$HubPort,
        [Parameter(Mandatory = $true)]
        [string]$HeaderValue
    )

    return @(
        '-y',
        $McpRemotePackage,
        "http://127.0.0.1:$HubPort/mcp",
        '--allow-http',
        '--transport',
        'http-only',
        '--header',
        $HeaderValue
    )
}

function Get-ClientLauncherPath {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    return (Join-Path $Context.RootPath $(if ($Context.IsWindows) { 'mcpace.cmd' } else { 'mcpace.sh' }))
}

function Get-ClientLauncherLabel {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    if ($Context.IsWindows) {
        return '.\mcpace.cmd'
    }

    return './mcpace.sh'
}

function Write-ClientLauncher {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $cmdPath = Join-Path $Context.RootPath 'mcpace.cmd'
    $cmdLines = @(
        '@echo off',
        'setlocal EnableExtensions',
        'set "TOKEN=%MCPACE_BEARER_TOKEN%"',
        'if not defined TOKEN (',
        '  where pwsh >nul 2>nul || (>&2 echo PowerShell 7 ^(pwsh^) is required when MCPACE_BEARER_TOKEN is not preset. & exit /b 1)',
        '  call :resolveToken',
        ')',
        'if not defined TOKEN (',
        '  >&2 echo MCPACE_BEARER_TOKEN is unavailable. Run .\auth.ps1 -Show or .\auth.ps1 -Reset.',
        '  exit /b 1',
        ')',
        ('npx -y {0} http://127.0.0.1:{1}/mcp --allow-http --transport http-only --header "Authorization:Bearer %TOKEN%"' -f $Context.McpRemotePackage, $Context.HubPort),
        'exit /b %ERRORLEVEL%',
        '',
        ':resolveToken',
        'for /f "usebackq delims=" %%I in (`pwsh -NoProfile -ExecutionPolicy Bypass -File "%~dp0auth.ps1" -PrintBearerToken 2^>nul`) do set "TOKEN=%%I"',
        'exit /b 0'
    )
    Set-Content -LiteralPath $cmdPath -Value ($cmdLines -join [Environment]::NewLine) -Encoding ASCII

    $shPath = Join-Path $Context.RootPath 'mcpace.sh'
    $shLines = @(
        '#!/usr/bin/env sh',
        'set -eu',
        'script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"',
        'token="${MCPACE_BEARER_TOKEN:-}"',
        'if [ -z "$token" ]; then',
        '  if command -v pwsh >/dev/null 2>&1; then',
        '    token="$(pwsh -NoProfile -ExecutionPolicy Bypass -File "$script_dir/auth.ps1" -PrintBearerToken 2>/dev/null || true)"',
        '  else',
        '    echo "PowerShell 7 (pwsh) is required when MCPACE_BEARER_TOKEN is not preset." >&2',
        '  fi',
        'fi',
        'if [ -z "$token" ]; then',
        '  echo "MCPACE_BEARER_TOKEN is unavailable. Run ./auth.ps1 -Show or ./auth.ps1 -Reset." >&2',
        '  exit 1',
        'fi',
        ('exec npx -y {0} http://127.0.0.1:{1}/mcp --allow-http --transport http-only --header "Authorization:Bearer $token"' -f $Context.McpRemotePackage, $Context.HubPort)
    )
    Set-Content -LiteralPath $shPath -Value ($shLines -join "`n") -Encoding UTF8
    try {
        if (-not $Context.IsWindows -and (Get-Command chmod -ErrorAction SilentlyContinue)) {
            & chmod +x $shPath 2>$null | Out-Null
        }
    }
    catch {}

    if ($Context.IsWindows) {
        return $cmdPath
    }

    return $shPath
}

function Get-ClientConfigJson {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $launcherPath = Get-ClientLauncherPath -Context $Context
    $snippet = [ordered]@{
        mcpServers = [ordered]@{
            mcpace = [ordered]@{
                command = $launcherPath
                args    = @()
            }
        }
    }

    return ($snippet | ConvertTo-Json -Depth 10)
}

function Get-VscodeClientConfigJson {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$LauncherPath
    )

    $snippet = [ordered]@{
        servers = [ordered]@{
            mcpace = [ordered]@{
                type    = 'stdio'
                command = $LauncherPath
                args    = @()
            }
        }
    }

    return ($snippet | ConvertTo-Json -Depth 10)
}

function Get-MaskedToken {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyString()]
        [string]$Token
    )

    if ([string]::IsNullOrWhiteSpace($Token)) {
        return '<missing>'
    }

    if ($Token.Length -le 8) {
        return $Token
    }

    return ('{0}...{1}' -f $Token.Substring(0, 4), $Token.Substring($Token.Length - 4))
}
