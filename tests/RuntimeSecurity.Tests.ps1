BeforeAll {
    $script:RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
    . (Join-Path $script:RepoRoot 'lib/runtime.ps1')
    $script:StateRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-runtime-security-" + [System.Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $script:StateRoot | Out-Null
    $script:OldStateRoot = [Environment]::GetEnvironmentVariable('MCPACE_STATE_ROOT')
    $script:OldBearer = [Environment]::GetEnvironmentVariable('MCPACE_BEARER_TOKEN')
    $script:OldBcrypt = [Environment]::GetEnvironmentVariable('MCPACE_ADMIN_PASSWORD_BCRYPT')
    [Environment]::SetEnvironmentVariable('MCPACE_STATE_ROOT', $script:StateRoot)
    [Environment]::SetEnvironmentVariable('MCPACE_BEARER_TOKEN', $null)
    [Environment]::SetEnvironmentVariable('MCPACE_ADMIN_PASSWORD_BCRYPT', $null)
}

AfterAll {
    [Environment]::SetEnvironmentVariable('MCPACE_STATE_ROOT', $script:OldStateRoot)
    [Environment]::SetEnvironmentVariable('MCPACE_BEARER_TOKEN', $script:OldBearer)
    [Environment]::SetEnvironmentVariable('MCPACE_ADMIN_PASSWORD_BCRYPT', $script:OldBcrypt)
    Remove-Item -LiteralPath $script:StateRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Describe 'runtime security hardening' {
    It 'bootstraps local auth state when env is missing' {
        $context = New-McpAceContext -RootPath $script:RepoRoot -StateRoot $script:StateRoot

        Test-Path -LiteralPath $context.AuthStatePath | Should -BeTrue
        $context.StateRoot | Should -Be $script:StateRoot
        $context.BearerTokenSource | Should -Be 'bootstrap'
        $context.AdminPasswordSource | Should -Be 'bootstrap'
        $context.AdminPasswordKnown | Should -BeTrue
        [string]$context.BearerToken | Should -Not -BeNullOrEmpty

        $authState = Read-LocalAuthState -Path $context.AuthStatePath
        [string]$authState.adminPassword | Should -Not -BeNullOrEmpty
        [string]$authState.adminPasswordBcrypt | Should -Match '^\$2'
    }

    It 'generates launcher scripts that resolve the local bearer token automatically' {
        $context = New-McpAceContext -RootPath $script:RepoRoot -StateRoot $script:StateRoot

        Write-ClientLauncher -Context $context | Out-Null

        $cmdContent = Get-Content -LiteralPath (Join-Path $script:RepoRoot 'mcpace.cmd') -Raw -Encoding UTF8
        $shContent = Get-Content -LiteralPath (Join-Path $script:RepoRoot 'mcpace.sh') -Raw -Encoding UTF8

        $cmdContent | Should -Match 'auth\.ps1" -PrintBearerToken'
        $shContent | Should -Match 'auth\.ps1" -PrintBearerToken'
        $cmdContent | Should -Not -Match 'change-me-local'
        $shContent | Should -Not -Match 'change-me-local'
        $cmdContent | Should -Not -Match 'MCPACE_BEARER_TOKEN is required'
        $shContent | Should -Not -Match 'MCPACE_BEARER_TOKEN is required'
    }

    It 'prints launcher-based client config JSON instead of raw bearer headers' {
        $context = New-McpAceContext -RootPath $script:RepoRoot -StateRoot $script:StateRoot

        $json = Get-ClientConfigJson -Context $context
        $json | Should -Match 'mcpace\.cmd'
        $json | Should -Not -Match 'Authorization:Bearer'
        $json | Should -Not -Match [regex]::Escape([string]$context.BearerToken)
    }

    It 'supports env override for bearer token without touching repo-local runtime state' {
        [Environment]::SetEnvironmentVariable('MCPACE_BEARER_TOKEN', 'unit-test-token-1234567890')
        $context = New-McpAceContext -RootPath $script:RepoRoot -StateRoot $script:StateRoot
        [Environment]::SetEnvironmentVariable('MCPACE_BEARER_TOKEN', $null)

        $context.BearerToken | Should -Be 'unit-test-token-1234567890'
        $context.BearerTokenSource | Should -Be 'env'
        $context.RuntimeDir.StartsWith($script:StateRoot, [System.StringComparison]::OrdinalIgnoreCase) | Should -BeTrue
        Test-Path -LiteralPath (Join-Path $script:RepoRoot 'data\runtime\auth-state.json') | Should -BeFalse
    }
}
