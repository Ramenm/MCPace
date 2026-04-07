Describe 'repository governance baseline' {
    BeforeAll {
        $script:RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
    }

    It 'contains required governance files' {
        foreach ($relativePath in @('.gitignore', '.editorconfig', '.gitattributes', 'LICENSE', 'CONTRIBUTING.md', 'CODEOWNERS', '.env.example', 'release-manifest.json')) {
            Test-Path -LiteralPath (Join-Path $script:RepoRoot $relativePath) | Should -BeTrue
        }
    }

    It 'ignores runtime state and generated launchers' {
        $gitignore = Get-Content -LiteralPath (Join-Path $script:RepoRoot '.gitignore') -Raw -Encoding UTF8
        foreach ($needle in @('/.omx/', '/.env', '/data/', '/logs/', '/backups/', '/mcpace.cmd', '/mcpace.sh', '/dist/')) {
            $gitignore | Should -Match ([regex]::Escape($needle))
        }
    }
}
