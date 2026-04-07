BeforeAll {
    $script:RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
    . (Join-Path $script:RepoRoot 'lib/runtime.ps1')
    $script:Config = Read-JsonFile -Path (Join-Path $script:RepoRoot 'mcpace.config.json')
    $script:SettingsRaw = Read-JsonFile -Path (Join-Path $script:RepoRoot 'mcp_settings.json')
}

Describe 'source settings policy' {
    It 'has no source policy violations' {
        $violations = Get-SourceSettingsPolicyViolations -Config $script:Config -SettingsRaw $script:SettingsRaw -EnforceOptionalDefaults:$true
        @($violations).Count | Should -Be 0
    }

    It 'uses env placeholders for required secrets' {
        [string]$script:SettingsRaw.bearerKeys[0].token | Should -Be '${MCPACE_BEARER_TOKEN}'
        [string](@($script:SettingsRaw.users | Where-Object { [bool]$_.isAdmin } | Select-Object -First 1).password) | Should -Be '${MCPACE_ADMIN_PASSWORD_BCRYPT}'
    }

    It 'keeps optional integrations aligned with declared source defaults' {
        $optionalServers = @($script:Config.servers.PSObject.Properties | Where-Object { -not [bool]$_.Value.required })
        foreach ($server in $optionalServers) {
            $name = [string]$server.Name
            $expectedEnabled = ($server.Value.PSObject.Properties.Name -contains 'defaultEnabled') -and [bool]$server.Value.defaultEnabled
            $entry = @($script:SettingsRaw.mcpServers.PSObject.Properties | Where-Object { [string]$_.Name -eq $name } | Select-Object -First 1)
            if ($entry) {
                [bool]$entry[0].Value.enabled | Should -Be $expectedEnabled
            }
        }
    }

    It 'does not commit pending OAuth authorization state' {
        Test-SettingsTreeHasProperty -Value $script:SettingsRaw -PropertyName 'pendingAuthorization' | Should -BeFalse
    }
}
