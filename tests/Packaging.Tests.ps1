BeforeAll {
    $script:RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
    $script:OutputRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-release-" + [System.Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $script:OutputRoot | Out-Null
}

AfterAll {
    Remove-Item -LiteralPath $script:OutputRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Describe 'release bundle build' {
    It 'builds a portable archive from source' {
        & (Join-Path $script:RepoRoot 'build-release.ps1') -OutputDir $script:OutputRoot
        $LASTEXITCODE | Should -Be 0

        $archive = @(Get-ChildItem -LiteralPath $script:OutputRoot -File -Filter 'mcpace-*.zip' | Select-Object -First 1)
        $archive | Should -Not -BeNullOrEmpty

        $extractRoot = Join-Path $script:OutputRoot 'extracted'
        Expand-Archive -LiteralPath $archive[0].FullName -DestinationPath $extractRoot -Force

        Test-Path -LiteralPath (Join-Path $extractRoot 'auth.ps1') | Should -BeTrue
        Test-Path -LiteralPath (Join-Path $extractRoot 'README.md') | Should -BeTrue
        Test-Path -LiteralPath (Join-Path $extractRoot 'logs\.gitkeep') | Should -BeTrue
        Test-Path -LiteralPath (Join-Path $extractRoot 'data\runtime\.gitkeep') | Should -BeTrue
        Test-Path -LiteralPath (Join-Path $extractRoot 'data\server-state\.gitkeep') | Should -BeTrue
        Test-Path -LiteralPath (Join-Path $extractRoot 'backups\.gitkeep') | Should -BeTrue
        Test-Path -LiteralPath (Join-Path $extractRoot 'data\runtime\mcp_settings.effective.json') | Should -BeFalse
        Test-Path -LiteralPath (Join-Path $extractRoot 'data\runtime\mcp_settings.local-overrides.json') | Should -BeFalse
    }
}
