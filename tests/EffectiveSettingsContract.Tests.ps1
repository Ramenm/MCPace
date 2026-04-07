BeforeAll {
    $script:RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
    . (Join-Path $script:RepoRoot 'lib/runtime.ps1')
    $script:OldStateRoot = [Environment]::GetEnvironmentVariable('MCPACE_STATE_ROOT')
    $script:OldBearer = [Environment]::GetEnvironmentVariable('MCPACE_BEARER_TOKEN')
    $script:OldBcrypt = [Environment]::GetEnvironmentVariable('MCPACE_ADMIN_PASSWORD_BCRYPT')
    $script:OldShapeValue = [Environment]::GetEnvironmentVariable('MCPACE_SHAPE_TEST')
}

AfterAll {
    [Environment]::SetEnvironmentVariable('MCPACE_STATE_ROOT', $script:OldStateRoot)
    [Environment]::SetEnvironmentVariable('MCPACE_BEARER_TOKEN', $script:OldBearer)
    [Environment]::SetEnvironmentVariable('MCPACE_ADMIN_PASSWORD_BCRYPT', $script:OldBcrypt)
    [Environment]::SetEnvironmentVariable('MCPACE_SHAPE_TEST', $script:OldShapeValue)
}

Describe 'effective settings contract' {
    It 'preserves JSON arrays across helper transformations' {
        [Environment]::SetEnvironmentVariable('MCPACE_SHAPE_TEST', 'resolved-value')

        $value = [pscustomobject]@{
            empty = @()
            single = @(
                [pscustomobject]@{
                    name = 'one'
                    tags = @('alpha')
                }
            )
            many = @('a', 'b')
            nested = [pscustomobject]@{
                scopes = @('${MCPACE_SHAPE_TEST}')
                ids = @(
                    [pscustomobject]@{
                        values = @()
                    }
                )
            }
        }

        $copy = Copy-JsonLikeValue -Value $value
        $stable = ConvertTo-StableJsonLikeValue -Value $value
        $expanded = Expand-EnvPlaceholdersInValue -Value $value

        foreach ($candidate in @($copy, $stable, $expanded)) {
            Test-JsonArrayLikeValue -Value $candidate.empty | Should -BeTrue
            @($candidate.empty).Count | Should -Be 0

            Test-JsonArrayLikeValue -Value $candidate.single | Should -BeTrue
            @($candidate.single).Count | Should -Be 1
            Test-JsonArrayLikeValue -Value $candidate.single[0].tags | Should -BeTrue
            @($candidate.single[0].tags).Count | Should -Be 1

            Test-JsonArrayLikeValue -Value $candidate.many | Should -BeTrue
            @($candidate.many).Count | Should -Be 2

            Test-JsonArrayLikeValue -Value $candidate.nested.scopes | Should -BeTrue
            @($candidate.nested.scopes).Count | Should -Be 1
            Test-JsonArrayLikeValue -Value $candidate.nested.ids | Should -BeTrue
            @($candidate.nested.ids).Count | Should -Be 1
            Test-JsonArrayLikeValue -Value $candidate.nested.ids[0].values | Should -BeTrue
            @($candidate.nested.ids[0].values).Count | Should -Be 0
        }

        [string]$expanded.nested.scopes[0] | Should -Be 'resolved-value'
    }

    It 'writes effective settings to disk without collapsing required arrays' {
        $stateRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-effective-settings-" + [System.Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Force -Path $stateRoot | Out-Null

        try {
            [Environment]::SetEnvironmentVariable('MCPACE_STATE_ROOT', $stateRoot)
            [Environment]::SetEnvironmentVariable('MCPACE_BEARER_TOKEN', 'unit-test-token-1234567890')
            [Environment]::SetEnvironmentVariable('MCPACE_ADMIN_PASSWORD_BCRYPT', '$2b$10$1hLtpWUfeMNXxJBW9KWeneA5OClk.HQy5a1z/PHIcX0l6094xgrKq')

            $context = New-McpAceContext -RootPath $script:RepoRoot -StateRoot $stateRoot
            $effective = Read-JsonFile -Path $context.SettingsEffectivePath

            [string]$context.PowerShellCommand | Should -Match 'pwsh'

            Test-JsonArrayLikeValue -Value $effective.bearerKeys | Should -BeTrue
            @($effective.bearerKeys).Count | Should -BeGreaterOrEqual 1
            Test-JsonArrayLikeValue -Value $effective.bearerKeys[0].allowedGroups | Should -BeTrue
            @($effective.bearerKeys[0].allowedGroups).Count | Should -Be 0
            Test-JsonArrayLikeValue -Value $effective.bearerKeys[0].allowedServers | Should -BeTrue
            @($effective.bearerKeys[0].allowedServers).Count | Should -Be 0

            Test-JsonArrayLikeValue -Value $effective.users | Should -BeTrue
            @($effective.users).Count | Should -BeGreaterOrEqual 1
            @($effective.users | Where-Object { [bool]$_.isAdmin }).Count | Should -BeGreaterOrEqual 1

            Test-JsonArrayLikeValue -Value $effective.prompts | Should -BeTrue
            @($effective.prompts).Count | Should -Be 0
            Test-JsonArrayLikeValue -Value $effective.resources | Should -BeTrue
            @($effective.resources).Count | Should -Be 0

            Test-JsonArrayLikeValue -Value $effective.mcpServers.browser.args | Should -BeTrue
            @($effective.mcpServers.browser.args).Count | Should -BeGreaterOrEqual 1
            Test-JsonArrayLikeValue -Value $effective.mcpServers.filesystem.args | Should -BeTrue
            @($effective.mcpServers.filesystem.args).Count | Should -BeGreaterOrEqual 1
            Test-JsonArrayLikeValue -Value $effective.systemConfig.oauthServer.allowedScopes | Should -BeTrue
            @($effective.systemConfig.oauthServer.allowedScopes).Count | Should -BeGreaterOrEqual 1
            Test-JsonArrayLikeValue -Value $effective.systemConfig.oauthServer.dynamicRegistration.allowedGrantTypes | Should -BeTrue
            @($effective.systemConfig.oauthServer.dynamicRegistration.allowedGrantTypes).Count | Should -BeGreaterOrEqual 1

            $clientConfig = Get-ClientConfigJson -Context $context | ConvertFrom-Json
            [string]$clientConfig.mcpServers.mcpace.command | Should -Match 'mcpace\.(cmd|sh)$'
            @($clientConfig.mcpServers.mcpace.args).Count | Should -Be 0

            $vscodeConfig = Get-VscodeClientConfigJson -LauncherPath (Get-ClientLauncherPath -Context $context) | ConvertFrom-Json
            [string]$vscodeConfig.servers.mcpace.command | Should -Match 'mcpace\.(cmd|sh)$'
            @($vscodeConfig.servers.mcpace.args).Count | Should -Be 0
        }
        finally {
            Remove-Item -LiteralPath $stateRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
