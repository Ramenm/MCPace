#requires -Version 7.0
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'lib/runtime.ps1')

function wg($t) { Write-Host $t -ForegroundColor Green -NoNewline }
function wr($t) { Write-Host $t -ForegroundColor Red -NoNewline }
function wy($t) { Write-Host $t -ForegroundColor Yellow -NoNewline }
function wc($t) { Write-Host $t -ForegroundColor Cyan -NoNewline }
function wk($t) { Write-Host $t -ForegroundColor DarkGray -NoNewline }
function ww($t) { Write-Host $t -NoNewline }
function nl { Write-Host '' }

function Write-StateCell {
    param(
        [Parameter(Mandatory = $true)][string]$State
    )

    switch ($State) {
        'running'  { wg '[ON]'; ww '  '; wg 'RUNNING   ' }
        'healthy'  { wg '[ON]'; ww '  '; wg 'HEALTHY   ' }
        'degraded' { wy '[ON]'; ww '  '; wy 'DEGRADED  ' }
        'starting' { wy '[..]'; ww '  '; wy 'STARTING  ' }
        'blocked'  { wr '[!!]'; ww '  '; wr 'BLOCKED   ' }
        default    { wr '[OFF]'; ww ' '; wr 'OFFLINE   ' }
    }
}

function Get-AutostartLabel {
    param(
        [Parameter(Mandatory = $true)]$Status
    )

    if (-not $Status.Exists) {
        return 'OFF'
    }
    if ($Status.Enabled) {
        return ("ON ({0})" -f $Status.State)
    }
    return ("OFF ({0})" -f $Status.State)
}

function Limit-UiText {
    param(
        [Parameter(Mandatory = $false)][AllowEmptyString()][string]$Text = '',
        [Parameter(Mandatory = $false)][int]$MaxLength = 96
    )

    $flat = ([string]$Text -replace '\s+', ' ').Trim()
    if ([string]::IsNullOrWhiteSpace($flat)) {
        return ''
    }

    if ($flat.Length -le $MaxLength) {
        return $flat
    }

    if ($MaxLength -le 3) {
        return $flat.Substring(0, $MaxLength)
    }

    return ("{0}..." -f $flat.Substring(0, $MaxLength - 3))
}

function Write-ServiceOverview {
    param(
        [Parameter(Mandatory = $true)][string]$Hotkey,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)]$DisplayModel,
        [Parameter(Mandatory = $true)][int]$Port
    )

    ww '   '; wy ("[{0}]" -f $Hotkey); ww (' {0}' -f $Name.PadRight(24))
    Write-StateCell -State $DisplayModel.State
    if (-not [string]::IsNullOrWhiteSpace([string]$DisplayModel.HeadlineMetric)) {
        ww ' '
        wk (Limit-UiText -Text $DisplayModel.HeadlineMetric -MaxLength 28)
    }
    ww ("  :{0}" -f $Port); nl

    if (-not [string]::IsNullOrWhiteSpace([string]$DisplayModel.Detail)) {
        ww '       '
        wk (Limit-UiText -Text $DisplayModel.Detail -MaxLength 90)
        nl
    }
}

function Get-LastActionLogHint {
    param(
        [Parameter(Mandatory = $false)][AllowEmptyString()][string]$Message = ''
    )

    $text = [string]$Message
    if ([string]::IsNullOrWhiteSpace($text)) {
        return ''
    }

    if ($text -match 'transport|Connection closed|session|Smoke test') {
        return '.\logs\mcpace.current.log'
    }
    if ($text -match 'ABP|Browser MCP|port is occupied') {
        return '.\logs\abp.stderr.log'
    }
    if ($text -match 'MCPace|required path|OAuth|install|restart|startup|warning') {
        return '.\logs\launcher.log'
    }
    return ''
}

function Set-LastAction {
    param(
        [Parameter(Mandatory = $true)][string]$Message,
        [Parameter(Mandatory = $false)][string]$Color = 'White'
    )

    $script:lastActionMessage = $Message
    $script:lastActionColor = $Color
    $script:lastActionAt = Get-Date
}

function Draw-Dashboard {
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $true)]$Snapshot,
        [Parameter(Mandatory = $true)]$AutostartStatus,
        [Parameter(Mandatory = $false)][AllowEmptyString()][string]$LastActionMessage = '',
        [Parameter(Mandatory = $false)][string]$LastActionColor = 'DarkGray',
        [Parameter(Mandatory = $false)]$LastActionAt = $null,
        [Parameter(Mandatory = $false)]$LastSuccessfulProbeAt = $null,
        [Parameter(Mandatory = $true)][int]$RefreshIntervalSec
    )

    Clear-Host
    $sep = '-' * 62
    $tokenMasked = Get-MaskedToken -Token $Context.BearerToken
    $healthModel = $Snapshot.HealthModel

    ww "  $sep"; nl
    ww '    '; wc 'MCPace'; ww '  '; wk "v$($Context.Config.version)"; nl
    ww "  $sep"; nl

    Write-ServiceOverview -Hotkey '1' -Name 'Browser MCP (ABP)' -DisplayModel $Snapshot.AbpDisplay -Port $Context.AbpPort
    Write-ServiceOverview -Hotkey '2' -Name 'MCPace (Docker)' -DisplayModel $Snapshot.HubDisplay -Port $Context.HubPort

    ww "  $sep"; nl
    ww '   Health:'; nl
    ww '     refresh '; wc ($Snapshot.CollectedAt.ToString('HH:mm:ss')); ww '  '; wk ("auto-refresh {0}s" -f $RefreshIntervalSec); nl
    if (-not $Snapshot.ProbeSuccessful) {
        ww '     probe   '; wy 'stale'
        ww '  '
        $probeText = if ($LastSuccessfulProbeAt) {
            "last successful {0}" -f ((Get-DateTimeOffsetSafe -Value $LastSuccessfulProbeAt).ToString('HH:mm:ss'))
        }
        else {
            'last successful never'
        }
        wk $probeText
        nl
    }
    ww '     servers '; wk $healthModel.CounterText; nl
    ww '     summary '
    switch ([string]$healthModel.SummaryState) {
        'ok'    { wg $healthModel.SummaryText }
        'error' { wr $healthModel.SummaryText }
        default { wy $healthModel.SummaryText }
    }
    nl
    $actionItems = @($healthModel.ActionItems)
    if ($actionItems.Count -gt 0) {
        ww '     next    '; wy (Limit-UiText -Text $actionItems[0] -MaxLength 92); nl
        foreach ($line in $actionItems | Select-Object -Skip 1) {
            ww '             '; wy (Limit-UiText -Text $line -MaxLength 92); nl
        }
    }
    else {
        ww '     next    '; wk 'none'; nl
    }
    ww "  $sep"; nl
    ww '   MCP endpoint: '; wc "http://127.0.0.1:$($Context.HubPort)/mcp"; nl
    ww '   Token:        '; wy $tokenMasked; nl
    $launcherLabel = Get-ClientLauncherLabel -Context $Context
    ww '   Launcher:     '; wc $launcherLabel; nl
    ww '   Note:         '; wk ("{0} is a client bridge and keeps this terminal busy until Ctrl+C." -f $launcherLabel); nl
    ww '   Autostart:    '; wc (Get-AutostartLabel -Status $AutostartStatus); ww '  '; wk $Context.AutostartTaskName; nl
    ww '   Settings:     '; wk ("logs={0}d backups={1} smoke={2}s" -f $Context.LogRetentionDays, $Context.BackupRetentionCount, $Context.SmokeTimeoutSec); nl
    ww '   Logs:         '; wc '.\logs\launcher.log'; ww '  '; wc '.\logs\abp.stderr.log'; nl
    ww "  $sep"; nl
    ww '   '; wg '[S]'; ww ' Start missing   '; wy '[R]'; ww ' Restart all   '; wr '[Q]'; ww ' Quit'; nl
    ww '   '; wr '[1]'; ww ' Toggle ABP      '; wr '[2]'; ww ' Toggle MCPace'; nl
    ww '   '; wc '[I]'; ww ' Install+fix     '; wc '[T]'; ww ' Smoke test     '; wc '[A]'; ww ' Toggle autostart'; nl
    ww '   '; wc '[L]'; ww ' Rotate logs     '; wc '[B]'; ww ' Backup data    '; wc '[N]'; ww ' Settings'; nl
    ww "  $sep"; nl

    ww '   Last action:  '
    if ([string]::IsNullOrWhiteSpace($LastActionMessage)) {
        wk 'none yet'
    }
    else {
        Write-Host (Limit-UiText -Text $LastActionMessage -MaxLength 88) -ForegroundColor $LastActionColor -NoNewline
    }
    if ($LastActionAt) {
        ww '  '
        wk ((Get-DateTimeOffsetSafe -Value $LastActionAt).ToString('HH:mm:ss'))
    }
    nl
    $logHint = if ([string]$LastActionColor -eq 'Red') { Get-LastActionLogHint -Message $LastActionMessage } else { '' }
    if (-not [string]::IsNullOrWhiteSpace($logHint)) {
        ww '   Log hint:     '; wc $logHint; nl
    }

    ww ''; nl
}

function Test-HubUiReady {
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $uiUrl = "http://127.0.0.1:$($Context.HubPort)"
    try {
        $response = Invoke-WebRequest -Uri $uiUrl -Method Get -UseBasicParsing -TimeoutSec $Context.ProbeTimeoutSec -ErrorAction Stop
        return ([int]$response.StatusCode -ge 200 -and [int]$response.StatusCode -lt 500)
    }
    catch {
        return $false
    }
}

function Wait-HubUiReady {
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $true)][int]$TimeoutSec
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        if (Test-HubUiReady -Context $Context) {
            return $true
        }
        Start-Sleep -Milliseconds 800
    }

    return $false
}

function Ensure-WindowsMcpHost {
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $runtimeEntry = Get-ServerRuntimeEntry -Context $Context -Name 'windows-mcp'
    if (-not $Context.IsWindows) {
        return [pscustomobject]@{
            Attempted = $false
            Success   = $true
            Message   = 'windows-mcp skipped on non-Windows platform.'
        }
    }

    if (-not $Context.Settings -or -not $Context.Settings.mcpServers -or -not ($Context.Settings.mcpServers.PSObject.Properties.Name -contains 'windows-mcp')) {
        return [pscustomobject]@{
            Attempted = $false
            Success   = $true
            Message   = 'windows-mcp is not configured.'
        }
    }

    if (-not [bool]$Context.Settings.mcpServers.'windows-mcp'.enabled) {
        $detail = if ($runtimeEntry -and -not [string]::IsNullOrWhiteSpace($runtimeEntry.DisabledReason)) {
            $runtimeEntry.DisabledReason
        }
        else {
            'windows-mcp is disabled.'
        }
        return [pscustomobject]@{
            Attempted = $false
            Success   = $true
            Message   = $detail
        }
    }

    $scriptPath = Join-Path $Context.RootPath 'windows-mcp-host.ps1'
    if (-not (Test-Path -LiteralPath $scriptPath)) {
        return [pscustomobject]@{
            Attempted = $false
            Success   = $false
            Message   = 'windows-mcp-host.ps1 not found.'
        }
    }

    if ([string]::IsNullOrWhiteSpace([string]$Context.PowerShellCommand)) {
        return [pscustomobject]@{
            Attempted = $false
            Success   = $false
            Message   = 'No PowerShell executable was found for windows-mcp host startup.'
        }
    }

    $targetPort = 8233
    try {
        $argsList = @($Context.Settings.mcpServers.'windows-mcp'.args)
        foreach ($arg in $argsList) {
            $value = [string]$arg
            if ($value -match ':(\d+)/mcp/?$') {
                $targetPort = [int]$Matches[1]
                break
            }
        }
    }
    catch {}

    try {
        $owner = Get-PortOwner -Port $targetPort
        if ($owner -and ([string]$owner.CommandLine -match 'windows-mcp')) {
            return [pscustomobject]@{
                Attempted = $true
                Success   = $true
                Message   = ("windows-mcp already running on port {0} (PID {1})." -f $targetPort, $owner.ProcessId)
            }
        }
    }
    catch {}

    try {
        $argumentList = @(
            '-NoProfile',
            '-ExecutionPolicy',
            'Bypass',
            '-File',
            $scriptPath,
            '-Mode',
            'start',
            '-Port',
            "$targetPort",
            '-StartTimeoutSec',
            '45'
        )
        Start-Process -FilePath $Context.PowerShellCommand -ArgumentList $argumentList -WindowStyle Hidden | Out-Null

        $startedQuickly = Wait-Until -TimeoutSec 3 -Test {
            $candidate = Get-PortOwner -Port $targetPort
            return ($candidate -and ([string]$candidate.CommandLine -match 'windows-mcp'))
        }

        $message = if ($startedQuickly) {
            ("windows-mcp is ready on port {0}." -f $targetPort)
        }
        else {
            ("windows-mcp start requested in background on port {0}; continuing without waiting." -f $targetPort)
        }

        return [pscustomobject]@{
            Attempted = $true
            Success   = $true
            Message   = $message
        }
    }
    catch {
        return [pscustomobject]@{
            Attempted = $true
            Success   = $false
            Message   = ("Windows-MCP host start failed: {0}" -f $_.Exception.Message)
        }
    }
}

try {
    $context = New-McpAceContext -RootPath $PSScriptRoot
}
catch {
    Write-Host 'MCPace configuration is invalid.' -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Yellow
    exit 1
}
$script:lastActionMessage = ''
$script:lastActionColor = 'DarkGray'
$script:lastActionAt = $null
$lastSuccessfulProbeAt = $null
$dashboardRefreshTicks = 20
$dashboardSleepMs = 100
$dashboardRefreshSec = [int](($dashboardRefreshTicks * $dashboardSleepMs) / 1000)
$uiUrl = "http://127.0.0.1:$($context.HubPort)"
$uiWaitSeconds = [Math]::Max([int]$context.StartupTimeoutSec, 15)

try {
    Write-Host ('[startup] Checking prerequisites...') -ForegroundColor Cyan
    Assert-Prerequisites -Context $context

    Write-Host ('[startup] Starting ABP + MCPace services...') -ForegroundColor Cyan
    $result = Ensure-StackRunning -Context $context
    $context = $result.Context
    $uiUrl = "http://127.0.0.1:$($context.HubPort)"
    $windowsMcp = @($result.HostBridgeResults | Where-Object { $_.Name -eq 'windows-mcp' } | Select-Object -First 1)
    if (-not $windowsMcp) {
        $windowsMcp = [pscustomobject]@{
            Attempted = $false
            Success   = $true
            Message   = ''
        }
    }

    Write-Host ("[startup] Waiting for UI: {0}" -f $uiUrl) -ForegroundColor Cyan
    $uiDeadline = (Get-Date).AddSeconds($uiWaitSeconds)
    $uiReady = $false
    while ((Get-Date) -lt $uiDeadline) {
        $hubHealth = Get-HubHealthInfo -Context $context
        if (Test-HubUiReady -Context $context) {
            $uiReady = $true
            break
        }

        Write-Host (
            "[startup] UI pending... hub={0}, servers={1}/{2}" -f `
            $hubHealth.Status, $hubHealth.ServersConnected, $hubHealth.ServersTotal
        ) -ForegroundColor DarkGray
        Start-Sleep -Milliseconds 900
    }

    if ($uiReady) {
        Write-Host ("[startup] UI is reachable: {0}" -f $uiUrl) -ForegroundColor Green
    }
    else {
        Write-Host ("[startup] UI did not respond in {0}s, continuing to dashboard..." -f $uiWaitSeconds) -ForegroundColor Yellow
    }

    if ($result.ABPReady -and $result.HubReady -and (($windowsMcp.Attempted -eq $false) -or $windowsMcp.Success)) {
        $startupMessage = if ($windowsMcp.Attempted) { "Services are ready. $($windowsMcp.Message)" } else { 'Services are ready.' }
        Set-LastAction -Message $startupMessage -Color 'Green'
    }
    else {
        $startupMessage = 'Startup finished with warnings. Run .\check.ps1 for details.'
        if ($windowsMcp.Attempted -and -not $windowsMcp.Success) {
            $startupMessage = ("{0} {1}" -f $startupMessage, $windowsMcp.Message)
        }
        Set-LastAction -Message $startupMessage -Color 'Yellow'
    }
}
catch {
    Clear-Host
    Write-Host 'MCPace failed to start.' -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Yellow
    Write-Host ''
    Write-Host 'What to check:' -ForegroundColor Cyan
    Write-Host '  1. Docker Desktop is running.'
    Write-Host '  2. Node.js 18+ is installed and visible to PowerShell.'
    Write-Host ("  3. Port {0} is free for ABP." -f $context.AbpPort)
    Write-Host ''
    exit 1
}

while ($true) {
    $snapshot = Get-ManagerDashboardSnapshot -Context $context
    if ($snapshot.ProbeSuccessful) {
        $lastSuccessfulProbeAt = $snapshot.CollectedAt
    }
    $autostartStatus = Get-AutostartStatus -Context $context
    Draw-Dashboard -Context $context -Snapshot $snapshot -AutostartStatus $autostartStatus -LastActionMessage $script:lastActionMessage -LastActionColor $script:lastActionColor -LastActionAt $script:lastActionAt -LastSuccessfulProbeAt $lastSuccessfulProbeAt -RefreshIntervalSec $dashboardRefreshSec

    for ($tick = 0; $tick -lt $dashboardRefreshTicks; $tick++) {
        if ($Host.UI.RawUI.KeyAvailable) {
            $key = $Host.UI.RawUI.ReadKey('NoEcho,IncludeKeyDown')
            $ch = $key.Character.ToString().ToUpperInvariant()

            try {
                switch ($ch) {
                    '1' {
                        if (($snapshot.AbpState.State -eq 'running') -or ($snapshot.AbpState.State -eq 'starting')) {
                            [void](Stop-ABP -Context $context)
                            Set-LastAction -Message 'ABP stopped.' -Color 'Yellow'
                        }
                        elseif ($snapshot.AbpState.State -eq 'blocked') {
                            Set-LastAction -Message 'ABP port is occupied by another process. Free the port or change it in mcpace.config.json.' -Color 'Red'
                        }
                        else {
                            Start-ABP -Context $context
                            if (Wait-Until -TimeoutSec $context.StartupTimeoutSec -Test { Test-ABPReady -Context $context }) {
                                Set-LastAction -Message 'ABP started.' -Color 'Green'
                            }
                            else {
                                Set-LastAction -Message 'ABP process started, but readiness probe did not pass in time.' -Color 'Yellow'
                            }
                        }
                    }
                    '2' {
                        if (($snapshot.HubState.State -eq 'healthy') -or ($snapshot.HubState.State -eq 'degraded') -or ($snapshot.HubState.State -eq 'starting')) {
                            Stop-Hub -Context $context
                            Set-LastAction -Message 'MCPace stopped.' -Color 'Yellow'
                        }
                        else {
                            Start-Hub -Context $context
                            if (Ensure-HubConnectivity -Context $context -AllowReconnect) {
                                Set-LastAction -Message 'MCPace started.' -Color 'Green'
                            }
                            else {
                                Set-LastAction -Message 'MCPace container started, but health did not become healthy in time.' -Color 'Yellow'
                            }
                        }
                    }
                    'S' {
                        $result = Ensure-StackRunning -Context $context
                        if ($result.ABPReady -and $result.HubReady) {
                            Set-LastAction -Message 'Missing services were started and are ready.' -Color 'Green'
                        }
                        else {
                            Set-LastAction -Message 'Start completed with warnings. Run .\check.ps1 for a quick status dump.' -Color 'Yellow'
                        }
                    }
                    'R' {
                        $result = Restart-Stack -Context $context
                        if ($result.ABPReady -and $result.HubReady) {
                            Set-LastAction -Message 'Stack restarted cleanly.' -Color 'Green'
                        }
                        else {
                            Set-LastAction -Message 'Restart completed with warnings. Check logs and run .\check.ps1.' -Color 'Yellow'
                        }
                    }
                    'I' {
                        $install = Invoke-Install -Context $context -RunSmoke
                        if ($install.Success) {
                            Set-LastAction -Message ("Install completed. Rotated logs: {0}. {1}" -f $install.RotatedCount, $install.SmokeMessage) -Color 'Green'
                        }
                        else {
                            Set-LastAction -Message ("Install warning. ABP={0}, Hub={1}. {2}" -f $install.ABPReady, $install.HubReady, $install.SmokeMessage) -Color 'Yellow'
                        }
                    }
                    'T' {
                        $smoke = Invoke-SmokeTest -Context $context
                        Set-LastAction -Message $smoke.Message -Color $(if ($smoke.Success) { 'Green' } else { 'Red' })
                    }
                    'A' {
                        $auto = Get-AutostartStatus -Context $context
                        if ($auto.Exists -and $auto.Enabled) {
                            $context = Disable-Autostart -Context $context
                            Set-LastAction -Message ("Autostart disabled ({0})." -f $context.AutostartTaskName) -Color 'Yellow'
                        }
                        else {
                            $context = Enable-Autostart -Context $context
                            Set-LastAction -Message ("Autostart enabled ({0})." -f $context.AutostartTaskName) -Color 'Green'
                        }
                    }
                    'L' {
                        $rotation = Rotate-Logs -Context $context
                        Set-LastAction -Message ("Log rotation completed. Removed {0} file(s) older than {1} days." -f $rotation.RemovedCount, $rotation.Days) -Color 'Green'
                    }
                    'B' {
                        $backup = New-DataBackup -Context $context
                        Set-LastAction -Message ("Backup created: {0}" -f $backup.BackupPath) -Color 'Green'
                    }
                    'N' {
                        Clear-Host
                        Write-Host 'Settings editor (press Enter to keep current value)' -ForegroundColor Cyan
                        Write-Host ''
                        Write-Host ("Current log retention days: {0}" -f $context.LogRetentionDays)
                        $inLogDays = Read-Host 'New log retention days'
                        Write-Host ("Current backup retention count: {0}" -f $context.BackupRetentionCount)
                        $inBackupCount = Read-Host 'New backup retention count'
                        Write-Host ("Current backup dir: {0}" -f $context.BackupDir)
                        $inBackupDir = Read-Host 'New backup dir (relative or absolute)'
                        Write-Host ("Current smoke timeout sec: {0}" -f $context.SmokeTimeoutSec)
                        $inSmokeTimeout = Read-Host 'New smoke timeout sec'

                        $logDays = $context.LogRetentionDays
                        $backupCount = $context.BackupRetentionCount
                        $backupDir = $context.BackupDir
                        $smokeTimeout = $context.SmokeTimeoutSec

                        if (-not [string]::IsNullOrWhiteSpace($inLogDays)) { $logDays = [int]$inLogDays }
                        if (-not [string]::IsNullOrWhiteSpace($inBackupCount)) { $backupCount = [int]$inBackupCount }
                        if (-not [string]::IsNullOrWhiteSpace($inBackupDir)) { $backupDir = [string]$inBackupDir }
                        if (-not [string]::IsNullOrWhiteSpace($inSmokeTimeout)) { $smokeTimeout = [int]$inSmokeTimeout }

                        $context = Save-ManagerSettings -Context $context `
                            -LogRetentionDays $logDays `
                            -BackupRetentionCount $backupCount `
                            -BackupDir $backupDir `
                            -AutostartTaskName $context.AutostartTaskName `
                            -AutostartEnabled $context.AutostartEnabled `
                            -SmokeTimeoutSec $smokeTimeout

                        Set-LastAction -Message 'Settings saved.' -Color 'Green'
                    }
                    'Q' {
                        Clear-Host
                        Write-Host 'Dashboard closed. Services keep running.' -ForegroundColor Yellow
                        Write-Host 'Quick checks:' -ForegroundColor Cyan
                        Write-Host '  .\check.ps1'
                        Write-Host '  .\install.ps1'
                        Write-Host '  .\smoke-test.ps1'
                        $launcherLabel = Get-ClientLauncherLabel -Context $context
                        Write-Host ("  {0}" -f $launcherLabel)
                        Write-Host (("  {0} keeps current terminal open (Ctrl+C to stop).") -f $launcherLabel)
                        Write-Host ''
                        exit 0
                    }
                }
            }
            catch {
                Set-LastAction -Message $_.Exception.Message -Color 'Red'
            }

            break
        }

        Start-Sleep -Milliseconds $dashboardSleepMs
    }
}
