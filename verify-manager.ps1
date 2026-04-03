#requires -Version 7.0
[CmdletBinding()]
param(
    [ValidateSet('standard', 'full', 'ci-runtime')]
    [string]$Profile = 'standard',
    [string]$JsonOutputPath = 'reports\verification-latest.json',
    [string]$MarkdownOutputPath = 'reports\verification-latest.md',
    [switch]$ListScenarios,
    [switch]$IncludeLiveDestructive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'lib/runtime.ps1')

$catalog = @(Get-VerificationScenarioCatalog -Profile $Profile)
if ($ListScenarios) {
    $catalog | ConvertTo-Json -Depth 10
    exit 0
}

$catalogLookup = @{}
foreach ($entry in @($catalog)) {
    $catalogLookup[[string]$entry.scenarioId] = $entry
}

function New-ScenarioResult {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$ScenarioId,
        [Parameter(Mandatory = $true)][string]$Verdict,
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Actual,
        [Parameter(Mandatory = $false)][AllowEmptyCollection()][string[]]$Artifacts = @(),
        [Parameter(Mandatory = $false)][long]$DurationMs = 0,
        [Parameter(Mandatory = $false)][string]$Severity = '',
        [Parameter(Mandatory = $false)][string]$Phase = '',
        [Parameter(Mandatory = $false)][string]$Scope = '',
        [Parameter(Mandatory = $false)][AllowEmptyCollection()][string[]]$Preconditions = @(),
        [Parameter(Mandatory = $false)][string]$Command = '',
        [Parameter(Mandatory = $false)][string]$Expected = ''
    )

    $metadata = if ($catalogLookup.ContainsKey($ScenarioId)) { $catalogLookup[$ScenarioId] } else { $null }

    return [pscustomobject]@{
        scenarioId    = $ScenarioId
        phase         = if (-not [string]::IsNullOrWhiteSpace($Phase)) { $Phase } elseif ($metadata) { [string]$metadata.phase } else { 'misc' }
        scope         = if (-not [string]::IsNullOrWhiteSpace($Scope)) { $Scope } elseif ($metadata) { [string]$metadata.scope } else { 'current-host' }
        preconditions = if ($Preconditions.Count -gt 0) { @($Preconditions) } elseif ($metadata) { @($metadata.preconditions) } else { @() }
        command       = if (-not [string]::IsNullOrWhiteSpace($Command)) { $Command } elseif ($metadata) { [string]$metadata.command } else { '' }
        expected      = if (-not [string]::IsNullOrWhiteSpace($Expected)) { $Expected } elseif ($metadata) { [string]$metadata.expected } else { '' }
        actual        = $Actual
        artifacts     = @($Artifacts)
        verdict       = $Verdict
        severity      = if (-not [string]::IsNullOrWhiteSpace($Severity)) { $Severity } elseif ($metadata) { [string]$metadata.severity } else { 'medium' }
        durationMs    = $DurationMs
    }
}

function Write-ScenarioProgress {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Result
    )

    $color = switch ([string]$Result.verdict) {
        'pass' { 'Green' }
        'warn' { 'Yellow' }
        'not-proven' { 'Yellow' }
        'not-applicable' { 'DarkGray' }
        'skipped' { 'DarkGray' }
        default { 'Red' }
    }

    $text = [string]$Result.actual
    if ($text.Length -gt 180) {
        $text = $text.Substring(0, 177) + '...'
    }

    Write-Host ("[{0}] {1} - {2}" -f ([string]$Result.verdict).ToUpperInvariant(), [string]$Result.scenarioId, $text) -ForegroundColor $color
}

function Invoke-ExternalProcessCapture {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $false)][AllowEmptyCollection()][string[]]$ArgumentList = @(),
        [Parameter(Mandatory = $false)][string]$WorkingDirectory = $PSScriptRoot,
        [Parameter(Mandatory = $false)][int]$TimeoutSec = 900
    )

    $stdoutPath = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-verify-out-" + [guid]::NewGuid().ToString('N') + '.log')
    $stderrPath = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-verify-err-" + [guid]::NewGuid().ToString('N') + '.log')
    $startedAt = Get-Date
    try {
        $process = Start-Process -FilePath $FilePath -ArgumentList $ArgumentList -WorkingDirectory $WorkingDirectory -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
        if (-not $process.WaitForExit($TimeoutSec * 1000)) {
            try { $process.Kill() } catch {}
            return [pscustomobject]@{
                ExitCode   = -1
                TimedOut   = $true
                StdOut     = if (Test-Path -LiteralPath $stdoutPath) { Get-Content -LiteralPath $stdoutPath -Raw -Encoding UTF8 } else { '' }
                StdErr     = if (Test-Path -LiteralPath $stderrPath) { Get-Content -LiteralPath $stderrPath -Raw -Encoding UTF8 } else { '' }
                DurationMs = [int][Math]::Round(((Get-Date) - $startedAt).TotalMilliseconds)
            }
        }

        $process.WaitForExit()
        return [pscustomobject]@{
            ExitCode   = [int]$process.ExitCode
            TimedOut   = $false
            StdOut     = if (Test-Path -LiteralPath $stdoutPath) { Get-Content -LiteralPath $stdoutPath -Raw -Encoding UTF8 } else { '' }
            StdErr     = if (Test-Path -LiteralPath $stderrPath) { Get-Content -LiteralPath $stderrPath -Raw -Encoding UTF8 } else { '' }
            DurationMs = [int][Math]::Round(((Get-Date) - $startedAt).TotalMilliseconds)
        }
    }
    finally {
        Remove-Item -LiteralPath $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue
    }
}

function Get-CommandDirectory {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$CommandName
    )

    $command = Get-Command $CommandName -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $command) {
        return ''
    }

    $path = if (-not [string]::IsNullOrWhiteSpace([string]$command.Source)) { [string]$command.Source } else { [string]$command.Path }
    if ([string]::IsNullOrWhiteSpace($path)) {
        return ''
    }

    return (Split-Path -Parent $path)
}

function Get-MinimalPath {
    [CmdletBinding()]
    param(
        [switch]$IncludeNode,
        [switch]$IncludeDocker
    )

    $dirs = New-Object System.Collections.Generic.List[string]
    $pwshDir = Get-CommandDirectory -CommandName 'pwsh'
    if (-not [string]::IsNullOrWhiteSpace($pwshDir) -and -not $dirs.Contains($pwshDir)) {
        $dirs.Add($pwshDir)
    }

    if ($IncludeNode) {
        foreach ($name in @('node', 'npx')) {
            $dir = Get-CommandDirectory -CommandName $name
            if (-not [string]::IsNullOrWhiteSpace($dir) -and -not $dirs.Contains($dir)) {
                $dirs.Add($dir)
            }
        }
    }

    if ($IncludeDocker) {
        $dockerDir = Get-CommandDirectory -CommandName 'docker'
        if (-not [string]::IsNullOrWhiteSpace($dockerDir) -and -not $dirs.Contains($dockerDir)) {
            $dirs.Add($dockerDir)
        }
    }

    foreach ($systemDir in @($env:SystemRoot, (Join-Path $env:SystemRoot 'System32'))) {
        if (-not [string]::IsNullOrWhiteSpace($systemDir) -and -not $dirs.Contains($systemDir)) {
            $dirs.Add($systemDir)
        }
    }

    return ($dirs -join [System.IO.Path]::PathSeparator)
}

function Get-CommandCaptureSummary {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Capture
    )

    $parts = @()
    if ($Capture.TimedOut) {
        $parts += 'timed out'
    }
    $parts += ("exit={0}" -f [int]$Capture.ExitCode)

    $stdoutText = if ($null -eq $Capture.StdOut) { '' } else { ([string]$Capture.StdOut).Trim() }
    $stderrText = if ($null -eq $Capture.StdErr) { '' } else { ([string]$Capture.StdErr).Trim() }
    $text = @(@($stdoutText, $stderrText) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($text.Count -gt 0) {
        $combined = ($text -join ' | ').Trim()
        if ($combined.Length -gt 260) {
            $combined = $combined.Substring(0, 257) + '...'
        }
        $parts += $combined
    }

    return ($parts -join '; ')
}

function Invoke-PwshCapture {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)][AllowEmptyCollection()][string[]]$ArgumentList = @(),
        [Parameter(Mandatory = $false)][string]$WorkingDirectory = $PSScriptRoot,
        [Parameter(Mandatory = $false)][int]$TimeoutSec = 900
    )

    $pwsh = Get-Command pwsh -ErrorAction Stop
    $path = if (-not [string]::IsNullOrWhiteSpace([string]$pwsh.Source)) { [string]$pwsh.Source } else { [string]$pwsh.Path }
    return Invoke-ExternalProcessCapture -FilePath $path -ArgumentList $ArgumentList -WorkingDirectory $WorkingDirectory -TimeoutSec $TimeoutSec
}

function Invoke-WindowsPowerShellCapture {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)][AllowEmptyCollection()][string[]]$ArgumentList = @(),
        [Parameter(Mandatory = $false)][string]$WorkingDirectory = $PSScriptRoot,
        [Parameter(Mandatory = $false)][int]$TimeoutSec = 900
    )

    $windowsPowerShellPath = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    if (-not (Test-Path -LiteralPath $windowsPowerShellPath)) {
        throw "Missing Windows PowerShell executable: $windowsPowerShellPath"
    }

    return Invoke-ExternalProcessCapture -FilePath $windowsPowerShellPath -ArgumentList $ArgumentList -WorkingDirectory $WorkingDirectory -TimeoutSec $TimeoutSec
}

function Invoke-VerificationMcpRequest {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [Parameter(Mandatory = $true)]$Body,
        [Parameter(Mandatory = $true)][string]$Token,
        [Parameter(Mandatory = $false)][AllowEmptyString()][string]$SessionId = '',
        [Parameter(Mandatory = $false)][int]$TimeoutSec = 30
    )

    $headers = @{
        Authorization = "Bearer $Token"
        Accept = 'application/json, text/event-stream'
        'Content-Type' = 'application/json'
    }
    if (-not [string]::IsNullOrWhiteSpace($SessionId)) {
        $headers['mcp-session-id'] = $SessionId
    }

    $bodyJson = $Body | ConvertTo-Json -Depth 20 -Compress
    $response = Invoke-WebRequest -Uri $Url -Method Post -Headers $headers -Body $bodyJson -TimeoutSec $TimeoutSec -UseBasicParsing -ErrorAction Stop
    $returnedSessionId = ''
    if ($null -ne $response.Headers -and $response.Headers['mcp-session-id']) {
        $returnedSessionId = [string]$response.Headers['mcp-session-id']
    }

    return [pscustomobject]@{
        StatusCode = [int]$response.StatusCode
        SessionId  = if (-not [string]::IsNullOrWhiteSpace($returnedSessionId)) { $returnedSessionId } else { $SessionId }
        Raw        = [string]$response.Content
        Payload    = Parse-McpResponsePayload -Text ([string]$response.Content)
    }
}

function New-IsolatedVerificationEnvironment {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)][AllowEmptyCollection()][string[]]$EnableServers = @(),
        [Parameter(Mandatory = $false)][switch]$DisableLeanCtx = $true
    )

    $runId = ([guid]::NewGuid().ToString('N'))
    $root = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-verify-{0}" -f $runId)
    $managerRoot = Join-Path $root 'manager'
    $primaryRoot = Join-Path $root 'workspace-app'
    $extraRoot = Join-Path $root 'workspace-docs'
    $stateRoot = Join-Path $root 'state'
    New-Item -ItemType Directory -Force -Path $root, $primaryRoot, $extraRoot, $stateRoot | Out-Null
    Set-Content -LiteralPath (Join-Path $primaryRoot 'workspace.txt') -Value 'verification primary workspace' -Encoding ASCII
    Set-Content -LiteralPath (Join-Path $extraRoot 'docs.txt') -Value 'verification extra workspace' -Encoding ASCII

    Copy-PortableManagerRoot -SourceRoot $PSScriptRoot -DestinationRoot $managerRoot

    $configPath = Join-Path $managerRoot 'mcpace.config.json'
    $config = Read-JsonFile -Path $configPath
    $config.ports.abp = Get-FreeTcpPort
    $config.ports.hub = Get-FreeTcpPort
    $config.hub.containerName = "mcpace-verify-$runId"
    $config.health.probeTimeoutSec = [Math]::Max([int]$config.health.probeTimeoutSec, 5)
    $config.health.startupTimeoutSec = [Math]::Max([int]$config.health.startupTimeoutSec, 180)
    if ($DisableLeanCtx -and $config.servers.'lean-ctx') {
        $config.servers.'lean-ctx'.installer.autoInstall = $false
    }
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

    if ($DisableLeanCtx -or $EnableServers.Count -gt 0) {
        $serverMap = [ordered]@{}
        if ($DisableLeanCtx) {
            $serverMap['lean-ctx'] = [pscustomobject]@{ enabled = $false }
        }
        foreach ($name in @($EnableServers)) {
            $serverMap[[string]$name] = [pscustomobject]@{ enabled = $true }
        }
        $overrides = [pscustomobject]@{
            mcpServers = [pscustomobject]$serverMap
        }
        $localOverridesPath = Join-Path $stateRoot 'data\runtime\mcp_settings.local-overrides.json'
        Write-LocalServerOverrides -Path $localOverridesPath -Overrides $overrides | Out-Null
    }

    $context = New-McpAceContext -RootPath $managerRoot -StateRoot $stateRoot
    return [pscustomobject]@{
        Root        = $root
        ManagerRoot = $managerRoot
        StateRoot   = $stateRoot
        PrimaryRoot = $primaryRoot
        ExtraRoot   = $extraRoot
        Context     = $context
    }
}

function Remove-IsolatedVerificationEnvironment {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Environment
    )

    if ($Environment.Context) {
        try { Remove-Hub -Context $Environment.Context } catch {}
        try { Stop-ABP -Context $Environment.Context | Out-Null } catch {}
    }
    if ($Environment.Root -and (Test-Path -LiteralPath $Environment.Root)) {
        Remove-Item -LiteralPath $Environment.Root -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Add-Result {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Collection,
        [Parameter(Mandatory = $true)]$Result
    )

    $null = $Collection.Add($Result)
    Write-ScenarioProgress -Result $Result
}

$results = New-Object System.Collections.Generic.List[object]
$artifactsRoot = Join-Path $PSScriptRoot 'reports'
New-Item -ItemType Directory -Force -Path $artifactsRoot | Out-Null

$pwshVersionText = ''
try { $pwshVersionText = (& pwsh -NoProfile -Command '$PSVersionTable.PSVersion.ToString()' 2>$null).Trim() } catch { $pwshVersionText = '' }
$nodeVersionText = ''
try { $nodeVersionText = (& node --version 2>$null).Trim() } catch { $nodeVersionText = '' }
$environment = @{
    os = [System.Runtime.InteropServices.RuntimeInformation]::OSDescription
    pwsh = $pwshVersionText
    node = $nodeVersionText
    dockerReady = (Test-DockerReady)
}

$context = $null
try {
    $context = New-McpAceContext -RootPath $PSScriptRoot
}
catch {
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'bootstrap/windows-bypass-check' -Verdict 'fail' -Actual $_.Exception.Message -DurationMs 0)
    $report = New-VerificationReport -Profile $Profile -Results $results.ToArray() -Environment $environment
    $markdown = ConvertTo-VerificationMarkdownReport -Report $report
    $jsonResolved = if ([System.IO.Path]::IsPathRooted($JsonOutputPath)) { $JsonOutputPath } else { Join-Path $PSScriptRoot $JsonOutputPath }
    $markdownResolved = if ([System.IO.Path]::IsPathRooted($MarkdownOutputPath)) { $MarkdownOutputPath } else { Join-Path $PSScriptRoot $MarkdownOutputPath }
    Write-JsonFile -Path $jsonResolved -Value $report
    Set-Content -LiteralPath $markdownResolved -Value $markdown -Encoding UTF8
    exit 1
}

# source / bootstrap
$pesterCapture = Invoke-PwshCapture -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', 'Invoke-Pester -CI -Path ./tests')
Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'source/pester-suite' -Verdict $(if (-not $pesterCapture.TimedOut -and $pesterCapture.ExitCode -eq 0) { 'pass' } else { 'fail' }) -Actual (Get-CommandCaptureSummary -Capture $pesterCapture) -DurationMs $pesterCapture.DurationMs)

if ($context.IsWindows) {
    $directCheckCapture = Invoke-PwshCapture -ArgumentList @('-NoProfile', '-File', '.\check.ps1')
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'bootstrap/windows-direct-check' -Verdict $(if (-not $directCheckCapture.TimedOut -and $directCheckCapture.ExitCode -eq 0) { 'pass' } else { 'fail' }) -Actual (Get-CommandCaptureSummary -Capture $directCheckCapture) -DurationMs $directCheckCapture.DurationMs)
}
else {
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'bootstrap/windows-direct-check' -Verdict 'not-applicable' -Actual 'Scenario applies only to Windows hosts.' -DurationMs 0)
}

$bypassCheckCapture = Invoke-PwshCapture -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', '.\check.ps1')
Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'bootstrap/windows-bypass-check' -Verdict $(if (-not $bypassCheckCapture.TimedOut -and $bypassCheckCapture.ExitCode -eq 0) { 'pass' } else { 'fail' }) -Actual (Get-CommandCaptureSummary -Capture $bypassCheckCapture) -DurationMs $bypassCheckCapture.DurationMs)

$missingDockerCommand = @'
$env:PATH = '{0}'
. (Join-Path '{1}' 'lib/runtime.ps1')
$ctx = New-McpAceContext -RootPath '{1}'
try {{
    Assert-Prerequisites -Context $ctx
    Write-Output 'unexpected success'
    exit 2
}}
catch {{
    Write-Output $_.Exception.Message
    exit 0
}}
'@ -f (Get-MinimalPath -IncludeNode), $PSScriptRoot
$missingDockerCapture = Invoke-PwshCapture -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', $missingDockerCommand)
$missingDockerVerdict = if (-not $missingDockerCapture.TimedOut -and $missingDockerCapture.ExitCode -eq 0 -and (($missingDockerCapture.StdOut + $missingDockerCapture.StdErr) -match 'Required command is missing: docker')) { 'pass' } else { 'fail' }
Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'bootstrap/missing-docker' -Verdict $missingDockerVerdict -Actual (Get-CommandCaptureSummary -Capture $missingDockerCapture) -DurationMs $missingDockerCapture.DurationMs)

$missingNodeCommand = @'
$env:PATH = '{0}'
. (Join-Path '{1}' 'lib/runtime.ps1')
$ctx = New-McpAceContext -RootPath '{1}'
try {{
    Assert-Prerequisites -Context $ctx
    Write-Output 'unexpected success'
    exit 2
}}
catch {{
    Write-Output $_.Exception.Message
    exit 0
}}
'@ -f (Get-MinimalPath -IncludeDocker), $PSScriptRoot
$missingNodeCapture = Invoke-PwshCapture -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', $missingNodeCommand)
$missingNodeVerdict = if (-not $missingNodeCapture.TimedOut -and $missingNodeCapture.ExitCode -eq 0 -and (($missingNodeCapture.StdOut + $missingNodeCapture.StdErr) -match 'Required command is missing: node')) { 'pass' } else { 'fail' }
Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'bootstrap/missing-node' -Verdict $missingNodeVerdict -Actual (Get-CommandCaptureSummary -Capture $missingNodeCapture) -DurationMs $missingNodeCapture.DurationMs)

if ($context.IsWindows) {
    $nonPwshCapture = Invoke-WindowsPowerShellCapture -ArgumentList @('-NoProfile', '-File', '.\boot.ps1')
    $nonPwshVerdict = if ($nonPwshCapture.ExitCode -ne 0) { 'pass' } else { 'fail' }
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'bootstrap/non-pwsh-shell' -Verdict $nonPwshVerdict -Actual (Get-CommandCaptureSummary -Capture $nonPwshCapture) -DurationMs $nonPwshCapture.DurationMs)
}
else {
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'bootstrap/non-pwsh-shell' -Verdict 'not-applicable' -Actual 'Scenario applies only to Windows hosts.' -DurationMs 0)
}

# lifecycle
$bootCapture = Invoke-PwshCapture -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', '.\boot.ps1')
Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'lifecycle/boot-idempotent' -Verdict $(if (-not $bootCapture.TimedOut -and $bootCapture.ExitCode -eq 0) { 'pass' } else { 'fail' }) -Actual (Get-CommandCaptureSummary -Capture $bootCapture) -DurationMs $bootCapture.DurationMs)

$checkCapture = Invoke-PwshCapture -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', '.\check.ps1')
Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'lifecycle/check' -Verdict $(if (-not $checkCapture.TimedOut -and $checkCapture.ExitCode -eq 0) { 'pass' } else { 'fail' }) -Actual (Get-CommandCaptureSummary -Capture $checkCapture) -DurationMs $checkCapture.DurationMs)

$smokeCapture = Invoke-PwshCapture -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', '.\smoke-test.ps1')
Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'lifecycle/smoke' -Verdict $(if (-not $smokeCapture.TimedOut -and $smokeCapture.ExitCode -eq 0) { 'pass' } else { 'fail' }) -Actual (Get-CommandCaptureSummary -Capture $smokeCapture) -DurationMs $smokeCapture.DurationMs)

$readinessResultsPath = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-readiness-results-" + [guid]::NewGuid().ToString('N') + '.json')
$readinessCapture = Invoke-PwshCapture -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', '.\validate-readiness.ps1', '-ResultsJsonPath', $readinessResultsPath)
$readinessVerdict = if (-not $readinessCapture.TimedOut -and $readinessCapture.ExitCode -eq 0) { 'pass' } else { 'fail' }
$readinessActual = Get-CommandCaptureSummary -Capture $readinessCapture
Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'lifecycle/readiness' -Verdict $readinessVerdict -Actual $readinessActual -Artifacts @($readinessResultsPath) -DurationMs $readinessCapture.DurationMs)
if (Test-Path -LiteralPath $readinessResultsPath) {
    try {
        $readinessPayload = Read-JsonFile -Path $readinessResultsPath
        if ($readinessPayload.passed) {
            $results[$results.Count - 1] = New-ScenarioResult -ScenarioId 'lifecycle/readiness' -Verdict 'pass' -Actual $readinessActual -Artifacts @($readinessResultsPath) -DurationMs $readinessCapture.DurationMs
        }
        foreach ($result in @($readinessPayload.results)) {
            if ($catalogLookup.ContainsKey([string]$result.scenarioId)) {
                Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId ([string]$result.scenarioId) -Verdict ([string]$result.verdict) -Actual ([string]$result.actual) -Artifacts @($readinessResultsPath) -DurationMs ([long]$result.durationMs))
            }
        }
    }
    catch {
        Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'lifecycle/readiness' -Verdict 'warn' -Actual ("Failed to parse structured readiness results: {0}" -f $_.Exception.Message) -Artifacts @($readinessResultsPath) -DurationMs 0)
    }
}

# clients / direct API
$context = New-McpAceContext -RootPath $PSScriptRoot
$currentEnsure = Ensure-StackRunning -Context $context
$context = $currentEnsure.Context
$launcherConfig = Get-ClientConfigJson -Context $context | ConvertFrom-Json
$launcherCommand = [string]$launcherConfig.mcpServers.mcpace.command
$launcherVerdict = if ($launcherCommand -match 'mcpace\.(cmd|sh)$' -and @($launcherConfig.mcpServers.mcpace.args).Count -eq 0) { 'pass' } else { 'fail' }
Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'clients/launcher-config' -Verdict $launcherVerdict -Actual ("command={0}; args={1}" -f $launcherCommand, @($launcherConfig.mcpServers.mcpace.args).Count) -DurationMs 0)

$editorEnvironment = $null
try {
    $editorEnvironment = New-IsolatedVerificationEnvironment
    $editorCapture = Invoke-PwshCapture -WorkingDirectory $editorEnvironment.ManagerRoot -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', '.\setup-mcp-clients.ps1', '-Overwrite')
    $editorVerdict = 'fail'
    $editorActual = Get-CommandCaptureSummary -Capture $editorCapture
    $editorProfilePath = Join-Path $editorEnvironment.ManagerRoot '.vscode\mcp.json'
    if (-not $editorCapture.TimedOut -and $editorCapture.ExitCode -eq 0 -and (Test-Path -LiteralPath $editorProfilePath)) {
        $editorProfile = Get-Content -LiteralPath $editorProfilePath -Raw -Encoding UTF8 | ConvertFrom-Json
        $profileCommand = [string]$editorProfile.servers.mcpace.command
        if ($profileCommand -match 'mcpace\.(cmd|sh)$' -and @($editorProfile.servers.mcpace.args).Count -eq 0) {
            $editorVerdict = 'pass'
            $editorActual = ("command={0}; args={1}" -f $profileCommand, @($editorProfile.servers.mcpace.args).Count)
        }
    }
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'clients/editor-profile' -Verdict $editorVerdict -Actual $editorActual -Artifacts @($editorProfilePath) -DurationMs $editorCapture.DurationMs)
}
finally {
    if ($editorEnvironment) {
        Remove-IsolatedVerificationEnvironment -Environment $editorEnvironment
    }
}

try {
    $healthResponse = Invoke-WebRequest -Uri "http://127.0.0.1:$($context.HubPort)/health" -Method Get -TimeoutSec ([Math]::Max($context.ProbeTimeoutSec, 5)) -UseBasicParsing -ErrorAction Stop
    $healthPayload = Parse-McpResponsePayload -Text ([string]$healthResponse.Content)
    $healthVerdict = if ([int]$healthResponse.StatusCode -eq 200 -and @('healthy', 'degraded') -contains [string]$healthPayload.status) { 'pass' } else { 'fail' }
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'clients/api-health' -Verdict $healthVerdict -Actual ("status={0}; health={1}" -f [int]$healthResponse.StatusCode, [string]$healthPayload.status) -DurationMs 0)
}
catch {
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'clients/api-health' -Verdict 'fail' -Actual $_.Exception.Message -DurationMs 0)
}

try {
    $headers = @{ Authorization = "Bearer $($context.BearerToken)" }
    $serversPayload = $null
    $serverApiError = ''
    for ($attempt = 1; $attempt -le 3; $attempt++) {
        try {
            $serversPayload = Invoke-RestMethod -Uri "http://127.0.0.1:$($context.HubPort)/api/servers" -Headers $headers -TimeoutSec 30 -ErrorAction Stop
            break
        }
        catch {
            $serverApiError = [string]$_.Exception.Message
            Start-Sleep -Seconds 2
        }
    }
    if ($null -eq $serversPayload -or $null -eq $serversPayload.data) {
        throw $serverApiError
    }
    $serverCount = @($serversPayload.data).Count
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'clients/api-servers' -Verdict 'pass' -Actual ("server-count={0}" -f $serverCount) -DurationMs 0)
}
catch {
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'clients/api-servers' -Verdict 'fail' -Actual $_.Exception.Message -DurationMs 0)
}

try {
    $smokeResult = Invoke-SmokeTest -Context $context
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'clients/mcp-session' -Verdict $(if ($smokeResult.Success) { 'pass' } else { 'fail' }) -Actual ([string]$smokeResult.Message) -DurationMs 0)
}
catch {
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'clients/mcp-session' -Verdict 'fail' -Actual $_.Exception.Message -DurationMs 0)
}

# servers
$abpState = Get-ABPState -Context $context
$serverStatuses = @(Get-HubServerStatuses -Context $context)
$requiredConnectivity = Get-RequiredServerConnectivity -Context $context -ServerStatuses $serverStatuses
$requiredVerdict = if ($abpState.State -eq 'running' -and $requiredConnectivity.Disconnected.Count -eq 0 -and $requiredConnectivity.Required.Count -gt 0) { 'pass' } else { 'fail' }
Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'servers/required-path' -Verdict $requiredVerdict -Actual ("abp={0}; connected={1}; missing={2}" -f [string]$abpState.State, ($requiredConnectivity.Connected -join ','), $(if ($requiredConnectivity.Disconnected.Count -gt 0) { $requiredConnectivity.Disconnected -join ',' } else { 'none' })) -DurationMs 0)

$freshStateRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-verify-state-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $freshStateRoot | Out-Null
try {
    $baselineContext = New-McpAceContext -RootPath $PSScriptRoot -StateRoot $freshStateRoot
    $defaultDisabledEntries = @($baselineContext.ServerRuntime | Where-Object { -not $_.Required -and -not $_.SourceEnabled })
    $optionalDisabledProblems = @($defaultDisabledEntries | Where-Object { [bool]$_.EffectiveEnabled -and [string]::IsNullOrWhiteSpace([string]$_.DisabledReason) })
    $optionalDisabledVerdict = if ($optionalDisabledProblems.Count -eq 0) { 'pass' } else { 'fail' }
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'servers/optional-default-disabled' -Verdict $optionalDisabledVerdict -Actual ("count={0}; problematic={1}" -f $defaultDisabledEntries.Count, $(if ($optionalDisabledProblems.Count -gt 0) { ($optionalDisabledProblems | ForEach-Object { $_.Name }) -join ',' } else { 'none' })) -DurationMs 0)

    $sourceEnabledOptionalEntries = @($baselineContext.ServerRuntime | Where-Object { -not $_.Required -and $_.SourceEnabled })
    $sourceEnabledVerdict = if ($sourceEnabledOptionalEntries.Count -gt 0) { 'pass' } else { 'warn' }
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'servers/optional-source-enabled' -Verdict $sourceEnabledVerdict -Actual ("entries={0}" -f (($sourceEnabledOptionalEntries | ForEach-Object { "{0}:{1}/{2}" -f $_.Name, $_.ConfiguredEnabled, $_.EffectiveEnabled }) -join ', ')) -DurationMs 0)
}
finally {
    Remove-Item -LiteralPath $freshStateRoot -Recurse -Force -ErrorAction SilentlyContinue
}

foreach ($optionalServerName in @('github', 'git', 'sentry', 'windows-mcp', 'screenpipe', 'firecrawl')) {
    $scenarioId = "servers/optional-user-enabled/$optionalServerName"
    if (-not $catalogLookup.ContainsKey($scenarioId)) {
        continue
    }

    $optionalEnvironment = $null
    try {
        $optionalEnvironment = New-IsolatedVerificationEnvironment -EnableServers @($optionalServerName)
        $optionalContext = $optionalEnvironment.Context
        $entry = Get-ServerRuntimeEntry -Context $optionalContext -Name $optionalServerName
        if (-not $entry) {
            Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId $scenarioId -Verdict 'fail' -Actual 'Optional server entry not found in isolated context.' -DurationMs 0)
            continue
        }

        if (-not [bool]$entry.ConfiguredEnabled) {
            Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId $scenarioId -Verdict 'fail' -Actual 'Configured enablement was not preserved after applying the user override.' -DurationMs 0)
            continue
        }

        if (-not [bool]$entry.EffectiveEnabled) {
            $verdict = if (-not [string]::IsNullOrWhiteSpace([string]$entry.DisabledReason)) { 'pass' } else { 'fail' }
            Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId $scenarioId -Verdict $verdict -Actual ("runtime-gated: {0}" -f [string]$entry.DisabledReason) -DurationMs 0)
            continue
        }

        $started = Get-Date
        $bootResult = Ensure-StackRunning -Context $optionalContext
        $optionalContext = $bootResult.Context
        $optionalStatuses = @(Get-HubServerStatuses -Context $optionalContext)
        $optionalStatus = @($optionalStatuses | Where-Object { [string]$_.name -eq $optionalServerName } | Select-Object -First 1)
        $optionalVerdict = if ($optionalStatus -and [string]$optionalStatus.status -eq 'connected') { 'pass' } else { 'fail' }
        Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId $scenarioId -Verdict $optionalVerdict -Actual ("effectiveEnabled=True; status={0}" -f $(if ($optionalStatus) { [string]$optionalStatus.status } else { 'missing' })) -DurationMs ([int][Math]::Round(((Get-Date) - $started).TotalMilliseconds)))
    }
    catch {
        Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId $scenarioId -Verdict 'fail' -Actual $_.Exception.Message -DurationMs 0)
    }
    finally {
        if ($optionalEnvironment) {
            Remove-IsolatedVerificationEnvironment -Environment $optionalEnvironment
        }
    }
}

# persistence
$tempStateRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-verify-persistence-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $tempStateRoot | Out-Null
$oldStateRoot = [Environment]::GetEnvironmentVariable('MCPACE_STATE_ROOT')
$oldBearer = [Environment]::GetEnvironmentVariable('MCPACE_BEARER_TOKEN')
$oldBcrypt = [Environment]::GetEnvironmentVariable('MCPACE_ADMIN_PASSWORD_BCRYPT')
try {
    [Environment]::SetEnvironmentVariable('MCPACE_STATE_ROOT', $tempStateRoot)
    [Environment]::SetEnvironmentVariable('MCPACE_BEARER_TOKEN', $null)
    [Environment]::SetEnvironmentVariable('MCPACE_ADMIN_PASSWORD_BCRYPT', $null)

    $authContext = New-McpAceContext -RootPath $PSScriptRoot -StateRoot $tempStateRoot
    $authVerdict = if ((Test-Path -LiteralPath $authContext.AuthStatePath) -and [string]$authContext.BearerTokenSource -eq 'bootstrap' -and [string]$authContext.AdminPasswordSource -eq 'bootstrap') { 'pass' } else { 'fail' }
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'persistence/auth-bootstrap' -Verdict $authVerdict -Actual ("authState={0}; bearerSource={1}; adminSource={2}" -f $authContext.AuthStatePath, $authContext.BearerTokenSource, $authContext.AdminPasswordSource) -DurationMs 0)

    [Environment]::SetEnvironmentVariable('MCPACE_BEARER_TOKEN', 'verification-bearer-token')
    $overrideContext = New-McpAceContext -RootPath $PSScriptRoot -StateRoot $tempStateRoot
    $overrideVerdict = if ([string]$overrideContext.BearerToken -eq 'verification-bearer-token' -and [string]$overrideContext.BearerTokenSource -eq 'env') { 'pass' } else { 'fail' }
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'persistence/env-override' -Verdict $overrideVerdict -Actual ("tokenSource={0}; token={1}" -f $overrideContext.BearerTokenSource, $overrideContext.BearerToken) -DurationMs 0)
    [Environment]::SetEnvironmentVariable('MCPACE_BEARER_TOKEN', $null)

    [Environment]::SetEnvironmentVariable('MCPACE_BEARER_TOKEN', 'verification-bearer-token')
    [Environment]::SetEnvironmentVariable('MCPACE_ADMIN_PASSWORD_BCRYPT', '$2b$10$1hLtpWUfeMNXxJBW9KWeneA5OClk.HQy5a1z/PHIcX0l6094xgrKq')
    $arrayContext = New-McpAceContext -RootPath $PSScriptRoot -StateRoot $tempStateRoot
    $effective = Read-JsonFile -Path $arrayContext.SettingsEffectivePath
    $arrayVerdict = if (
        (Test-JsonArrayLikeValue -Value $effective.bearerKeys) -and
        (Test-JsonArrayLikeValue -Value $effective.users) -and
        (Test-JsonArrayLikeValue -Value $effective.prompts) -and
        (Test-JsonArrayLikeValue -Value $effective.resources) -and
        (Test-JsonArrayLikeValue -Value $effective.mcpServers.browser.args)
    ) { 'pass' } else { 'fail' }
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'persistence/effective-array-shape' -Verdict $arrayVerdict -Actual ("bearerKeys={0}; users={1}; browserArgs={2}" -f @($effective.bearerKeys).Count, @($effective.users).Count, @($effective.mcpServers.browser.args).Count) -DurationMs 0)
    [Environment]::SetEnvironmentVariable('MCPACE_BEARER_TOKEN', $null)
    [Environment]::SetEnvironmentVariable('MCPACE_ADMIN_PASSWORD_BCRYPT', $null)

    $backupResult = New-DataBackup -Context $authContext
    $backupVerdict = if (Test-Path -LiteralPath $backupResult.BackupPath) { 'pass' } else { 'fail' }
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'persistence/backup' -Verdict $backupVerdict -Actual ("backup={0}; purged={1}" -f $backupResult.BackupPath, $backupResult.PurgedCount) -Artifacts @([string]$backupResult.BackupPath) -DurationMs 0)

    if ($catalogLookup.ContainsKey('destructive/temp-auth-reset')) {
        $beforeResetAuthContext = New-McpAceContext -RootPath $PSScriptRoot -StateRoot $tempStateRoot
        $beforeToken = [string]$beforeResetAuthContext.BearerToken
        Reset-LocalAuthState -Path $beforeResetAuthContext.AuthStatePath
        $afterResetAuthContext = New-McpAceContext -RootPath $PSScriptRoot -StateRoot $tempStateRoot
        $tempAuthResetVerdict = if ([string]$afterResetAuthContext.BearerToken -ne $beforeToken -and [bool]$afterResetAuthContext.AdminPasswordKnown) { 'pass' } else { 'fail' }
        Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'destructive/temp-auth-reset' -Verdict $tempAuthResetVerdict -Actual ("tokenChanged={0}" -f ([string]$afterResetAuthContext.BearerToken -ne $beforeToken)) -DurationMs 0)
    }
}
finally {
    [Environment]::SetEnvironmentVariable('MCPACE_STATE_ROOT', $oldStateRoot)
    [Environment]::SetEnvironmentVariable('MCPACE_BEARER_TOKEN', $oldBearer)
    [Environment]::SetEnvironmentVariable('MCPACE_ADMIN_PASSWORD_BCRYPT', $oldBcrypt)
    Remove-Item -LiteralPath $tempStateRoot -Recurse -Force -ErrorAction SilentlyContinue
}

$overrideStateRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-verify-overrides-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $overrideStateRoot | Out-Null
try {
    $overridesPath = Join-Path $overrideStateRoot 'data\runtime\mcp_settings.local-overrides.json'
    Write-LocalServerOverrides -Path $overridesPath -Overrides ([pscustomobject]@{
        mcpServers = [pscustomobject]@{
            'windows-mcp' = [pscustomobject]@{
                enabled = $true
            }
        }
    }) | Out-Null
    $overridePersistContext = New-McpAceContext -RootPath $PSScriptRoot -StateRoot $overrideStateRoot
    $windowsEntry = Get-ServerRuntimeEntry -Context $overridePersistContext -Name 'windows-mcp'
    $localOverridesVerdict = if ($windowsEntry -and [bool]$windowsEntry.ConfiguredEnabled -and [string]$windowsEntry.EnabledSource -eq 'local-override') { 'pass' } else { 'fail' }
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'persistence/local-overrides' -Verdict $localOverridesVerdict -Actual ("configured={0}; source={1}; effective={2}" -f $windowsEntry.ConfiguredEnabled, $windowsEntry.EnabledSource, $windowsEntry.EffectiveEnabled) -DurationMs 0)
}
catch {
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'persistence/local-overrides' -Verdict 'fail' -Actual $_.Exception.Message -DurationMs 0)
}
finally {
    Remove-Item -LiteralPath $overrideStateRoot -Recurse -Force -ErrorAction SilentlyContinue
}

$releaseOutputRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-verify-release-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $releaseOutputRoot | Out-Null
try {
    $releaseCapture = Invoke-PwshCapture -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', '.\build-release.ps1', '-OutputDir', $releaseOutputRoot)
    $archive = @(Get-ChildItem -LiteralPath $releaseOutputRoot -File -Filter 'mcpace-*.zip' | Select-Object -First 1)
    $releaseArtifacts = @()
    if ($archive) {
        $releaseArtifacts += [string]$archive[0].FullName
    }
    $releaseVerdict = if (-not $releaseCapture.TimedOut -and $releaseCapture.ExitCode -eq 0 -and $archive) { 'pass' } else { 'fail' }
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'persistence/release-bundle' -Verdict $releaseVerdict -Actual (Get-CommandCaptureSummary -Capture $releaseCapture) -Artifacts $releaseArtifacts -DurationMs $releaseCapture.DurationMs)
}
finally {
    Remove-Item -LiteralPath $releaseOutputRoot -Recurse -Force -ErrorAction SilentlyContinue
}

if ($catalogLookup.ContainsKey('destructive/temp-reset-hub-data')) {
    $tempRepairEnvironment = $null
    try {
        $tempRepairEnvironment = New-IsolatedVerificationEnvironment
        $bootTemp = Ensure-StackRunning -Context $tempRepairEnvironment.Context
        $tempRepairEnvironment.Context = $bootTemp.Context
        $tempRepairCapture = Invoke-PwshCapture -WorkingDirectory $tempRepairEnvironment.ManagerRoot -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', '.\repair.ps1', '-ResetHubData')
        $reloadedTempContext = New-McpAceContext -RootPath $tempRepairEnvironment.ManagerRoot -StateRoot $tempRepairEnvironment.StateRoot
        $tempStatuses = @(Get-HubServerStatuses -Context $reloadedTempContext)
        $tempRequired = Get-RequiredServerConnectivity -Context $reloadedTempContext -ServerStatuses $tempStatuses
        $missingText = if ($tempRequired.Disconnected.Count -gt 0) { $tempRequired.Disconnected -join ',' } else { 'none' }
        $tempRepairVerdict = if (-not $tempRepairCapture.TimedOut -and $tempRepairCapture.ExitCode -eq 0 -and $tempRequired.Disconnected.Count -eq 0) { 'pass' } else { 'fail' }
        Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'destructive/temp-reset-hub-data' -Verdict $tempRepairVerdict -Actual ("repair={0}; missing={1}" -f (Get-CommandCaptureSummary -Capture $tempRepairCapture), $missingText) -DurationMs $tempRepairCapture.DurationMs)
    }
    catch {
        Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'destructive/temp-reset-hub-data' -Verdict 'fail' -Actual $_.Exception.Message -DurationMs 0)
    }
    finally {
        if ($tempRepairEnvironment) {
            Remove-IsolatedVerificationEnvironment -Environment $tempRepairEnvironment
        }
    }
}

if ($Profile -eq 'full' -or $IncludeLiveDestructive) {
    $liveBackup = $null
    try {
        $liveBackup = New-DataBackup -Context $context
        $liveArtifacts = @([string]$liveBackup.BackupPath)

        $authResetCapture = Invoke-PwshCapture -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', '.\auth.ps1', '-Reset')
        $authResetCheck = Invoke-PwshCapture -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', '.\check.ps1')
        $liveAuthResetVerdict = if (-not $authResetCapture.TimedOut -and $authResetCapture.ExitCode -eq 0 -and $authResetCheck.ExitCode -eq 0) { 'pass' } else { 'fail' }
        Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'destructive/live-auth-reset' -Verdict $liveAuthResetVerdict -Actual ("backup={0}; reset={1}; check={2}" -f $liveBackup.BackupPath, (Get-CommandCaptureSummary -Capture $authResetCapture), (Get-CommandCaptureSummary -Capture $authResetCheck)) -Artifacts $liveArtifacts -DurationMs ($authResetCapture.DurationMs + $authResetCheck.DurationMs))

        $liveResetCapture = Invoke-PwshCapture -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', '.\repair.ps1', '-ResetHubData')
        $liveResetCheck = Invoke-PwshCapture -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', '.\check.ps1')
        $liveResetVerdict = if (-not $liveResetCapture.TimedOut -and $liveResetCapture.ExitCode -eq 0 -and $liveResetCheck.ExitCode -eq 0) { 'pass' } else { 'fail' }
        Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'destructive/live-reset-hub-data' -Verdict $liveResetVerdict -Actual ("repair={0}; check={1}" -f (Get-CommandCaptureSummary -Capture $liveResetCapture), (Get-CommandCaptureSummary -Capture $liveResetCheck)) -Artifacts $liveArtifacts -DurationMs ($liveResetCapture.DurationMs + $liveResetCheck.DurationMs))
    }
    catch {
        $liveFailureArtifacts = @()
        if ($liveBackup) {
            $liveFailureArtifacts += [string]$liveBackup.BackupPath
        }
        Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'destructive/live-reset-hub-data' -Verdict 'fail' -Actual $_.Exception.Message -Artifacts $liveFailureArtifacts -DurationMs 0)
    }
}
else {
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'destructive/live-auth-reset' -Verdict 'skipped' -Actual 'Live destructive scenarios are disabled for standard and ci-runtime profiles.' -DurationMs 0)
    Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'destructive/live-reset-hub-data' -Verdict 'skipped' -Actual 'Live destructive scenarios are disabled for standard and ci-runtime profiles.' -DurationMs 0)
}

# platform/documentation lanes
Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'platform/windows-current-host' -Verdict $(if ($context.IsWindows) { 'pass' } else { 'not-proven' }) -Actual $(if ($context.IsWindows) { 'Current audit was executed on this Windows host.' } else { 'Current host is not Windows.' }) -DurationMs 0)

$ubuntuWorkflowPath = Join-Path $PSScriptRoot '.github\workflows\runtime-smoke-ubuntu.yml'
$ubuntuWorkflowText = if (Test-Path -LiteralPath $ubuntuWorkflowPath) { Get-Content -LiteralPath $ubuntuWorkflowPath -Raw -Encoding UTF8 } else { '' }
$ubuntuVerdict = if ($ubuntuWorkflowText -match 'verify-manager\.ps1' -or $ubuntuWorkflowText -match 'manager\.sh verify') { 'pass' } else { 'warn' }
Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'platform/ubuntu-workflow' -Verdict $ubuntuVerdict -Actual $(if ($ubuntuVerdict -eq 'pass') { 'Workflow references the repeatable verification entrypoint.' } else { 'Workflow exists but does not reference the repeatable verification entrypoint.' }) -Artifacts @($ubuntuWorkflowPath) -DurationMs 0)

$macosGatePath = Join-Path $PSScriptRoot 'docs\runtime-smoke-macos.md'
$macosGateText = if (Test-Path -LiteralPath $macosGatePath) { Get-Content -LiteralPath $macosGatePath -Raw -Encoding UTF8 } else { '' }
$macosVerdict = if ($macosGateText -match 'verify-manager\.ps1') { 'pass' } else { 'warn' }
Add-Result -Collection $results -Result (New-ScenarioResult -ScenarioId 'platform/macos-manual-gate' -Verdict $macosVerdict -Actual $(if ($macosVerdict -eq 'pass') { 'Manual gate references verify-manager.ps1.' } else { 'Manual gate exists but does not reference verify-manager.ps1.' }) -Artifacts @($macosGatePath) -DurationMs 0)

$jsonResolved = if ([System.IO.Path]::IsPathRooted($JsonOutputPath)) { $JsonOutputPath } else { Join-Path $PSScriptRoot $JsonOutputPath }
$markdownResolved = if ([System.IO.Path]::IsPathRooted($MarkdownOutputPath)) { $MarkdownOutputPath } else { Join-Path $PSScriptRoot $MarkdownOutputPath }
$jsonDir = Split-Path -Parent $jsonResolved
$markdownDir = Split-Path -Parent $markdownResolved
if (-not [string]::IsNullOrWhiteSpace($jsonDir)) { New-Item -ItemType Directory -Force -Path $jsonDir | Out-Null }
if (-not [string]::IsNullOrWhiteSpace($markdownDir)) { New-Item -ItemType Directory -Force -Path $markdownDir | Out-Null }

try {
    $report = New-VerificationReport -Profile $Profile -Results $results.ToArray() -Environment $environment
    $markdown = ConvertTo-VerificationMarkdownReport -Report $report
    Write-JsonFile -Path $jsonResolved -Value $report
    Set-Content -LiteralPath $markdownResolved -Value $markdown -Encoding UTF8
}
catch {
    Write-Host ("Verification report generation failed: {0}" -f $_.Exception.Message) -ForegroundColor Red
    if (-not [string]::IsNullOrWhiteSpace([string]$_.ScriptStackTrace)) {
        Write-Host $_.ScriptStackTrace -ForegroundColor Yellow
    }
    throw
}

Write-Host ''
Write-Host ("Verification JSON: {0}" -f $jsonResolved) -ForegroundColor Cyan
Write-Host ("Verification Markdown: {0}" -f $markdownResolved) -ForegroundColor Cyan

if ([string]$report.overallVerdict -eq 'fail') {
    exit 1
}

exit 0
