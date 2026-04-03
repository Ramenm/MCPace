#requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$KeepArtifacts,
    [switch]$SkipDockerPolicy,
    [ValidateRange(60, 3600)]
    [int]$MaxDurationSec = 240,
    [switch]$InternalExecution,
    [string]$ResultsJsonPath = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-RedirectedTextFile {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $false)]
        [System.ConsoleColor]$ForegroundColor
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return
    }

    $text = ''
    for ($attempt = 1; $attempt -le 20; $attempt++) {
        try {
            $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
            try {
                $reader = New-Object System.IO.StreamReader($stream, [System.Text.Encoding]::UTF8, $true)
                try {
                    $text = $reader.ReadToEnd()
                }
                finally {
                    $reader.Dispose()
                }
            }
            finally {
                $stream.Dispose()
            }
            break
        }
        catch {
            if ($attempt -eq 20) {
                throw
            }
            Start-Sleep -Milliseconds 200
        }
    }

    if ([string]::IsNullOrWhiteSpace($text)) {
        return
    }

    $trimmed = $text.TrimEnd("`r", "`n")
    if ([string]::IsNullOrWhiteSpace($trimmed)) {
        return
    }

    if ($PSBoundParameters.ContainsKey('ForegroundColor')) {
        Write-Host $trimmed -ForegroundColor $ForegroundColor
        return
    }

    Write-Host $trimmed
}

if (-not $InternalExecution) {
    $shellPath = ''
    try {
        $shellPath = [string](Get-Process -Id $PID).Path
        $shellLeaf = [System.IO.Path]::GetFileName($shellPath)
        if ($shellLeaf -notmatch '^pwsh(\.exe)?$') {
            $shellPath = ''
        }
    }
    catch {
        $shellPath = ''
    }
    if ([string]::IsNullOrWhiteSpace($shellPath)) {
        $candidate = Get-Command pwsh -ErrorAction SilentlyContinue
        if ($candidate) {
            $shellPath = if (-not [string]::IsNullOrWhiteSpace([string]$candidate.Source)) { [string]$candidate.Source } else { [string]$candidate.Name }
        }
    }
    if ([string]::IsNullOrWhiteSpace($shellPath)) {
        throw 'Unable to determine a PowerShell 7 (pwsh) executable for timeout supervision.'
    }

    $stdoutPath = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-validate-out-" + [System.Guid]::NewGuid().ToString('N') + '.log')
    $stderrPath = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-validate-err-" + [System.Guid]::NewGuid().ToString('N') + '.log')
    try {
        $argumentList = @(
            '-NoProfile',
            '-ExecutionPolicy',
            'Bypass',
            '-File',
            $PSCommandPath,
            '-InternalExecution',
            '-MaxDurationSec',
            [string]$MaxDurationSec
        )
        if ($KeepArtifacts) {
            $argumentList += '-KeepArtifacts'
        }
        if ($SkipDockerPolicy) {
            $argumentList += '-SkipDockerPolicy'
        }
        if (-not [string]::IsNullOrWhiteSpace($ResultsJsonPath)) {
            $argumentList += @('-ResultsJsonPath', $ResultsJsonPath)
        }

        $process = Start-Process -FilePath $shellPath -ArgumentList $argumentList -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
        if (-not $process.WaitForExit($MaxDurationSec * 1000)) {
            try {
                $process.Kill()
            }
            catch {}

            Write-RedirectedTextFile -Path $stdoutPath
            Write-RedirectedTextFile -Path $stderrPath -ForegroundColor Yellow
            Write-Host ("validate-readiness.ps1 exceeded the {0}s limit." -f $MaxDurationSec) -ForegroundColor Red
            exit 1
        }

        $process.WaitForExit()
        Write-RedirectedTextFile -Path $stdoutPath
        Write-RedirectedTextFile -Path $stderrPath -ForegroundColor Yellow
        exit $process.ExitCode
    }
    finally {
        Remove-Item -LiteralPath $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue
    }
}

. (Join-Path $PSScriptRoot 'lib/runtime.ps1')

$script:ValidationResults = @()
$script:StartedAt = Get-Date
$script:DeadlineAt = $script:StartedAt.AddSeconds($MaxDurationSec)

function Write-Step {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Message
    )

    Write-Host ("==> {0}" -f $Message) -ForegroundColor Cyan
}

function Add-ValidationResult {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [bool]$Success,
        [Parameter(Mandatory = $true)]
        [string]$Detail
    )

    $script:ValidationResults += [pscustomobject]@{
        Name    = $Name
        Success = $Success
        Detail  = $Detail
    }
}

function Assert-WithinDeadline {
    param(
        [Parameter(Mandatory = $true)]
        [string]$StageName
    )

    if ((Get-Date) -gt $script:DeadlineAt) {
        throw ("validate-readiness.ps1 exceeded the {0}s limit during '{1}'." -f $MaxDurationSec, $StageName)
    }
}

function Get-FreeTcpPort {
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

function Invoke-DockerCommand {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    $output = & docker @Arguments 2>&1
    $exitCode = $LASTEXITCODE
    return [pscustomobject]@{
        ExitCode = $exitCode
        Output   = (($output | Out-String).Trim())
    }
}

$sourceContext = $null
$temporaryContext = $null
$validationRoot = $null
$sourceStateRoot = $null
$temporaryStateRoot = $null

try {
    $runId = "{0}-{1}" -f (Get-Date -Format 'yyyyMMddHHmmss'), ([System.Guid]::NewGuid().ToString('N').Substring(0, 8))
    $validationRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-readiness-{0}" -f $runId)
    $sourceStateRoot = Join-Path $validationRoot 'current-root-state'
    New-Item -ItemType Directory -Force -Path $validationRoot, $sourceStateRoot | Out-Null

    $sourceContext = New-McpAceContext -RootPath $PSScriptRoot -StateRoot $sourceStateRoot
    Assert-Prerequisites -Context $sourceContext

    Assert-WithinDeadline -StageName 'current-root-completeness'
    Write-Step 'Checking current manager root completeness'
    $portableLayout = Test-PortableManagerRootLayout -RootPath $sourceContext.ManagerRoot
    if (-not $portableLayout.Passed) {
        throw ("portable manager root is incomplete: {0}" -f ($portableLayout.MissingRequired -join ', '))
    }
    Add-ValidationResult -Name 'portable-layout/current-root' -Success $true -Detail 'required files are present'

    Assert-WithinDeadline -StageName 'live-regression-suite'
    Write-Step 'Running live regression suite on current manager root'
    $liveBoot = Ensure-StackRunning -Context $sourceContext
    if (-not ($liveBoot.ABPReady -and $liveBoot.HubReady)) {
        throw 'live boot did not make ABP and MCPace ready.'
    }
    Add-ValidationResult -Name 'live-regression/boot' -Success $true -Detail 'ABP and MCPace are ready'

    $liveStatuses = @(Get-HubServerStatuses -Context $sourceContext)
    $liveRequired = Get-RequiredServerConnectivity -Context $sourceContext -ServerStatuses $liveStatuses
    if ($liveRequired.Disconnected.Count -gt 0) {
        throw ("live required path is incomplete: {0}" -f ($liveRequired.Disconnected -join ', '))
    }
    Add-ValidationResult -Name 'live-regression/check' -Success $true -Detail 'required path is ready'

    $liveSmoke = Invoke-SmokeTest -Context $sourceContext
    if (-not $liveSmoke.Success) {
        throw $liveSmoke.Message
    }
    Add-ValidationResult -Name 'live-regression/smoke' -Success $true -Detail $liveSmoke.Message

    Assert-WithinDeadline -StageName 'external-multi-root-environment'
    Write-Step 'Creating external multi-root validation environment'
    $managerRoot = Join-Path $validationRoot 'manager'
    $primaryRoot = Join-Path $validationRoot 'project-app'
    $extraRoot = Join-Path $validationRoot 'project-docs'
    $temporaryStateRoot = Join-Path $validationRoot 'manager-state'
    New-Item -ItemType Directory -Force -Path $primaryRoot, $extraRoot, $temporaryStateRoot | Out-Null
    Set-Content -LiteralPath (Join-Path $primaryRoot 'workspace.txt') -Value 'primary workspace sentinel' -Encoding ASCII
    Set-Content -LiteralPath (Join-Path $extraRoot 'docs.txt') -Value 'extra workspace sentinel' -Encoding ASCII

    Copy-PortableManagerRoot -SourceRoot $sourceContext.RootPath -DestinationRoot $managerRoot
    Add-ValidationResult -Name 'portability/copy-manager-root' -Success $true -Detail $managerRoot

    $configPath = Join-Path $managerRoot 'mcpace.config.json'
    $config = Read-JsonFile -Path $configPath
    $config.ports.abp = Get-FreeTcpPort
    $config.ports.hub = Get-FreeTcpPort
    $config.hub.containerName = "mcpace-validation-$runId"
    $config.servers.'lean-ctx'.installer.autoInstall = $false
    $config.health.probeTimeoutSec = [Math]::Max([int]$config.health.probeTimeoutSec, 5)
    $config.health.startupTimeoutSec = [Math]::Max([int]$config.health.startupTimeoutSec, 180)
    $config.workspaces = [pscustomobject]@{
        primary = [pscustomobject]@{
            name = 'app'
            hostPath = $primaryRoot
            access = 'rw'
        }
        extras = @(
            [pscustomobject]@{
                name = 'readonly-docs'
                hostPath = $extraRoot
                access = 'ro'
            }
        )
    }
    Write-JsonFile -Path $configPath -Value $config

    $settingsPath = Join-Path $managerRoot 'mcp_settings.json'
    $settings = Read-JsonFile -Path $settingsPath
    $browserArgs = @()
    foreach ($arg in @($settings.mcpServers.browser.args)) {
        $argText = [string]$arg
        if ($argText -match '^http://host\.docker\.internal:\d+/mcp/?$') {
            $browserArgs += ("http://host.docker.internal:{0}/mcp" -f [int]$config.ports.abp)
        }
        else {
            $browserArgs += $argText
        }
    }
    $settings.mcpServers.browser.args = @($browserArgs)
    Write-JsonFile -Path $settingsPath -Value $settings

    $temporaryContext = New-McpAceContext -RootPath $managerRoot -StateRoot $temporaryStateRoot
    if ([System.IO.Path]::GetFullPath($temporaryContext.WorkspaceRegistry.Primary.HostPath) -ne [System.IO.Path]::GetFullPath($primaryRoot)) {
        throw 'temporary primary workspace host path was resolved incorrectly.'
    }
    if ($temporaryContext.WorkspaceRegistry.Extras.Count -ne 1) {
        throw 'temporary workspace registry did not create exactly one extra workspace.'
    }
    if (-not $temporaryContext.WorkspaceRegistry.Extras[0].ReadOnly) {
        throw 'temporary extra workspace is not read-only.'
    }
    Add-ValidationResult -Name 'portability/workspace-registry' -Success $true -Detail 'external primary and ro extra resolved correctly'

    Assert-WithinDeadline -StageName 'workspace-aware-effective-settings'
    Write-Step 'Checking workspace-aware effective settings'
    $effectiveSettings = Read-JsonFile -Path $temporaryContext.SettingsEffectivePath
    $filesystemArgs = @($effectiveSettings.mcpServers.filesystem.args | ForEach-Object { [string]$_ })
    foreach ($expectedPath in @('/workspace', '/workspaces/app', '/workspaces/readonly-docs')) {
        if ($filesystemArgs -notcontains $expectedPath) {
            throw ("filesystem args are missing '{0}'" -f $expectedPath)
        }
    }
    $serenaScript = (@($effectiveSettings.mcpServers.serena.args) | ForEach-Object { [string]$_ }) -join ' '
    if ([string]$effectiveSettings.mcpServers.serena.command -ne 'sh' -or $serenaScript -notmatch '/workspace') {
        throw 'serena was not wrapped with the primary workspace shell binding.'
    }
    $leanCtxScript = (@($effectiveSettings.mcpServers.'lean-ctx'.args) | ForEach-Object { [string]$_ }) -join ' '
    if ([string]$effectiveSettings.mcpServers.'lean-ctx'.command -ne 'sh') {
        throw 'lean-ctx was not wrapped with the shell binding.'
    }
    if ($leanCtxScript -notmatch '/workspace') {
        throw 'lean-ctx wrapper does not cd into /workspace.'
    }
    if ($leanCtxScript -notmatch '/app/data/server-state/lean-ctx') {
        throw 'lean-ctx wrapper does not isolate HOME under /app/data/server-state/lean-ctx.'
    }
    $gitArgs = (@($effectiveSettings.mcpServers.git.args) | ForEach-Object { [string]$_ }) -join ' '
    if ($gitArgs -notmatch '/app/data/git-repo') {
        throw 'git settings are not rooted in /app/data/git-repo.'
    }
    Add-ValidationResult -Name 'compatibility/effective-settings' -Success $true -Detail 'filesystem, serena, lean-ctx and git transforms are correct'

    Assert-WithinDeadline -StageName 'portability-suite'
    Write-Step 'Running portability suite in external manager root'
    $tempBoot = Ensure-StackRunning -Context $temporaryContext
    if (-not ($tempBoot.ABPReady -and $tempBoot.HubReady)) {
        throw 'temporary portability boot did not make ABP and MCPace ready.'
    }
    Add-ValidationResult -Name 'portability/boot' -Success $true -Detail 'temporary ABP and MCPace are ready'

    $tempStatuses = @(Get-HubServerStatuses -Context $temporaryContext)
    $tempRequired = Get-RequiredServerConnectivity -Context $temporaryContext -ServerStatuses $tempStatuses
    if ($tempRequired.Disconnected.Count -gt 0) {
        throw ("temporary required path is incomplete: {0}" -f ($tempRequired.Disconnected -join ', '))
    }
    Add-ValidationResult -Name 'portability/check' -Success $true -Detail 'temporary required path is ready'

    $tempSmoke = Invoke-SmokeTest -Context $temporaryContext
    if (-not $tempSmoke.Success) {
        throw $tempSmoke.Message
    }
    Add-ValidationResult -Name 'portability/smoke' -Success $true -Detail $tempSmoke.Message

    if (-not $SkipDockerPolicy) {
        Assert-WithinDeadline -StageName 'docker-mount-policy'
        Write-Step 'Validating Docker read-write and read-only mount policy'
        $rwProbePath = Join-Path $primaryRoot 'probe.txt'
        if (Test-Path -LiteralPath $rwProbePath) {
            Remove-Item -LiteralPath $rwProbePath -Force -ErrorAction SilentlyContinue
        }

        $rwCheck = Invoke-DockerCommand -Arguments @(
            'run',
            '--rm',
            '-v',
            ("{0}:/workspaces/app" -f $primaryRoot),
            $temporaryContext.HubImage,
            'sh',
            '-lc',
            'echo ok > /workspaces/app/probe.txt && cat /workspaces/app/probe.txt'
        )
        if ($rwCheck.ExitCode -ne 0) {
            throw ("rw mount probe failed: {0}" -f $rwCheck.Output)
        }
        if (-not (Test-Path -LiteralPath $rwProbePath)) {
            throw 'rw mount probe did not create probe.txt on the host.'
        }
        $rwContent = (Get-Content -LiteralPath $rwProbePath -Raw -Encoding UTF8).Trim()
        if ($rwContent -ne 'ok') {
            throw ("rw mount probe wrote unexpected content: {0}" -f $rwContent)
        }
        Add-ValidationResult -Name 'policy/rw-mount' -Success $true -Detail 'host file write succeeded'

        $roCheck = Invoke-DockerCommand -Arguments @(
            'run',
            '--rm',
            '-v',
            ("{0}:/workspaces/readonly-docs:ro" -f $extraRoot),
            $temporaryContext.HubImage,
            'sh',
            '-lc',
            'touch /workspaces/readonly-docs/probe.txt'
        )
        if ($roCheck.ExitCode -eq 0) {
            throw 'ro mount probe unexpectedly succeeded.'
        }
        if ($roCheck.Output -notmatch 'Read-only file system') {
            throw ("ro mount probe failed for an unexpected reason: {0}" -f $roCheck.Output)
        }
        Add-ValidationResult -Name 'policy/ro-mount' -Success $true -Detail 'docker blocked writes to read-only workspace'
    }
    else {
        Add-ValidationResult -Name 'policy/docker-mounts' -Success $true -Detail 'skipped by request'
    }
}
catch {
    if (-not ($script:ValidationResults | Where-Object { $_.Name -eq 'fatal' })) {
        Add-ValidationResult -Name 'fatal' -Success $false -Detail $_.Exception.Message
    }
}
finally {
    if ($temporaryContext) {
        try { Remove-Hub -Context $temporaryContext } catch {}
        try { Stop-ABP -Context $temporaryContext | Out-Null } catch {}
    }

    if ($validationRoot -and -not $KeepArtifacts) {
        try { Remove-Item -LiteralPath $validationRoot -Recurse -Force -ErrorAction SilentlyContinue } catch {}
    }
}

Write-Host ''
Write-Host ("Elapsed: {0:n1}s / limit {1}s" -f ((Get-Date) - $script:StartedAt).TotalSeconds, $MaxDurationSec)
Write-Host ''
Write-Host 'Validation results:'
foreach ($result in @($script:ValidationResults)) {
    $status = if ($result.Success) { 'OK' } else { 'FAIL' }
    $color = if ($result.Success) { 'Green' } else { 'Red' }
    Write-Host ("[{0}] {1} - {2}" -f $status, $result.Name, $result.Detail) -ForegroundColor $color
}

$failed = @($script:ValidationResults | Where-Object { -not $_.Success })

if (-not [string]::IsNullOrWhiteSpace($ResultsJsonPath)) {
    $catalogLookup = @{}
    foreach ($entry in @(Get-VerificationScenarioCatalog -Profile all)) {
        $catalogLookup[[string]$entry.scenarioId] = $entry
    }

    $structuredResults = @()
    foreach ($result in @($script:ValidationResults)) {
        $scenarioId = [string]$result.Name
        $metadata = if ($catalogLookup.ContainsKey($scenarioId)) { $catalogLookup[$scenarioId] } else { $null }
        $structuredResults += [pscustomobject]@{
            scenarioId    = $scenarioId
            phase         = if ($metadata) { [string]$metadata.phase } else { 'readiness' }
            scope         = if ($metadata) { [string]$metadata.scope } else { 'current-host' }
            preconditions = if ($metadata) { @($metadata.preconditions) } else { @() }
            command       = if ($metadata) { [string]$metadata.command } else { 'validate-readiness.ps1 internal step' }
            expected      = if ($metadata) { [string]$metadata.expected } else { 'Step completes successfully.' }
            actual        = [string]$result.Detail
            artifacts     = @()
            verdict       = if ($result.Success) { 'pass' } else { 'fail' }
            severity      = if ($metadata) { [string]$metadata.severity } else { 'blocker' }
            durationMs    = 0
        }
    }

    $payload = [pscustomobject]@{
        schemaVersion = '1.0'
        generatedAt   = (Get-Date).ToString('o')
        maxDurationSec = $MaxDurationSec
        durationMs    = [int][Math]::Round(((Get-Date) - $script:StartedAt).TotalMilliseconds)
        passed        = ($failed.Count -eq 0)
        results       = $structuredResults
    }

    $resolvedResultsPath = $ResultsJsonPath
    if (-not [System.IO.Path]::IsPathRooted($resolvedResultsPath)) {
        $resolvedResultsPath = Join-Path $PSScriptRoot $resolvedResultsPath
    }
    $resultsDir = Split-Path -Parent $resolvedResultsPath
    if (-not [string]::IsNullOrWhiteSpace($resultsDir)) {
        New-Item -ItemType Directory -Force -Path $resultsDir | Out-Null
    }
    Write-JsonFile -Path $resolvedResultsPath -Value $payload
}

Write-Host ''
if ($failed.Count -eq 0) {
    Write-Host 'Readiness validation passed on this Windows host for live stack, external multi-root portability and Docker mount policy.' -ForegroundColor Green
    Write-Host 'Still not proven automatically: brand-new clean host provisioning and Linux/macOS end-to-end smoke.' -ForegroundColor Yellow
    exit 0
}

Write-Host 'Readiness validation failed.' -ForegroundColor Red
Write-Host 'Still not proven automatically: brand-new clean host provisioning and Linux/macOS end-to-end smoke.' -ForegroundColor Yellow
exit 1
