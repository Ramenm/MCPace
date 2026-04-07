Describe 'github governance kit' {
    BeforeAll {
        $script:RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
    }

    It 'contains GitHub templates and repo-local skill pack files' {
        foreach ($relativePath in @(
            '.github/pull_request_template.md',
            '.github/ISSUE_TEMPLATE/config.yml',
            '.github/ISSUE_TEMPLATE/cleanup-request.yml',
            '.github/ISSUE_TEMPLATE/repair-report.yml',
            'docs/github-repo-kit.md',
            'skills/github-project-prepare/SKILL.md',
            'skills/github-project-repair/SKILL.md',
            'skills/github-project-cleanup/SKILL.md'
        )) {
            Test-Path -LiteralPath (Join-Path $script:RepoRoot $relativePath) | Should -BeTrue
        }
    }

    It 'keeps cleanup behind an explicit request gate' {
        $cleanupSkill = Get-Content -LiteralPath (Join-Path $script:RepoRoot 'skills/github-project-cleanup/SKILL.md') -Raw -Encoding UTF8
        $cleanupTemplate = Get-Content -LiteralPath (Join-Path $script:RepoRoot '.github/ISSUE_TEMPLATE/cleanup-request.yml') -Raw -Encoding UTF8

        $cleanupSkill | Should -Match 'explicit'
        $cleanupSkill | Should -Match 'prepare'
        $cleanupSkill | Should -Match 'repair'
        $cleanupTemplate | Should -Match 'explicit'
        $cleanupTemplate | Should -Match 'cleanup'
    }

    It 'documents verification-first prepare and repair flows' {
        $prepareSkill = Get-Content -LiteralPath (Join-Path $script:RepoRoot 'skills/github-project-prepare/SKILL.md') -Raw -Encoding UTF8
        $repairSkill = Get-Content -LiteralPath (Join-Path $script:RepoRoot 'skills/github-project-repair/SKILL.md') -Raw -Encoding UTF8
        $prTemplate = Get-Content -LiteralPath (Join-Path $script:RepoRoot '.github/pull_request_template.md') -Raw -Encoding UTF8

        $prepareSkill | Should -Match 'verify'
        $prepareSkill | Should -Match 'gitignore'
        $repairSkill | Should -Match 'reproduce'
        $repairSkill | Should -Match 'minimal'
        $prTemplate | Should -Match 'Verification'
        $prTemplate | Should -Match 'Not-tested'
    }
}
