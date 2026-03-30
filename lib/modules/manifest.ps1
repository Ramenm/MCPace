function Get-PortableManagerRootManifest {
    [CmdletBinding()]
    param()

    return [pscustomobject]@{
        RequiredPaths = @(
            'lib',
            'mcpace.config.json',
            'mcp_settings.json',
            'manager.settings.json',
            'boot.ps1',
            'check.ps1',
            'install.ps1',
            'smoke-test.ps1',
            'start.ps1',
            'autostart.ps1',
            'backup.ps1',
            'auth.ps1',
            'repair.ps1',
            'rotate-logs.ps1',
            'setup-mcp-clients.ps1',
            'validate-readiness.ps1',
            'windows-mcp-host.ps1'
        )
        OptionalPaths = @(
            '.env.example',
            'README.md',
            'LICENSE',
            'docs',
            'reports',
            'memory-bank'
        )
    }
}

function Test-PortableManagerRootLayout {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$RootPath
    )

    $manifest = Get-PortableManagerRootManifest
    $missingRequired = @()
    foreach ($relativePath in @($manifest.RequiredPaths)) {
        if (-not (Test-Path -LiteralPath (Join-Path $RootPath $relativePath))) {
            $missingRequired += $relativePath
        }
    }

    $missingOptional = @()
    foreach ($relativePath in @($manifest.OptionalPaths)) {
        if (-not (Test-Path -LiteralPath (Join-Path $RootPath $relativePath))) {
            $missingOptional += $relativePath
        }
    }

    return [pscustomobject]@{
        RootPath        = $RootPath
        Passed          = ($missingRequired.Count -eq 0)
        MissingRequired = @($missingRequired)
        MissingOptional = @($missingOptional)
        Manifest        = $manifest
    }
}
