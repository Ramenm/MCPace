function Get-FreeTcpPort {
    [CmdletBinding()]
    param()

    $listener = New-Object System.Net.Sockets.TcpListener ([System.Net.IPAddress]::Loopback, 0)
    try {
        $listener.Start()
        return ([int]$listener.LocalEndpoint.Port)
    }
    finally {
        $listener.Stop()
    }
}

function Copy-PortableManagerRoot {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$SourceRoot,
        [Parameter(Mandatory = $true)]
        [string]$DestinationRoot
    )

    $layout = Test-PortableManagerRootLayout -RootPath $SourceRoot
    if (-not $layout.Passed) {
        throw ("portable manager root is incomplete: {0}" -f ($layout.MissingRequired -join ', '))
    }

    New-Item -ItemType Directory -Force -Path $DestinationRoot | Out-Null
    foreach ($relativePath in @($layout.Manifest.RequiredPaths)) {
        $sourcePath = Join-Path $SourceRoot $relativePath
        $destinationPath = Join-Path $DestinationRoot $relativePath
        if (Test-Path -LiteralPath $sourcePath -PathType Container) {
            New-Item -ItemType Directory -Force -Path $destinationPath | Out-Null
            Copy-Item -Path (Join-Path $sourcePath '*') -Destination $destinationPath -Recurse -Force
        }
        else {
            $destinationDir = Split-Path -Parent $destinationPath
            if (-not [string]::IsNullOrWhiteSpace($destinationDir)) {
                New-Item -ItemType Directory -Force -Path $destinationDir | Out-Null
            }
            Copy-Item -LiteralPath $sourcePath -Destination $destinationPath -Force
        }
    }
}

function Get-VerificationScenarioCatalog {
    [CmdletBinding()]
    param(
        [ValidateSet('all', 'standard', 'full', 'ci-runtime')]
        [string]$Profile = 'all'
    )

    $catalog = New-Object System.Collections.Generic.List[object]

    function Add-Scenario {
        param(
            [Parameter(Mandatory = $true)][string]$ScenarioId,
            [Parameter(Mandatory = $true)][string]$Phase,
            [Parameter(Mandatory = $true)][string]$Scope,
            [Parameter(Mandatory = $true)][string[]]$Preconditions,
            [Parameter(Mandatory = $true)][string]$Command,
            [Parameter(Mandatory = $true)][string]$Expected,
            [Parameter(Mandatory = $true)][string]$Severity,
            [Parameter(Mandatory = $true)][string[]]$Profiles
        )

        $null = $catalog.Add([pscustomobject]@{
            scenarioId    = $ScenarioId
            phase         = $Phase
            scope         = $Scope
            preconditions = @($Preconditions)
            command       = $Command
            expected      = $Expected
            severity      = $Severity
            profiles      = @($Profiles)
        })
    }

    Add-Scenario -ScenarioId 'bootstrap/windows-direct-check' -Phase 'bootstrap' -Scope 'windows-current-host' -Preconditions @('Windows host', 'PowerShell 7 installed') -Command 'pwsh -NoProfile -File .\check.ps1' -Expected 'Direct script execution is deterministic; execution-policy failure is recorded as a blocker.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'bootstrap/windows-bypass-check' -Phase 'bootstrap' -Scope 'windows-current-host' -Preconditions @('Windows host', 'PowerShell 7 installed') -Command 'pwsh -NoProfile -ExecutionPolicy Bypass -File .\check.ps1' -Expected 'Bypass bootstrap path succeeds and reaches a usable stack inspection path.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'bootstrap/missing-docker' -Phase 'bootstrap' -Scope 'current-host' -Preconditions @('PowerShell 7 installed') -Command 'Assert-Prerequisites with docker hidden from PATH' -Expected 'Missing Docker produces a deterministic prerequisite error.' -Severity 'high' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'bootstrap/missing-node' -Phase 'bootstrap' -Scope 'current-host' -Preconditions @('PowerShell 7 installed') -Command 'Assert-Prerequisites with node hidden from PATH' -Expected 'Missing Node.js produces a deterministic prerequisite error.' -Severity 'high' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'bootstrap/non-pwsh-shell' -Phase 'bootstrap' -Scope 'windows-current-host' -Preconditions @('Windows host', 'Windows PowerShell 5.1 available') -Command 'powershell.exe -NoProfile -File .\boot.ps1' -Expected 'Unsupported shell fails deterministically instead of hanging.' -Severity 'high' -Profiles @('standard', 'full')

    Add-Scenario -ScenarioId 'source/pester-suite' -Phase 'source' -Scope 'current-host' -Preconditions @('PowerShell 7') -Command 'pwsh -NoProfile -ExecutionPolicy Bypass -Command "Invoke-Pester -CI -Path ./tests"' -Expected 'Repository source suite passes without introducing new regressions.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')

    Add-Scenario -ScenarioId 'lifecycle/boot-idempotent' -Phase 'lifecycle' -Scope 'current-host' -Preconditions @('Docker ready', 'Node.js 18+', 'PowerShell 7') -Command 'pwsh -NoProfile -ExecutionPolicy Bypass -File .\boot.ps1' -Expected 'Boot path succeeds when rerun and does not leave the stack half-broken.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'lifecycle/check' -Phase 'lifecycle' -Scope 'current-host' -Preconditions @('Stack booted') -Command 'pwsh -NoProfile -ExecutionPolicy Bypass -File .\check.ps1' -Expected 'Check path exits 0 only when ABP and MCPace required path are ready.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'lifecycle/smoke' -Phase 'lifecycle' -Scope 'current-host' -Preconditions @('Stack booted', 'Valid bearer token') -Command 'pwsh -NoProfile -ExecutionPolicy Bypass -File .\smoke-test.ps1' -Expected 'Smoke path validates MCP initialize/repeat/reconnect against the public endpoint.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'lifecycle/readiness' -Phase 'lifecycle' -Scope 'current-host' -Preconditions @('Stack booted', 'Docker ready', 'Node.js 18+', 'PowerShell 7') -Command 'pwsh -NoProfile -ExecutionPolicy Bypass -File .\validate-readiness.ps1' -Expected 'Readiness path validates live stack, portability, and Docker mount policy within the configured budget.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')

    Add-Scenario -ScenarioId 'clients/launcher-config' -Phase 'clients' -Scope 'current-host' -Preconditions @('PowerShell 7', 'Client launcher generated') -Command 'Get-ClientConfigJson / Write-ClientLauncher' -Expected 'Generic launcher-first config references only mcpace.cmd or mcpace.sh with no direct upstream server endpoints.' -Severity 'high' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'clients/editor-profile' -Phase 'clients' -Scope 'current-host' -Preconditions @('PowerShell 7') -Command 'pwsh -NoProfile -ExecutionPolicy Bypass -File .\setup-mcp-clients.ps1 -Overwrite' -Expected 'Generated editor profile references only the launcher surface.' -Severity 'high' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'clients/api-health' -Phase 'clients' -Scope 'current-host' -Preconditions @('Stack booted') -Command 'GET /health' -Expected 'Health endpoint returns 200 and a supported health status.' -Severity 'high' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'clients/api-servers' -Phase 'clients' -Scope 'current-host' -Preconditions @('Stack booted', 'Valid bearer token') -Command 'Authenticated GET /api/servers' -Expected 'Authenticated server listing succeeds and returns an array-backed data envelope.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'clients/mcp-session' -Phase 'clients' -Scope 'current-host' -Preconditions @('Stack booted', 'Valid bearer token') -Command 'Authenticated MCP initialize + notifications/initialized + tools/list + resources/list + reconnect' -Expected 'MCP session handshake, repeat request, and reconnect all succeed without session corruption.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')

    Add-Scenario -ScenarioId 'servers/required-path' -Phase 'servers' -Scope 'current-host' -Preconditions @('Stack booted') -Command 'Get-HubServerStatuses + Get-RequiredServerConnectivity + Get-ABPState' -Expected 'Every required server is effectiveEnabled and connected; the browser path is reachable on the host.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'servers/optional-default-disabled' -Phase 'servers' -Scope 'current-host' -Preconditions @('Context generated') -Command 'Inspect optional server runtime entries with source defaults' -Expected 'Default-disabled optional servers remain explained, stable, and correctly gated.' -Severity 'medium' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'servers/optional-source-enabled' -Phase 'servers' -Scope 'current-host' -Preconditions @('Context generated') -Command 'Inspect optional source-enabled server runtime entries' -Expected 'Source-enabled optional servers remain observable with correct configured/effective state and disabled reasons when locally overridden.' -Severity 'medium' -Profiles @('standard', 'full', 'ci-runtime')

    foreach ($optionalServerName in @('github', 'git', 'sentry', 'windows-mcp', 'screenpipe', 'firecrawl')) {
        Add-Scenario -ScenarioId ("servers/optional-user-enabled/{0}" -f $optionalServerName) -Phase 'servers' -Scope 'isolated-manager-root' -Preconditions @('Copied manager root', 'User-style enable override applied') -Command ("Enable optional server '{0}' in isolated runtime context and verify live connection or explicit runtime gating." -f $optionalServerName) -Expected ("If prerequisites and credentials exist, server '{0}' reaches a live connected state; otherwise configuredEnabled remains true and runtime gating is explicit." -f $optionalServerName) -Severity 'high' -Profiles @('standard', 'full')
    }

    Add-Scenario -ScenarioId 'persistence/auth-bootstrap' -Phase 'persistence' -Scope 'temp-state-root' -Preconditions @('Auth env vars cleared') -Command 'New-McpAceContext on temp state root' -Expected 'Local auth state bootstraps automatically when env auth is absent.' -Severity 'high' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'persistence/env-override' -Phase 'persistence' -Scope 'temp-state-root' -Preconditions @('Temp state root', 'Bearer env override set') -Command 'New-McpAceContext with MCPACE_BEARER_TOKEN override' -Expected 'Environment override wins over local bootstrap auth without mutating repo-local runtime state.' -Severity 'high' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'persistence/local-overrides' -Phase 'persistence' -Scope 'temp-state-root' -Preconditions @('Temp state root') -Command 'Write and reload local server overrides' -Expected 'Configured enablement intent survives restarts through local overrides.' -Severity 'high' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'persistence/effective-array-shape' -Phase 'persistence' -Scope 'temp-state-root' -Preconditions @('Temp state root') -Command 'Generate effective settings and inspect array-backed fields' -Expected 'Generated effective settings preserve required arrays and workspace-aware transforms.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'persistence/backup' -Phase 'persistence' -Scope 'temp-state-root' -Preconditions @('Temp state root') -Command 'New-DataBackup' -Expected 'Backup archive is created and retention policy remains bounded.' -Severity 'medium' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'persistence/release-bundle' -Phase 'persistence' -Scope 'current-host' -Preconditions @('PowerShell 7') -Command 'pwsh -NoProfile -ExecutionPolicy Bypass -File .\build-release.ps1 -OutputDir <temp>' -Expected 'Portable release bundle is built from manifest-tracked source without runtime state leakage.' -Severity 'high' -Profiles @('standard', 'full', 'ci-runtime')

    Add-Scenario -ScenarioId 'destructive/temp-auth-reset' -Phase 'destructive' -Scope 'temp-state-root' -Preconditions @('Temp state root', 'Non-destructive scenarios passed') -Command 'Reset-LocalAuthState in temp runtime context' -Expected 'Auth reset changes auth material and the next context remains readable.' -Severity 'high' -Profiles @('standard', 'full')
    Add-Scenario -ScenarioId 'destructive/temp-reset-hub-data' -Phase 'destructive' -Scope 'isolated-manager-root' -Preconditions @('Copied manager root', 'Isolated stack booted') -Command 'pwsh -NoProfile -ExecutionPolicy Bypass -File .\repair.ps1 -ResetHubData' -Expected 'Destructive repair on isolated manager root returns the stack to a healthy readable state.' -Severity 'high' -Profiles @('standard', 'full')
    Add-Scenario -ScenarioId 'destructive/live-auth-reset' -Phase 'destructive' -Scope 'current-host' -Preconditions @('Backup created', 'Non-destructive current-host suite passed') -Command 'pwsh -NoProfile -ExecutionPolicy Bypass -File .\auth.ps1 -Reset' -Expected 'Live auth reset completes and the stack remains recoverable.' -Severity 'blocker' -Profiles @('full')
    Add-Scenario -ScenarioId 'destructive/live-reset-hub-data' -Phase 'destructive' -Scope 'current-host' -Preconditions @('Backup created', 'Non-destructive current-host suite passed') -Command 'pwsh -NoProfile -ExecutionPolicy Bypass -File .\repair.ps1 -ResetHubData' -Expected 'Live destructive repair completes and the stack returns to a healthy readable state.' -Severity 'blocker' -Profiles @('full')

    Add-Scenario -ScenarioId 'platform/windows-current-host' -Phase 'platform' -Scope 'windows-current-host' -Preconditions @('Windows host') -Command 'Current verification harness run' -Expected 'Windows claims are backed by the current host report.' -Severity 'medium' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'platform/ubuntu-workflow' -Phase 'platform' -Scope 'github-actions' -Preconditions @('runtime-smoke-ubuntu workflow exists') -Command '.github/workflows/runtime-smoke-ubuntu.yml' -Expected 'Ubuntu runtime lane is represented as a workflow-driven support claim and report artifact producer.' -Severity 'medium' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'platform/macos-manual-gate' -Phase 'platform' -Scope 'manual-gate' -Preconditions @('docs/runtime-smoke-macos.md exists') -Command 'docs/runtime-smoke-macos.md' -Expected 'macOS remains explicitly documented as a manual gate and not auto-proven unless run.' -Severity 'medium' -Profiles @('standard', 'full', 'ci-runtime')

    Add-Scenario -ScenarioId 'live-regression/boot' -Phase 'readiness' -Scope 'current-host' -Preconditions @('Current manager root complete') -Command 'validate-readiness.ps1 internal live regression boot step' -Expected 'Live stack boot reaches ready state.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'live-regression/check' -Phase 'readiness' -Scope 'current-host' -Preconditions @('Live regression boot passed') -Command 'validate-readiness.ps1 internal live regression required-path check' -Expected 'Current required path is ready.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'live-regression/smoke' -Phase 'readiness' -Scope 'current-host' -Preconditions @('Live regression check passed') -Command 'validate-readiness.ps1 internal live regression smoke step' -Expected 'Current live smoke step passes.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'portable-layout/current-root' -Phase 'readiness' -Scope 'current-host' -Preconditions @('Current manager root') -Command 'validate-readiness.ps1 internal portable-layout check' -Expected 'Current manager root contains the required portable layout.' -Severity 'high' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'portability/copy-manager-root' -Phase 'readiness' -Scope 'isolated-manager-root' -Preconditions @('Validation temp root created') -Command 'validate-readiness.ps1 internal manager root copy step' -Expected 'Portable manager root copies successfully into an isolated environment.' -Severity 'high' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'portability/workspace-registry' -Phase 'readiness' -Scope 'isolated-manager-root' -Preconditions @('Copied manager root') -Command 'validate-readiness.ps1 internal workspace registry step' -Expected 'External primary and read-only extra workspaces resolve correctly.' -Severity 'high' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'compatibility/effective-settings' -Phase 'readiness' -Scope 'isolated-manager-root' -Preconditions @('Copied manager root', 'Isolated effective settings generated') -Command 'validate-readiness.ps1 internal workspace-aware effective settings step' -Expected 'Filesystem, serena, lean-ctx, and git transforms remain correct.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'portability/boot' -Phase 'readiness' -Scope 'isolated-manager-root' -Preconditions @('Copied manager root') -Command 'validate-readiness.ps1 internal isolated boot step' -Expected 'Isolated ABP and MCPace are ready.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'portability/check' -Phase 'readiness' -Scope 'isolated-manager-root' -Preconditions @('Isolated boot passed') -Command 'validate-readiness.ps1 internal isolated required-path check' -Expected 'Isolated required path is ready.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'portability/smoke' -Phase 'readiness' -Scope 'isolated-manager-root' -Preconditions @('Isolated required path ready') -Command 'validate-readiness.ps1 internal isolated smoke step' -Expected 'Isolated smoke step passes.' -Severity 'blocker' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'policy/rw-mount' -Phase 'readiness' -Scope 'isolated-manager-root' -Preconditions @('Copied manager root', 'Docker ready') -Command 'validate-readiness.ps1 internal read-write mount probe' -Expected 'Docker read-write mount permits host file writes.' -Severity 'high' -Profiles @('standard', 'full', 'ci-runtime')
    Add-Scenario -ScenarioId 'policy/ro-mount' -Phase 'readiness' -Scope 'isolated-manager-root' -Preconditions @('Copied manager root', 'Docker ready') -Command 'validate-readiness.ps1 internal read-only mount probe' -Expected 'Docker read-only mount blocks writes with a read-only filesystem error.' -Severity 'high' -Profiles @('standard', 'full', 'ci-runtime')

    if ($Profile -eq 'all') {
        return @($catalog | Sort-Object phase, scenarioId)
    }

    return @($catalog | Where-Object { $_.profiles -contains $Profile } | Sort-Object phase, scenarioId)
}

function New-VerificationReport {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Profile,
        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [array]$Results,
        [Parameter(Mandatory = $true)]
        [hashtable]$Environment
    )

    $counts = [ordered]@{}
    foreach ($key in @('pass', 'fail', 'warn', 'not-proven', 'skipped', 'not-applicable')) {
        $counts[$key] = @($Results | Where-Object { [string]$_.verdict -eq $key }).Count
    }

    $overallVerdict = 'pass'
    if ($counts['fail'] -gt 0) {
        $overallVerdict = 'fail'
    }
    elseif ($counts['warn'] -gt 0) {
        $overallVerdict = 'warn'
    }
    elseif ($counts['not-proven'] -gt 0) {
        $overallVerdict = 'not-proven'
    }

    return [pscustomobject]@{
        schemaVersion = '1.0'
        generatedAt   = (Get-Date).ToString('o')
        profile       = $Profile
        overallVerdict = $overallVerdict
        environment   = [pscustomobject]$Environment
        counts        = [pscustomobject]$counts
        results       = @($Results)
    }
}

function ConvertTo-VerificationMarkdownReport {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        $Report
    )

    $lines = @(
        '# Verification Audit',
        '',
        ('Generated: `{0}`' -f [string]$Report.generatedAt),
        ('Profile: `{0}`' -f [string]$Report.profile),
        ('Overall verdict: `{0}`' -f [string]$Report.overallVerdict),
        '',
        '## Summary',
        '',
        ('- Host OS: `{0}`' -f [string]$Report.environment.os),
        ('- PowerShell: `{0}`' -f [string]$Report.environment.pwsh),
        ('- Node: `{0}`' -f [string]$Report.environment.node),
        ('- Docker ready: `{0}`' -f [string]$Report.environment.dockerReady),
        ('- Results: pass=`{0}` fail=`{1}` warn=`{2}` not-proven=`{3}` skipped=`{4}` not-applicable=`{5}`' -f $Report.counts.pass, $Report.counts.fail, $Report.counts.warn, $Report.counts.'not-proven', $Report.counts.skipped, $Report.counts.'not-applicable'),
        ''
    )

    foreach ($phaseGroup in @($Report.results | Group-Object phase | Sort-Object Name)) {
        $lines += @(
            ("## {0}" -f ([System.Globalization.CultureInfo]::InvariantCulture.TextInfo.ToTitleCase([string]$phaseGroup.Name))),
            '',
            '| Scenario | Verdict | Severity | Scope | Notes |',
            '| --- | --- | --- | --- | --- |'
        )
        foreach ($result in @($phaseGroup.Group | Sort-Object scenarioId)) {
            $actual = [string]$result.actual
            if ($actual.Length -gt 120) {
                $actual = $actual.Substring(0, 117) + '...'
            }
            $actual = $actual.Replace('|', '\|').Replace("`r", ' ').Replace("`n", ' ')
            $lines += ('| `{0}` | `{1}` | `{2}` | `{3}` | {4} |' -f [string]$result.scenarioId, [string]$result.verdict, [string]$result.severity, [string]$result.scope, $actual)
        }
        $lines += ''
    }

    return ($lines -join [Environment]::NewLine)
}

function ConvertTo-VerificationMatrixMarkdown {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [array]$Catalog
    )

    $lines = @(
        '# Verification Scenario Matrix',
        '',
        'This document is the normative scenario catalog for `verify-manager.ps1`.',
        '',
        'Columns:',
        '',
        '- `Scenario ID`: stable identifier used in JSON and Markdown reports',
        '- `Preconditions`: required setup or environment assumptions',
        '- `Command`: command or internal path the harness exercises',
        '- `Expected`: acceptance target',
        '- `Severity`: blocker level when the scenario fails',
        ''
    )

    foreach ($phaseGroup in @($Catalog | Group-Object phase | Sort-Object Name)) {
        $lines += @(
            ("## {0}" -f ([System.Globalization.CultureInfo]::InvariantCulture.TextInfo.ToTitleCase([string]$phaseGroup.Name))),
            '',
            '| Scenario ID | Preconditions | Command | Expected | Severity |',
            '| --- | --- | --- | --- | --- |'
        )
        foreach ($scenario in @($phaseGroup.Group | Sort-Object scenarioId)) {
            $preconditions = (@($scenario.preconditions) -join '; ').Replace('|', '\|')
            $command = ([string]$scenario.command).Replace('|', '\|')
            $expected = ([string]$scenario.expected).Replace('|', '\|')
            $lines += ('| `{0}` | {1} | `{2}` | {3} | `{4}` |' -f [string]$scenario.scenarioId, $preconditions, $command, $expected, [string]$scenario.severity)
        }
        $lines += ''
    }

    return ($lines -join [Environment]::NewLine)
}
