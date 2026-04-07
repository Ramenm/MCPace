BeforeAll {
    $script:RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
    . (Join-Path $script:RepoRoot 'lib/runtime.ps1')
}

Describe 'verification harness surface' {
    It 'defines unique verification scenarios with the required schema' {
        $catalog = @(Get-VerificationScenarioCatalog -Profile all)

        $catalog.Count | Should -BeGreaterThan 0
        @($catalog.scenarioId | Sort-Object -Unique).Count | Should -Be $catalog.Count

        foreach ($scenario in @($catalog)) {
            [string]$scenario.scenarioId | Should -Not -BeNullOrEmpty
            [string]$scenario.phase | Should -Not -BeNullOrEmpty
            [string]$scenario.scope | Should -Not -BeNullOrEmpty
            [string]$scenario.command | Should -Not -BeNullOrEmpty
            [string]$scenario.expected | Should -Not -BeNullOrEmpty
            [string]$scenario.severity | Should -Not -BeNullOrEmpty
            @($scenario.preconditions).Count | Should -BeGreaterThan 0
            @($scenario.profiles).Count | Should -BeGreaterThan 0
        }
    }

    It 'formats a markdown verification report from the shared schema' {
        $report = New-VerificationReport -Profile 'standard' -Results @(
            [pscustomobject]@{
                scenarioId = 'bootstrap/example'
                phase = 'bootstrap'
                scope = 'current-host'
                preconditions = @('PowerShell 7')
                command = 'pwsh -File .\check.ps1'
                expected = 'Deterministic bootstrap behavior.'
                actual = 'ok'
                artifacts = @()
                verdict = 'pass'
                severity = 'high'
                durationMs = 1
            }
        ) -Environment @{
            os = 'Windows'
            pwsh = '7.6.0'
            node = 'v24.14.1'
            dockerReady = $true
        }

        $markdown = ConvertTo-VerificationMarkdownReport -Report $report

        $markdown | Should -Match 'Verification Audit'
        $markdown | Should -Match 'bootstrap/example'
        $markdown | Should -Match 'Overall verdict'
    }

    It 'keeps verification entrypoints in the release manifest' {
        $manifest = Read-JsonFile -Path (Join-Path $script:RepoRoot 'release-manifest.json')

        @($manifest.includePaths) | Should -Contain 'verify-manager.ps1'
        @($manifest.includePaths) | Should -Contain 'manager.sh'
        @($manifest.includePaths) | Should -Contain 'manager.cmd'
        @($manifest.includePaths) | Should -Contain 'docs'
        Test-Path -LiteralPath (Join-Path $script:RepoRoot 'docs\verification-matrix.md') | Should -BeTrue
    }
}
