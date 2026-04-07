BeforeAll {
    $script:RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
    . (Join-Path $script:RepoRoot 'lib/runtime.ps1')
}

Describe 'local server overrides' {
    BeforeEach {
        $script:StateRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-local-overrides-" + [System.Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Force -Path $script:StateRoot | Out-Null
    }

    AfterEach {
        Remove-Item -LiteralPath $script:StateRoot -Recurse -Force -ErrorAction SilentlyContinue
    }

    It 'applies persisted enabled overrides over the source template' {
        $overridesPath = Join-Path $script:StateRoot 'data\runtime\mcp_settings.local-overrides.json'
        Write-LocalServerOverrides -Path $overridesPath -Overrides ([pscustomobject]@{
            mcpServers = [pscustomobject]@{
                'windows-mcp' = [pscustomobject]@{
                    enabled = $true
                }
            }
        }) | Out-Null

        $context = New-McpAceContext -RootPath $script:RepoRoot -StateRoot $script:StateRoot
        $entry = @($context.ServerRuntime | Where-Object { $_.Name -eq 'windows-mcp' } | Select-Object -First 1)

        [bool]$entry.SourceEnabled | Should -BeFalse
        [bool]$entry.ConfiguredEnabled | Should -BeTrue
        [string]$entry.EnabledSource | Should -Be 'local-override'
        [bool]$context.LocalServerOverrides.mcpServers.'windows-mcp'.enabled | Should -BeTrue
    }

    It 'harvests enabled changes from the previous effective settings into local overrides' {
        $context = New-McpAceContext -RootPath $script:RepoRoot -StateRoot $script:StateRoot
        $effective = Read-JsonFile -Path $context.SettingsEffectivePath
        $effective.mcpServers.'windows-mcp'.enabled = $true
        Write-JsonFile -Path $context.SettingsEffectivePath -Value $effective

        $nextContext = New-McpAceContext -RootPath $script:RepoRoot -StateRoot $script:StateRoot
        $overrides = Read-LocalServerOverrides -Path $nextContext.LocalOverridesPath
        $entry = @($nextContext.ServerRuntime | Where-Object { $_.Name -eq 'windows-mcp' } | Select-Object -First 1)

        [bool]$overrides.mcpServers.'windows-mcp'.enabled | Should -BeTrue
        [bool]$entry.ConfiguredEnabled | Should -BeTrue
        [string]$entry.EnabledSource | Should -Be 'local-override'
    }

    It 'removes enabled overrides after the effective settings return to the source value' {
        $overridesPath = Join-Path $script:StateRoot 'data\runtime\mcp_settings.local-overrides.json'
        Write-LocalServerOverrides -Path $overridesPath -Overrides ([pscustomobject]@{
            mcpServers = [pscustomobject]@{
                github = [pscustomobject]@{
                    enabled = $true
                }
            }
        }) | Out-Null

        $context = New-McpAceContext -RootPath $script:RepoRoot -StateRoot $script:StateRoot
        $effective = Read-JsonFile -Path $context.SettingsEffectivePath
        $effective.mcpServers.github.enabled = $false
        Write-JsonFile -Path $context.SettingsEffectivePath -Value $effective

        $nextContext = New-McpAceContext -RootPath $script:RepoRoot -StateRoot $script:StateRoot
        $overrides = Read-LocalServerOverrides -Path $nextContext.LocalOverridesPath
        $entry = @($nextContext.ServerRuntime | Where-Object { $_.Name -eq 'github' } | Select-Object -First 1)

        @($overrides.mcpServers.PSObject.Properties | ForEach-Object { [string]$_.Name }) | Should -Not -Contain 'github'
        [bool]$entry.ConfiguredEnabled | Should -BeFalse
        [string]$entry.EnabledSource | Should -Be 'source'
    }

    It 'persists the full oauth object from the previous effective settings' {
        $context = New-McpAceContext -RootPath $script:RepoRoot -StateRoot $script:StateRoot
        $effective = Read-JsonFile -Path $context.SettingsEffectivePath
        $effective.mcpServers.github.oauth = [pscustomobject]@{
            accessToken = 'access-123'
            refreshToken = 'refresh-456'
            pendingAuthorization = [pscustomobject]@{
                code = 'oauth-code'
                state = 'oauth-state'
            }
        }
        Write-JsonFile -Path $context.SettingsEffectivePath -Value $effective

        $nextContext = New-McpAceContext -RootPath $script:RepoRoot -StateRoot $script:StateRoot
        $overrides = Read-LocalServerOverrides -Path $nextContext.LocalOverridesPath
        $oauth = $overrides.mcpServers.github.oauth
        $nextEffective = Read-JsonFile -Path $nextContext.SettingsEffectivePath

        [string]$oauth.accessToken | Should -Be 'access-123'
        [string]$oauth.refreshToken | Should -Be 'refresh-456'
        [string]$oauth.pendingAuthorization.code | Should -Be 'oauth-code'
        [string]$nextEffective.mcpServers.github.oauth.pendingAuthorization.state | Should -Be 'oauth-state'
    }

    It 'keeps an existing enabled override when runtime gating forces the effective settings off' {
        $overridesPath = Join-Path $script:StateRoot 'data\runtime\mcp_settings.local-overrides.json'
        Write-LocalServerOverrides -Path $overridesPath -Overrides ([pscustomobject]@{
            mcpServers = [pscustomobject]@{
                firecrawl = [pscustomobject]@{
                    enabled = $true
                }
            }
        }) | Out-Null

        $context = New-McpAceContext -RootPath $script:RepoRoot -StateRoot $script:StateRoot
        $entry = @($context.ServerRuntime | Where-Object { $_.Name -eq 'firecrawl' } | Select-Object -First 1)
        $nextContext = New-McpAceContext -RootPath $script:RepoRoot -StateRoot $script:StateRoot
        $overrides = Read-LocalServerOverrides -Path $nextContext.LocalOverridesPath
        $nextEntry = @($nextContext.ServerRuntime | Where-Object { $_.Name -eq 'firecrawl' } | Select-Object -First 1)

        [bool]$entry.ConfiguredEnabled | Should -BeTrue
        [bool]$entry.EffectiveEnabled | Should -BeFalse
        [string]$entry.DisabledReasonCategory | Should -Be 'placeholder'
        [bool]$overrides.mcpServers.firecrawl.enabled | Should -BeTrue
        [bool]$nextEntry.ConfiguredEnabled | Should -BeTrue
    }
}
