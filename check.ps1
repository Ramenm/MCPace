#requires -Version 7.0
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'lib/runtime.ps1')

function Get-HostBridgeServers {
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    return @(Get-HostBridgeRuntimeEntries -Context $Context)
}

try {
    $context = New-McpAceContext -RootPath $PSScriptRoot
    Write-ClientLauncher -Context $context | Out-Null
}
catch {
    Write-Host 'MCPace configuration is invalid.' -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Yellow
    exit 1
}

try {
    Assert-Prerequisites -Context $context
    $ensureResult = Ensure-StackRunning -Context $context
    $context = $ensureResult.Context
}
catch {
    Write-Host $_.Exception.Message -ForegroundColor Red
    exit 1
}

$snapshot = Get-ManagerDashboardSnapshot -Context $context
$abp = $snapshot.AbpState
$hub = $snapshot.HubState
$abpDiag = $snapshot.AbpDiagnostics
$hubHealth = $snapshot.HubHealth
$serverStatuses = @($snapshot.ServerStatuses)
$summary = $snapshot.ServerSummary
$requiredConnectivity = $snapshot.RequiredConnectivity
$issues = @($snapshot.ServerIssues)
$healthModel = $snapshot.HealthModel
$otherStatuses = @($snapshot.NonStandardStatuses)
$bridgeServers = @(Get-HostBridgeServers -Context $context)
$runtimeServers = @($context.ServerRuntime | Sort-Object Name)
$portableManagerLayout = Test-PortableManagerRootLayout -RootPath $context.ManagerRoot
$dockerPortPublishers = @()
if (($abp.State -eq 'blocked') -or (($abp.State -eq 'offline') -and -not $abpDiag.EndpointReachable)) {
    $dockerPortPublishers = @(Get-DockerPortPublishers -HostPort $context.AbpPort)
}

Write-Host ("ABP:     {0} {1}" -f $abp.State.ToUpperInvariant(), $abp.Detail)
Write-Host ("MCPace:  {0} {1}" -f $hub.State.ToUpperInvariant(), $hub.Detail)
Write-Host ("ABP endpoint: {0}" -f $(if ($abpDiag.EndpointReachable) { 'reachable' } else { 'not reachable' }))
if ($abpDiag.OwnerDisplay) {
    Write-Host ("ABP port owner: {0}" -f $abpDiag.OwnerDisplay)
}
else {
    Write-Host "ABP port owner: none"
}
if ($dockerPortPublishers.Count -gt 0) {
    Write-Host ("Containers publishing ABP port {0}:" -f $context.AbpPort) -ForegroundColor Yellow
    foreach ($item in $dockerPortPublishers) {
        Write-Host ("  - {0} ({1})" -f $item.Name, $item.Ports) -ForegroundColor Yellow
    }
}
Write-Host ("Manager root: {0}" -f $context.ManagerRoot)
Write-Host ("Primary workspace: {0} | host={1} | access={2}" -f $context.WorkspaceRegistry.Primary.Name, $context.WorkspaceRegistry.Primary.HostPath, $context.WorkspaceRegistry.Primary.Access)
Write-Host ("  container paths: {0}" -f (($context.WorkspaceRegistry.Primary.ExposedContainerPaths | Sort-Object) -join ', ')) -ForegroundColor DarkGray
if ($context.WorkspaceRegistry.Extras.Count -gt 0) {
    Write-Host "Extra workspaces:"
    foreach ($workspace in @($context.WorkspaceRegistry.Extras | Sort-Object Name)) {
        Write-Host ("  - {0}: host={1} | access={2} | container={3}" -f $workspace.Name, $workspace.HostPath, $workspace.Access, $workspace.CanonicalContainerPath)
    }
}
else {
    Write-Host "Extra workspaces: none"
}
Write-Host "Workspace mounts:"
foreach ($mount in @($context.WorkspaceRegistry.Mounts)) {
    $mode = if ($mount.ReadOnly) { 'ro' } else { 'rw' }
    Write-Host ("  - {0}: {1} -> {2} ({3}; {4})" -f $mount.WorkspaceName, $mount.HostPath, $mount.ContainerPath, $mode, $mount.Kind)
}
if ($portableManagerLayout.Passed) {
    Write-Host "Portable manager root: complete"
}
else {
    Write-Host "Portable manager root: missing required paths" -ForegroundColor Yellow
    Write-Host ("  missing: {0}" -f ($portableManagerLayout.MissingRequired -join ', ')) -ForegroundColor Yellow
}
if ($portableManagerLayout.MissingOptional.Count -gt 0) {
    Write-Host ("Portable manager optional paths missing: {0}" -f ($portableManagerLayout.MissingOptional -join ', ')) -ForegroundColor DarkGray
}
if ($bridgeServers.Count -gt 0) {
    Write-Host "Host bridge MCP servers:"
    foreach ($bridge in $bridgeServers) {
        $requiredLabel = if ($bridge.Required) { 'required' } else { 'optional' }
        $preflightLabel = if ($bridge.PreflightPassed) { 'passed' } else { 'failed' }
        Write-Host ("  - {0}: {1} | {2} | autoStart={3} | sourceEnabled={4} | configuredEnabled={5} | stateSource={6} | effectiveEnabled={7}" -f $bridge.Name, $bridge.Kind, $requiredLabel, $bridge.AutoStart, $bridge.SourceEnabled, $bridge.ConfiguredEnabled, $bridge.EnabledSource, $bridge.EffectiveEnabled)
        if (-not [string]::IsNullOrWhiteSpace($bridge.HealthUrl)) {
            Write-Host ("      health: {0}" -f $bridge.HealthUrl)
        }
        if (-not [string]::IsNullOrWhiteSpace($bridge.HostBridgeUrl)) {
            Write-Host ("      mcp url: {0}" -f $bridge.HostBridgeUrl)
        }
        Write-Host ("      preflight: {0}" -f $preflightLabel)
        if (-not [string]::IsNullOrWhiteSpace($bridge.PreflightSummary)) {
            Write-Host ("      preflight summary: {0}" -f $bridge.PreflightSummary) -ForegroundColor DarkGray
        }
        if ($bridge.PreflightReasons.Count -gt 0) {
            Write-Host ("      preflight detail: {0}" -f ($bridge.PreflightReasons -join '; ')) -ForegroundColor Yellow
        }
        if (-not [string]::IsNullOrWhiteSpace($bridge.DisabledReason)) {
            Write-Host ("      disabled reason: {0}" -f $bridge.DisabledReason) -ForegroundColor Yellow
        }
    }
}
if ($runtimeServers.Count -gt 0) {
    Write-Host "Runtime server placement:"
    foreach ($entry in $runtimeServers) {
        $requiredLabel = if ($entry.Required) { 'required' } else { 'optional' }
        Write-Host ("  - {0}: {1} | {2} | autoStart={3} | sourceEnabled={4} | configuredEnabled={5} | stateSource={6} | effectiveEnabled={7}" -f $entry.Name, $entry.Kind, $requiredLabel, $entry.AutoStart, $entry.SourceEnabled, $entry.ConfiguredEnabled, $entry.EnabledSource, $entry.EffectiveEnabled)
        if (-not [string]::IsNullOrWhiteSpace($entry.HealthUrl)) {
            Write-Host ("      health: {0}" -f $entry.HealthUrl)
        }
        if (-not [string]::IsNullOrWhiteSpace($entry.HostBridgeUrl)) {
            Write-Host ("      mcp url: {0}" -f $entry.HostBridgeUrl)
        }
        if (-not [string]::IsNullOrWhiteSpace($entry.PreflightSummary)) {
            Write-Host ("      preflight summary: {0}" -f $entry.PreflightSummary) -ForegroundColor DarkGray
        }
        if (
            (-not [string]::IsNullOrWhiteSpace([string]$entry.InstallTarget) -and [string]$entry.InstallTarget -ne 'none') -or
            (-not [string]::IsNullOrWhiteSpace([string]$entry.InstallStatus) -and [string]$entry.InstallStatus -ne 'not-managed')
        ) {
            Write-Host ("      install: target={0}; method={1}; package={2}; status={3}; binaryPresent={4}" -f $entry.InstallTarget, $entry.InstallMethod, $(if ([string]::IsNullOrWhiteSpace([string]$entry.InstallPackage)) { '-' } else { $entry.InstallPackage }), $entry.InstallStatus, $entry.BinaryPresent)
            if (-not [string]::IsNullOrWhiteSpace([string]$entry.BinaryProbeDetail)) {
                Write-Host ("      binary detail: {0}" -f $entry.BinaryProbeDetail) -ForegroundColor DarkGray
            }
            if (-not [string]::IsNullOrWhiteSpace([string]$entry.InstallError)) {
                Write-Host ("      install error: {0}" -f $entry.InstallError) -ForegroundColor Yellow
            }
        }
        if (-not [string]::IsNullOrWhiteSpace($entry.DisabledReason)) {
            Write-Host ("      disabled reason: {0}" -f $entry.DisabledReason) -ForegroundColor Yellow
        }
    }
}
if ($context.PlatformDisabledServers.Count -gt 0) {
    Write-Host (("Platform-disabled servers: {0}") -f (($context.PlatformDisabledServers | Sort-Object) -join ', ')) -ForegroundColor Yellow
}
if ($context.PlaceholderDisabledServers.Count -gt 0) {
    Write-Host (("Placeholder-disabled servers: {0}") -f (($context.PlaceholderDisabledServers | Sort-Object) -join ', ')) -ForegroundColor Yellow
}
if ($context.MissingCommandDisabledServers.Count -gt 0) {
    Write-Host (("Missing-command disabled servers: {0}") -f (($context.MissingCommandDisabledServers | Sort-Object) -join ', ')) -ForegroundColor Yellow
}
if ($context.PreflightDisabledServers.Count -gt 0) {
    Write-Host (("Preflight-disabled servers: {0}") -f (($context.PreflightDisabledServers | Sort-Object) -join ', ')) -ForegroundColor Yellow
}
Write-Host ("Health summary: {0}" -f $healthModel.SummaryText)
if ($hubHealth.Status -ne 'offline') {
    Write-Host ("Hub health: {0}; servers {1}/{2}" -f $hubHealth.Status, $hubHealth.ServersConnected, $hubHealth.ServersTotal)
    if ($hubHealth.Status -eq 'degraded' -and $hubHealth.ServersConnected -eq 0 -and $hubHealth.ServersTotal -gt 0) {
        Write-Host "Reason: browser disconnected from MCPace."
    }
    if ($requiredConnectivity.Required.Count -gt 0) {
        Write-Host ("Required path: connected={0}; missing={1}" -f ($requiredConnectivity.Connected -join ', '), $(if ($requiredConnectivity.Disconnected.Count -gt 0) { $requiredConnectivity.Disconnected -join ', ' } else { 'none' }))
    }
}
if ($serverStatuses.Count -gt 0) {
    Write-Host ("Servers total: {0} | Online: {1} | Offline: {2} | Connecting: {3} | Disabled: {4}" -f $summary.Total, $summary.Online, $summary.Offline, $summary.Connecting, $summary.Disabled)
    if ($otherStatuses.Count -gt 0) {
        Write-Host ("Other statuses: {0}" -f ($otherStatuses -join ', '))
    }
    Write-Host ''
    if ($issues.Count -gt 0) {
        Write-Host ''
        Write-Host ("Server issues ({0}):" -f $issues.Count) -ForegroundColor Yellow
        foreach ($issue in $issues) {
            $entry = $issue.Entry
            if ([string]::IsNullOrWhiteSpace($issue.Error)) {
                Write-Host ("  - {0}: {1}" -f $issue.Name, $issue.Status) -ForegroundColor Yellow
            }
            else {
                Write-Host ("  - {0}: {1} | {2}" -f $issue.Name, $issue.Status, $issue.Error) -ForegroundColor Yellow
            }
            if ($entry) {
                Write-Host ("      manager view: kind={0}; required={1}; autoStart={2}; sourceEnabled={3}; configuredEnabled={4}; stateSource={5}; effectiveEnabled={6}" -f $entry.Kind, $entry.Required, $entry.AutoStart, $entry.SourceEnabled, $entry.ConfiguredEnabled, $entry.EnabledSource, $entry.EffectiveEnabled) -ForegroundColor DarkGray
                if (
                    (-not [string]::IsNullOrWhiteSpace([string]$entry.InstallTarget) -and [string]$entry.InstallTarget -ne 'none') -or
                    (-not [string]::IsNullOrWhiteSpace([string]$entry.InstallStatus) -and [string]$entry.InstallStatus -ne 'not-managed')
                ) {
                    Write-Host ("      install view: target={0}; method={1}; package={2}; status={3}; binaryPresent={4}" -f $entry.InstallTarget, $entry.InstallMethod, $(if ([string]::IsNullOrWhiteSpace([string]$entry.InstallPackage)) { '-' } else { $entry.InstallPackage }), $entry.InstallStatus, $entry.BinaryPresent) -ForegroundColor DarkGray
                    if (-not [string]::IsNullOrWhiteSpace([string]$entry.InstallError)) {
                        Write-Host ("      install error: {0}" -f $entry.InstallError) -ForegroundColor Yellow
                    }
                }
            }
            $actionText = Get-ServerIssueActionText -Issue $issue
            if (-not [string]::IsNullOrWhiteSpace([string]$actionText)) {
                Write-Host ("      -> {0}" -f $actionText) -ForegroundColor Cyan
            }
        }

        $compatibilitySuspects = @($issues | Where-Object { $_.CompatibilitySuspect } | ForEach-Object { $_.CompatibilityMessage } | Select-Object -Unique)

        if ($compatibilitySuspects.Count -gt 0) {
            Write-Host ''
            Write-Host 'Compatibility suspects:' -ForegroundColor Yellow
            foreach ($line in ($compatibilitySuspects | Select-Object -Unique)) {
                Write-Host ("  - {0}" -f $line) -ForegroundColor Cyan
            }
        }
    }
}
Write-Host ("Endpoint: http://127.0.0.1:{0}/mcp" -f $context.HubPort)
Write-Host (("Launcher: {0}") -f (Get-ClientLauncherLabel -Context $context))
Write-Host ("Auth utility: .\auth.ps1")
Write-Host ("Auth source: bearer={0}; admin={1}" -f $context.BearerTokenSource, $context.AdminPasswordSource)
Write-Host ("Token: {0}" -f (Get-MaskedToken -Token $context.BearerToken))
Write-Host ''
Write-Host 'Generic/manual client config (launcher-first):' -ForegroundColor Cyan
Write-Host (Get-ClientConfigJson -Context $context)
Write-Host ''
Write-Host 'Editor setup:' -ForegroundColor Cyan
Write-Host '  pwsh ./setup-mcp-clients.ps1 -Overwrite'

if ($abpDiag.EndpointReachable -and (Test-HubReady -Context $context)) {
    exit 0
}

exit 1
