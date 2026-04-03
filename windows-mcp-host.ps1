#requires -Version 7.0
param(
    [ValidateSet('start','stop','status')]
    [string]$Mode = 'start',
    [int]$Port = 8233,
    [int]$StartTimeoutSec = 300
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$logsDir = Join-Path $PSScriptRoot 'logs'
New-Item -ItemType Directory -Force -Path $logsDir | Out-Null
$stdoutLog = Join-Path $logsDir 'windows-mcp.stdout.log'
$stderrLog = Join-Path $logsDir 'windows-mcp.stderr.log'

function Get-PortOwner {
    param([int]$TargetPort)
    $conn = Get-NetTCPConnection -LocalAddress '127.0.0.1' -LocalPort $TargetPort -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $conn) { return $null }
    $proc = Get-CimInstance Win32_Process -Filter "ProcessId = $($conn.OwningProcess)" -ErrorAction SilentlyContinue
    [pscustomobject]@{
        ProcessId = [int]$conn.OwningProcess
        Name = if ($proc) { [string]$proc.Name } else { '' }
        CommandLine = if ($proc) { [string]$proc.CommandLine } else { '' }
    }
}

function Test-WindowsMcpProcess {
    param($Owner)
    if (-not $Owner) { return $false }
    $cmd = [string]$Owner.CommandLine
    return ($cmd -match 'windows-mcp')
}

function Start-WindowsMcp {
    param(
        [int]$TargetPort,
        [int]$TimeoutSec
    )

    $owner = Get-PortOwner -TargetPort $TargetPort
    if ($owner) {
        if (Test-WindowsMcpProcess -Owner $owner) {
            Write-Output ("Windows-MCP already running on port {0} (PID {1})." -f $TargetPort, $owner.ProcessId)
            return
        }
        throw ("Port {0} is already used by another process ({1} PID {2})." -f $TargetPort, $owner.Name, $owner.ProcessId)
    }

    $uvx = Get-Command uvx -ErrorAction SilentlyContinue
    if (-not $uvx) {
        throw 'uvx not found. Install UV first, then retry.'
    }

    $args = @('windows-mcp','--transport','streamable-http','--host','127.0.0.1','--port',"$TargetPort")
    Start-Process -FilePath $uvx.Source -ArgumentList $args -WindowStyle Hidden -RedirectStandardOutput $stdoutLog -RedirectStandardError $stderrLog | Out-Null

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 500
        $owner = Get-PortOwner -TargetPort $TargetPort
        if ($owner -and (Test-WindowsMcpProcess -Owner $owner)) {
            Write-Output ("Windows-MCP started on http://127.0.0.1:{0}/mcp (PID {1})." -f $TargetPort, $owner.ProcessId)
            return
        }
    }

    throw ("Windows-MCP did not become ready on port {0} within {1}s. Check logs: {2}, {3}" -f $TargetPort, $TimeoutSec, $stdoutLog, $stderrLog)
}

function Stop-WindowsMcp {
    param([int]$TargetPort)

    $owner = Get-PortOwner -TargetPort $TargetPort
    if (-not $owner) {
        Write-Output ("Windows-MCP is not running on port {0}." -f $TargetPort)
        return
    }
    if (-not (Test-WindowsMcpProcess -Owner $owner)) {
        throw ("Port {0} belongs to another process ({1} PID {2}); not stopping." -f $TargetPort, $owner.Name, $owner.ProcessId)
    }

    Stop-Process -Id $owner.ProcessId -Force -ErrorAction Stop
    Write-Output ("Windows-MCP stopped (PID {0})." -f $owner.ProcessId)
}

function Show-Status {
    param([int]$TargetPort)

    $owner = Get-PortOwner -TargetPort $TargetPort
    if ($owner -and (Test-WindowsMcpProcess -Owner $owner)) {
        Write-Output ("Windows-MCP: RUNNING on http://127.0.0.1:{0}/mcp (PID {1})" -f $TargetPort, $owner.ProcessId)
    }
    elseif ($owner) {
        Write-Output ("Windows-MCP: PORT BUSY by {0} (PID {1})" -f $owner.Name, $owner.ProcessId)
    }
    else {
        Write-Output 'Windows-MCP: OFFLINE'
    }
}

switch ($Mode) {
    'start' { Start-WindowsMcp -TargetPort $Port -TimeoutSec $StartTimeoutSec }
    'stop' { Stop-WindowsMcp -TargetPort $Port }
    'status' { Show-Status -TargetPort $Port }
}
