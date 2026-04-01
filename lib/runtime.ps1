Set-StrictMode -Version Latest

$moduleRoot = Join-Path $PSScriptRoot 'modules'
foreach ($moduleName in @('manifest.ps1', 'source-policy.ps1', 'auth.ps1', 'client.ps1', 'verification.ps1')) {
    $modulePath = Join-Path $moduleRoot $moduleName
    if (Test-Path -LiteralPath $modulePath) {
        . $modulePath
    }
}

function Read-JsonFile {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Missing file: $Path"
    }

    return (Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json)
}

function Write-JsonFile {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        $Value
    )

    $json = $Value | ConvertTo-Json -Depth 20
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $json, $utf8NoBom)
}

function Get-TextSha256 {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyString()]
        [string]$Text
    )

    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
        $hashBytes = $sha.ComputeHash($bytes)
        return ([BitConverter]::ToString($hashBytes)).Replace('-', '')
    }
    finally {
        $sha.Dispose()
    }
}

function Get-ServerInstallerModel {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Raw
    )

    $autoInstall = $false
    $installTarget = 'none'
    $installMethod = 'none'
    $installPackage = ''
    $binaryName = ''
    $verifyCommand = ''
    $postInstallMode = 'none'

    if ($Raw) {
        if ($null -ne $Raw.autoInstall) {
            $autoInstall = [bool]$Raw.autoInstall
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$Raw.installTarget)) {
            $installTarget = [string]$Raw.installTarget
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$Raw.installMethod)) {
            $installMethod = [string]$Raw.installMethod
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$Raw.installPackage)) {
            $installPackage = [string]$Raw.installPackage
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$Raw.binaryName)) {
            $binaryName = [string]$Raw.binaryName
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$Raw.verifyCommand)) {
            $verifyCommand = [string]$Raw.verifyCommand
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$Raw.postInstallMode)) {
            $postInstallMode = [string]$Raw.postInstallMode
        }
    }

    return [pscustomobject]@{
        AutoInstall     = $autoInstall
        InstallTarget   = $installTarget.ToLowerInvariant()
        InstallMethod   = $installMethod.ToLowerInvariant()
        InstallPackage  = $installPackage
        BinaryName      = $binaryName
        VerifyCommand   = $verifyCommand
        PostInstallMode = $postInstallMode.ToLowerInvariant()
    }
}

function Test-InstallerRecipeDefined {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Installer
    )

    if (-not $Installer) {
        return $false
    }

    if (-not [bool]$Installer.AutoInstall) {
        return $false
    }

    if ([string]::IsNullOrWhiteSpace([string]$Installer.InstallTarget) -or [string]$Installer.InstallTarget -eq 'none') {
        return $false
    }

    if ([string]::IsNullOrWhiteSpace([string]$Installer.InstallMethod) -or [string]$Installer.InstallMethod -eq 'none') {
        return $false
    }

    if ([string]::IsNullOrWhiteSpace([string]$Installer.BinaryName)) {
        return $false
    }

    if ([string]::IsNullOrWhiteSpace([string]$Installer.InstallPackage)) {
        return $false
    }

    return $true
}

function Read-ServerInstallState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        return [pscustomobject]@{}
    }

    try {
        $raw = Read-JsonFile -Path $Path
        if ($raw) {
            return $raw
        }
    }
    catch {
    }

    return [pscustomobject]@{}
}

function Write-ServerInstallState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        $State
    )

    Write-JsonFile -Path $Path -Value $State
}

function Get-ServerInstallRecordFromMap {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $InstallState,
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    if ($InstallState) {
        $property = $InstallState.PSObject.Properties | Where-Object { [string]$_.Name -eq $Name } | Select-Object -First 1
        if ($property) {
            return $property.Value
        }
    }

    return [pscustomobject]@{
        installStatus   = ''
        installError    = ''
        binaryPresent   = $false
        lastAttemptedAt = ''
        lastUpdatedAt   = ''
        installTarget   = ''
        installMethod   = ''
        installPackage  = ''
    }
}

function Set-ServerInstallRecordInMap {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $InstallState,
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        $Record
    )

    $map = [ordered]@{}
    if ($InstallState) {
        foreach ($prop in $InstallState.PSObject.Properties) {
            $map[$prop.Name] = $prop.Value
        }
    }

    $map[$Name] = $Record
    return [pscustomobject]$map
}

function Get-HubRuntimeStateModel {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Raw
    )

    $appliedSettingsHash = ''
    $appliedAt = ''
    if ($Raw) {
        if (-not [string]::IsNullOrWhiteSpace([string]$Raw.appliedSettingsHash)) {
            $appliedSettingsHash = [string]$Raw.appliedSettingsHash
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$Raw.appliedAt)) {
            $appliedAt = [string]$Raw.appliedAt
        }
    }

    return [pscustomobject]@{
        appliedSettingsHash = $appliedSettingsHash
        appliedAt           = $appliedAt
    }
}

function Read-HubRuntimeState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        return (Get-HubRuntimeStateModel)
    }

    try {
        $raw = Read-JsonFile -Path $Path
        return (Get-HubRuntimeStateModel -Raw $raw)
    }
    catch {
        return (Get-HubRuntimeStateModel)
    }
}

function Write-HubRuntimeState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        [string]$AppliedSettingsHash
    )

    $state = [pscustomobject]@{
        appliedSettingsHash = $AppliedSettingsHash
        appliedAt           = (Get-Date).ToString('o')
    }

    Write-JsonFile -Path $Path -Value $state
    return $state
}

function Get-SettingsServerByName {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Settings,
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    if (-not $Settings) {
        return $null
    }

    $serversProperty = @($Settings.PSObject.Properties | Where-Object {
        $_.MemberType -in @('NoteProperty', 'Property') -and [string]$_.Name -eq 'mcpServers'
    } | Select-Object -First 1)
    if (-not $serversProperty) {
        return $null
    }

    $servers = $serversProperty[0].Value
    if (-not $servers) {
        return $null
    }

    foreach ($prop in @($servers.PSObject.Properties | Where-Object { $_.MemberType -in @('NoteProperty', 'Property') })) {
        if ([string]$prop.Name -eq $Name) {
            return $prop.Value
        }
    }

    return $null
}

function Get-JsonLikePropertyCount {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Value
    )

    if (-not $Value) {
        return 0
    }

    if ($Value -is [System.Collections.IDictionary]) {
        return @($Value.Keys).Count
    }

    if ($Value -is [pscustomobject]) {
        return @($Value.PSObject.Properties | Where-Object { $_.MemberType -in @('NoteProperty', 'Property') }).Count
    }

    return 0
}

function Copy-JsonLikeValue {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Value
    )

    if ($null -eq $Value) {
        return $null
    }

    if ($Value -is [string] -or $Value -is [ValueType]) {
        return $Value
    }

    if ($Value -is [System.Collections.IDictionary]) {
        $map = [ordered]@{}
        foreach ($key in $Value.Keys) {
            $childValue = $Value[$key]
            if (Test-JsonArrayLikeValue -Value $childValue) {
                $map[$key] = @(Copy-JsonLikeValue -Value $childValue)
            }
            else {
                $map[$key] = Copy-JsonLikeValue -Value $childValue
            }
        }
        return [pscustomobject]$map
    }

    if ($Value -is [pscustomobject]) {
        $map = [ordered]@{}
        foreach ($prop in @($Value.PSObject.Properties | Where-Object { $_.MemberType -in @('NoteProperty', 'Property') })) {
            if (Test-JsonArrayLikeValue -Value $prop.Value) {
                $map[$prop.Name] = @(Copy-JsonLikeValue -Value $prop.Value)
            }
            else {
                $map[$prop.Name] = Copy-JsonLikeValue -Value $prop.Value
            }
        }
        return [pscustomobject]$map
    }

    if ($Value -is [System.Collections.IEnumerable] -and -not ($Value -is [string])) {
        $items = @()
        foreach ($item in $Value) {
            if (Test-JsonArrayLikeValue -Value $item) {
                $items += ,(@(Copy-JsonLikeValue -Value $item))
            }
            else {
                $items += ,(Copy-JsonLikeValue -Value $item)
            }
        }
        return $items
    }

    return $Value
}

function ConvertTo-StableJsonLikeValue {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Value
    )

    if ($null -eq $Value) {
        return $null
    }

    if ($Value -is [string] -or $Value -is [ValueType]) {
        return $Value
    }

    if ($Value -is [System.Collections.IDictionary]) {
        $map = [ordered]@{}
        foreach ($key in @($Value.Keys | Sort-Object)) {
            $childValue = $Value[$key]
            if (Test-JsonArrayLikeValue -Value $childValue) {
                $map[$key] = @(ConvertTo-StableJsonLikeValue -Value $childValue)
            }
            else {
                $map[$key] = ConvertTo-StableJsonLikeValue -Value $childValue
            }
        }
        return [pscustomobject]$map
    }

    if ($Value -is [pscustomobject]) {
        $map = [ordered]@{}
        foreach ($prop in @($Value.PSObject.Properties | Where-Object { $_.MemberType -in @('NoteProperty', 'Property') } | Sort-Object Name)) {
            if (Test-JsonArrayLikeValue -Value $prop.Value) {
                $map[$prop.Name] = @(ConvertTo-StableJsonLikeValue -Value $prop.Value)
            }
            else {
                $map[$prop.Name] = ConvertTo-StableJsonLikeValue -Value $prop.Value
            }
        }
        return [pscustomobject]$map
    }

    if ($Value -is [System.Collections.IEnumerable] -and -not ($Value -is [string])) {
        $items = @()
        foreach ($item in $Value) {
            if (Test-JsonArrayLikeValue -Value $item) {
                $items += ,(@(ConvertTo-StableJsonLikeValue -Value $item))
            }
            else {
                $items += ,(ConvertTo-StableJsonLikeValue -Value $item)
            }
        }
        return $items
    }

    return $Value
}

function Test-JsonLikeEqual {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Left,
        [Parameter(Mandatory = $false)]
        $Right
    )

    $leftJson = ConvertTo-Json -InputObject (ConvertTo-StableJsonLikeValue -Value $Left) -Depth 20 -Compress
    $rightJson = ConvertTo-Json -InputObject (ConvertTo-StableJsonLikeValue -Value $Right) -Depth 20 -Compress
    return ($leftJson -eq $rightJson)
}

function Test-JsonLikeValueEmpty {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Value
    )

    if ($null -eq $Value) {
        return $true
    }

    if ($Value -is [string]) {
        return [string]::IsNullOrWhiteSpace($Value)
    }

    if ($Value -is [System.Collections.IDictionary]) {
        return (@($Value.Keys).Count -eq 0)
    }

    if ($Value -is [pscustomobject]) {
        return (@($Value.PSObject.Properties | Where-Object { $_.MemberType -in @('NoteProperty', 'Property') }).Count -eq 0)
    }

    if ($Value -is [System.Collections.IEnumerable]) {
        return (@($Value).Count -eq 0)
    }

    return $false
}

function Get-LocalServerOverrideEntryModel {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Raw
    )

    $entry = [ordered]@{}
    if ($Raw) {
        if ($Raw.PSObject.Properties.Name -contains 'enabled' -and $null -ne $Raw.enabled) {
            $entry['enabled'] = [bool]$Raw.enabled
        }
        if ($Raw.PSObject.Properties.Name -contains 'oauth' -and -not (Test-JsonLikeValueEmpty -Value $Raw.oauth)) {
            $entry['oauth'] = Copy-JsonLikeValue -Value $Raw.oauth
        }
    }

    return [pscustomobject]$entry
}

function Get-LocalServerOverridesModel {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Raw
    )

    $servers = [ordered]@{}
    if ($Raw -and ($Raw.PSObject.Properties.Name -contains 'mcpServers') -and $Raw.mcpServers) {
        foreach ($prop in @($Raw.mcpServers.PSObject.Properties | Where-Object { $_.MemberType -in @('NoteProperty', 'Property') })) {
            $entry = Get-LocalServerOverrideEntryModel -Raw $prop.Value
            if ((Get-JsonLikePropertyCount -Value $entry) -gt 0) {
                $servers[[string]$prop.Name] = $entry
            }
        }
    }

    return [pscustomobject]@{
        mcpServers = [pscustomobject]$servers
    }
}

function Read-LocalServerOverrides {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return (Get-LocalServerOverridesModel)
    }

    try {
        $raw = Read-JsonFile -Path $Path
        return (Get-LocalServerOverridesModel -Raw $raw)
    }
    catch {
        return (Get-LocalServerOverridesModel)
    }
}

function Write-LocalServerOverrides {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        $Overrides
    )

    $normalized = Get-LocalServerOverridesModel -Raw $Overrides
    $serverCount = Get-JsonLikePropertyCount -Value $normalized.mcpServers
    if ($serverCount -eq 0) {
        Remove-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
        return $normalized
    }

    $directory = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($directory)) {
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
    }

    Write-JsonFile -Path $Path -Value $normalized
    return $normalized
}

function Test-EffectiveServerEnabledHarvestIsAmbiguous {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Server
    )

    if (
        -not $Server -or
        -not ($Server.PSObject.Properties.Name -contains '_manager') -or
        -not $Server._manager
    ) {
        return $true
    }

    $manager = $Server._manager
    if (
        -not ($manager.PSObject.Properties.Name -contains 'configuredEnabled') -or
        -not ($manager.PSObject.Properties.Name -contains 'effectiveEnabled')
    ) {
        return $true
    }

    return ([bool]$manager.configuredEnabled -ne [bool]$manager.effectiveEnabled)
}

function Merge-LocalServerOverridesFromEffectiveSettings {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        $BaselineSettings,
        [Parameter(Mandatory = $false)]
        $EffectiveSettings,
        [Parameter(Mandatory = $false)]
        $ExistingOverrides
    )

    $existing = Get-LocalServerOverridesModel -Raw $ExistingOverrides
    if (-not $EffectiveSettings -or -not $EffectiveSettings.mcpServers) {
        return $existing
    }

    $servers = [ordered]@{}
    foreach ($prop in @($BaselineSettings.mcpServers.PSObject.Properties | Where-Object { $_.MemberType -in @('NoteProperty', 'Property') })) {
        $name = [string]$prop.Name
        $baselineServer = $prop.Value
        $effectiveServer = Get-SettingsServerByName -Settings $EffectiveSettings -Name $name
        $existingEntry = Get-SettingsServerByName -Settings $existing -Name $name
        $entry = [ordered]@{}

        $baselineEnabled = $false
        if ($baselineServer.PSObject.Properties.Name -contains 'enabled' -and $null -ne $baselineServer.enabled) {
            $baselineEnabled = [bool]$baselineServer.enabled
        }

        $effectiveEnabled = $baselineEnabled
        if ($effectiveServer -and ($effectiveServer.PSObject.Properties.Name -contains 'enabled') -and $null -ne $effectiveServer.enabled) {
            $effectiveEnabled = [bool]$effectiveServer.enabled
        }

        $existingHasEnabled = $false
        $existingEnabled = $false
        if ($existingEntry -and ($existingEntry.PSObject.Properties.Name -contains 'enabled') -and $null -ne $existingEntry.enabled) {
            $existingHasEnabled = $true
            $existingEnabled = [bool]$existingEntry.enabled
        }

        if ($effectiveEnabled -ne $baselineEnabled) {
            $entry['enabled'] = $effectiveEnabled
        }
        elseif ($existingHasEnabled -and (Test-EffectiveServerEnabledHarvestIsAmbiguous -Server $effectiveServer)) {
            $entry['enabled'] = $existingEnabled
        }

        $baselineOauth = if ($baselineServer.PSObject.Properties.Name -contains 'oauth') { $baselineServer.oauth } else { $null }
        $effectiveOauth = if ($effectiveServer -and ($effectiveServer.PSObject.Properties.Name -contains 'oauth')) { $effectiveServer.oauth } else { $null }
        if (
            -not (Test-JsonLikeEqual -Left $baselineOauth -Right $effectiveOauth) -and
            -not (Test-JsonLikeValueEmpty -Value $effectiveOauth)
        ) {
            $entry['oauth'] = Copy-JsonLikeValue -Value $effectiveOauth
        }

        if ($entry.Count -gt 0) {
            $servers[$name] = [pscustomobject]$entry
        }
    }

    return [pscustomobject]@{
        mcpServers = [pscustomobject]$servers
    }
}

function Apply-LocalServerOverridesToSettings {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        $Settings,
        [Parameter(Mandatory = $false)]
        $Overrides
    )

    if (
        -not $Overrides -or
        -not ($Overrides.PSObject.Properties.Name -contains 'mcpServers') -or
        -not $Overrides.mcpServers
    ) {
        return $Settings
    }

    foreach ($prop in @($Overrides.mcpServers.PSObject.Properties | Where-Object { $_.MemberType -in @('NoteProperty', 'Property') })) {
        $name = [string]$prop.Name
        $overrideEntry = $prop.Value
        $server = Get-SettingsServerByName -Settings $Settings -Name $name
        if (-not $server) {
            continue
        }

        if ($overrideEntry.PSObject.Properties.Name -contains 'enabled' -and $null -ne $overrideEntry.enabled) {
            $server.enabled = [bool]$overrideEntry.enabled
        }
        if ($overrideEntry.PSObject.Properties.Name -contains 'oauth') {
            $server | Add-Member -NotePropertyName 'oauth' -NotePropertyValue (Copy-JsonLikeValue -Value $overrideEntry.oauth) -Force
        }
    }

    return $Settings
}

function Get-ManagerSettingsModel {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Raw
    )

    $logRetentionDays = 14
    $backupRetentionCount = 10
    $backupDir = 'backups'
    $autostartTaskName = 'MCPace Autostart'
    $autostartEnabled = $false
    $smokeTimeoutSec = 30

    if ($Raw) {
        if ($Raw.maintenance) {
            if ($null -ne $Raw.maintenance.logRetentionDays) {
                $candidate = [int]$Raw.maintenance.logRetentionDays
                if ($candidate -gt 0) { $logRetentionDays = $candidate }
            }
            if ($null -ne $Raw.maintenance.backupRetentionCount) {
                $candidate = [int]$Raw.maintenance.backupRetentionCount
                if ($candidate -gt 0) { $backupRetentionCount = $candidate }
            }
            if (-not [string]::IsNullOrWhiteSpace([string]$Raw.maintenance.backupDir)) {
                $backupDir = [string]$Raw.maintenance.backupDir
            }
        }
        if ($Raw.autostart) {
            if (-not [string]::IsNullOrWhiteSpace([string]$Raw.autostart.taskName)) {
                $autostartTaskName = [string]$Raw.autostart.taskName
            }
            if ($null -ne $Raw.autostart.enabled) {
                $autostartEnabled = [bool]$Raw.autostart.enabled
            }
        }
        if ($Raw.smokeTest -and $null -ne $Raw.smokeTest.timeoutSec) {
            $candidate = [int]$Raw.smokeTest.timeoutSec
            if ($candidate -gt 0) { $smokeTimeoutSec = $candidate }
        }
    }

    return [pscustomobject]@{
        maintenance = [pscustomobject]@{
            logRetentionDays    = $logRetentionDays
            backupRetentionCount = $backupRetentionCount
            backupDir           = $backupDir
        }
        autostart = [pscustomobject]@{
            taskName = $autostartTaskName
            enabled  = $autostartEnabled
        }
        smokeTest = [pscustomobject]@{
            timeoutSec = $smokeTimeoutSec
        }
    }
}

function Read-ManagerSettings {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        $defaults = Get-ManagerSettingsModel
        Write-JsonFile -Path $Path -Value $defaults
        return $defaults
    }

    try {
        $raw = Read-JsonFile -Path $Path
        return (Get-ManagerSettingsModel -Raw $raw)
    }
    catch {
        $defaults = Get-ManagerSettingsModel
        Write-JsonFile -Path $Path -Value $defaults
        return $defaults
    }
}

function Save-ManagerSettings {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $true)][int]$LogRetentionDays,
        [Parameter(Mandatory = $true)][int]$BackupRetentionCount,
        [Parameter(Mandatory = $true)][string]$BackupDir,
        [Parameter(Mandatory = $true)][string]$AutostartTaskName,
        [Parameter(Mandatory = $true)][bool]$AutostartEnabled,
        [Parameter(Mandatory = $true)][int]$SmokeTimeoutSec
    )

    if ($LogRetentionDays -lt 1) { throw 'logRetentionDays must be >= 1.' }
    if ($BackupRetentionCount -lt 1) { throw 'backupRetentionCount must be >= 1.' }
    if ([string]::IsNullOrWhiteSpace($BackupDir)) { throw 'backupDir cannot be empty.' }
    if ([string]::IsNullOrWhiteSpace($AutostartTaskName)) { throw 'autostart task name cannot be empty.' }
    if ($SmokeTimeoutSec -lt 5) { throw 'smoke timeout must be >= 5 sec.' }

    $settings = [pscustomobject]@{
        maintenance = [pscustomobject]@{
            logRetentionDays    = $LogRetentionDays
            backupRetentionCount = $BackupRetentionCount
            backupDir           = $BackupDir
        }
        autostart = [pscustomobject]@{
            taskName = $AutostartTaskName
            enabled  = $AutostartEnabled
        }
        smokeTest = [pscustomobject]@{
            timeoutSec = $SmokeTimeoutSec
        }
    }

    Write-JsonFile -Path $Context.ManagerSettingsPath -Value $settings
    return (New-McpAceContext -RootPath $Context.RootPath -StateRoot $Context.StateRoot)
}

function Get-WorkspaceBindingModel {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Raw
    )

    $defaultCwd = 'none'
    $pathArgs = 'none'
    $isolateHome = $false

    if ($Raw) {
        if (-not [string]::IsNullOrWhiteSpace([string]$Raw.defaultCwd)) {
            switch ([string]$Raw.defaultCwd.ToLowerInvariant()) {
                'primary' { $defaultCwd = 'primary' }
                default { $defaultCwd = 'none' }
            }
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$Raw.pathArgs)) {
            switch ([string]$Raw.pathArgs.ToLowerInvariant()) {
                'allroots' { $pathArgs = 'allRoots' }
                default { $pathArgs = 'none' }
            }
        }
        if ($null -ne $Raw.isolateHome) {
            $isolateHome = [bool]$Raw.isolateHome
        }
    }

    return [pscustomobject]@{
        DefaultCwd = $defaultCwd
        PathArgs   = $pathArgs
        IsolateHome = $isolateHome
    }
}

function ConvertTo-WorkspaceIdentifier {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    if ([string]::IsNullOrWhiteSpace($Name)) {
        throw 'workspace name cannot be empty.'
    }

    if ($Name -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]*$') {
        throw ("workspace name '{0}' is invalid. Use letters, digits, dot, dash or underscore." -f $Name)
    }

    return $Name.ToLowerInvariant()
}

function ConvertTo-WorkspaceEnvSuffix {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    if ([string]::IsNullOrWhiteSpace($Name)) {
        throw 'workspace name cannot be empty.'
    }

    $result = ($Name.ToUpperInvariant() -replace '[^A-Z0-9]', '_')
    $result = ($result -replace '_+', '_').Trim('_')
    if ([string]::IsNullOrWhiteSpace($result)) {
        throw ("workspace name '{0}' cannot be converted to an env-safe identifier." -f $Name)
    }

    return $result
}

function Resolve-WorkspaceHostPath {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$ManagerRoot,
        [Parameter(Mandatory = $true)]
        [string]$HostPath
    )

    if ([string]::IsNullOrWhiteSpace($HostPath)) {
        throw 'workspace hostPath cannot be empty.'
    }

    $candidate = if ([System.IO.Path]::IsPathRooted($HostPath)) {
        $HostPath
    }
    else {
        Join-Path $ManagerRoot $HostPath
    }

    try {
        return (Resolve-Path -LiteralPath $candidate).Path
    }
    catch {
        throw ("workspace path does not exist: {0}" -f $candidate)
    }
}

function New-WorkspaceEntry {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [string]$HostPath,
        [Parameter(Mandatory = $true)]
        [string]$Access,
        [Parameter(Mandatory = $true)]
        [string]$ManagerRoot,
        [Parameter(Mandatory = $true)]
        [bool]$IsPrimary
    )

    $normalizedName = $Name.Trim()
    $normalizedAccess = if ([string]::IsNullOrWhiteSpace($Access)) { 'rw' } else { $Access.Trim().ToLowerInvariant() }
    if ($normalizedAccess -notin @('rw', 'ro')) {
        throw ("workspace '{0}' has invalid access '{1}'. Use 'rw' or 'ro'." -f $normalizedName, $Access)
    }

    $identifier = ConvertTo-WorkspaceIdentifier -Name $normalizedName
    $envSuffix = ConvertTo-WorkspaceEnvSuffix -Name $normalizedName
    $resolvedHostPath = Resolve-WorkspaceHostPath -ManagerRoot $ManagerRoot -HostPath $HostPath
    $canonicalContainerPath = "/workspaces/$identifier"
    $exposedPaths = @($canonicalContainerPath)
    $compatibilityPath = ''
    if ($IsPrimary) {
        $compatibilityPath = '/workspace'
        $exposedPaths = @($compatibilityPath, $canonicalContainerPath)
    }

    return [pscustomobject]@{
        Name                     = $normalizedName
        Identifier               = $identifier
        EnvSuffix                = $envSuffix
        HostPath                 = $resolvedHostPath
        Access                   = $normalizedAccess
        ReadOnly                 = ($normalizedAccess -eq 'ro')
        IsPrimary                = $IsPrimary
        CompatibilityContainerPath = $compatibilityPath
        CanonicalContainerPath   = $canonicalContainerPath
        ExposedContainerPaths    = @($exposedPaths)
    }
}

function Get-WorkspaceRegistry {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Config,
        [Parameter(Mandatory = $true)]
        [string]$ManagerRoot
    )

    $hasWorkspaceConfig = $false
    $primaryWorkspace = $null
    $extras = @()

    if ($Config -and ($Config.PSObject.Properties.Name -contains 'workspaces') -and $Config.workspaces) {
        $hasWorkspaceConfig = $true
        if (-not $Config.workspaces.primary) {
            throw 'mcpace.config.json workspaces.primary is required when workspaces is configured.'
        }

        $primaryName = if (-not [string]::IsNullOrWhiteSpace([string]$Config.workspaces.primary.name)) { [string]$Config.workspaces.primary.name } else { 'primary' }
        if ([string]::IsNullOrWhiteSpace([string]$Config.workspaces.primary.hostPath)) {
            throw 'mcpace.config.json workspaces.primary.hostPath is required.'
        }
        $primaryAccess = if (-not [string]::IsNullOrWhiteSpace([string]$Config.workspaces.primary.access)) { [string]$Config.workspaces.primary.access } else { 'rw' }
        $primaryWorkspace = New-WorkspaceEntry -Name $primaryName -HostPath ([string]$Config.workspaces.primary.hostPath) -Access $primaryAccess -ManagerRoot $ManagerRoot -IsPrimary $true

        if ($Config.workspaces.PSObject.Properties.Name -contains 'extras') {
            foreach ($rawExtra in @($Config.workspaces.extras)) {
                if (-not $rawExtra) { continue }
                if ([string]::IsNullOrWhiteSpace([string]$rawExtra.name)) {
                    throw 'mcpace.config.json workspaces.extras[].name is required.'
                }
                if ([string]::IsNullOrWhiteSpace([string]$rawExtra.hostPath)) {
                    throw ("mcpace.config.json workspaces.extras[{0}].hostPath is required." -f [string]$rawExtra.name)
                }
                $extraAccess = if (-not [string]::IsNullOrWhiteSpace([string]$rawExtra.access)) { [string]$rawExtra.access } else { 'ro' }
                $extras += New-WorkspaceEntry -Name ([string]$rawExtra.name) -HostPath ([string]$rawExtra.hostPath) -Access $extraAccess -ManagerRoot $ManagerRoot -IsPrimary $false
            }
        }
    }
    else {
        $primaryWorkspace = New-WorkspaceEntry -Name 'primary' -HostPath $ManagerRoot -Access 'rw' -ManagerRoot $ManagerRoot -IsPrimary $true
    }

    $all = @($primaryWorkspace) + @($extras)
    $nameSeen = @{}
    $identifierSeen = @{}
    $envSeen = @{}
    foreach ($workspace in $all) {
        $nameKey = $workspace.Name.ToLowerInvariant()
        if ($nameSeen.ContainsKey($nameKey)) {
            throw ("workspace name '{0}' is duplicated." -f $workspace.Name)
        }
        $nameSeen[$nameKey] = $true

        if ($identifierSeen.ContainsKey($workspace.Identifier)) {
            throw ("workspace identifier collision for '{0}'. Rename one of the workspaces." -f $workspace.Name)
        }
        $identifierSeen[$workspace.Identifier] = $true

        if ($envSeen.ContainsKey($workspace.EnvSuffix)) {
            throw ("workspace env identifier collision for '{0}'. Rename one of the workspaces." -f $workspace.Name)
        }
        $envSeen[$workspace.EnvSuffix] = $true
    }

    $mounts = @(
        [pscustomobject]@{
            WorkspaceName = $primaryWorkspace.Name
            ContainerPath = $primaryWorkspace.CompatibilityContainerPath
            HostPath      = $primaryWorkspace.HostPath
            Access        = $primaryWorkspace.Access
            ReadOnly      = $primaryWorkspace.ReadOnly
            Kind          = 'compatibility'
        },
        [pscustomobject]@{
            WorkspaceName = $primaryWorkspace.Name
            ContainerPath = $primaryWorkspace.CanonicalContainerPath
            HostPath      = $primaryWorkspace.HostPath
            Access        = $primaryWorkspace.Access
            ReadOnly      = $primaryWorkspace.ReadOnly
            Kind          = 'canonical'
        }
    )
    foreach ($workspace in $extras) {
        $mounts += [pscustomobject]@{
            WorkspaceName = $workspace.Name
            ContainerPath = $workspace.CanonicalContainerPath
            HostPath      = $workspace.HostPath
            Access        = $workspace.Access
            ReadOnly      = $workspace.ReadOnly
            Kind          = 'canonical'
        }
    }

    $exposedPaths = @()
    foreach ($workspace in $all) {
        foreach ($path in @($workspace.ExposedContainerPaths)) {
            if ($exposedPaths -notcontains $path) {
                $exposedPaths += $path
            }
        }
    }

    $placeholderVariables = [ordered]@{
        MCPACE_PRIMARY_WORKSPACE = $primaryWorkspace.CompatibilityContainerPath
        MCPACE_WORKSPACES_ROOT   = '/workspaces'
        MCPACE_MANAGER_DATA      = '/app/data'
    }
    foreach ($workspace in $all) {
        $placeholderVariables["MCPACE_WORKSPACE_$($workspace.EnvSuffix)"] = $workspace.CanonicalContainerPath
    }

    return [pscustomobject]@{
        ManagerRoot          = $ManagerRoot
        HasExplicitConfig    = $hasWorkspaceConfig
        Primary              = $primaryWorkspace
        Extras               = @($extras)
        All                  = @($all)
        Mounts               = @($mounts)
        ExposedContainerPaths = @($exposedPaths)
        PlaceholderVariables = [pscustomobject]$placeholderVariables
    }
}

function Test-WorkspaceContainerPathArgument {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Value,
        [Parameter(Mandatory = $true)]$WorkspaceRegistry
    )

    return (@($WorkspaceRegistry.ExposedContainerPaths) -contains $Value)
}

function Wrap-ServerCommandWithShellBinding {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$ServerName,
        [Parameter(Mandatory = $true)]
        [string]$Command,
        [Parameter(Mandatory = $false)]
        [AllowEmptyCollection()]
        [array]$Args = @(),
        [Parameter(Mandatory = $true)]$Binding,
        [Parameter(Mandatory = $true)]$WorkspaceRegistry
    )

    $scriptParts = @()
    if ([bool]$Binding.IsolateHome) {
        $serverId = ConvertTo-WorkspaceIdentifier -Name $ServerName
        $homePath = "/app/data/server-state/$serverId"
        $scriptParts += ("mkdir -p {0}" -f (ConvertTo-PosixShellLiteral -Value $homePath))
        $scriptParts += ("export HOME={0}" -f (ConvertTo-PosixShellLiteral -Value $homePath))
    }
    if ([string]$Binding.DefaultCwd -eq 'primary') {
        $scriptParts += ("cd {0}" -f (ConvertTo-PosixShellLiteral -Value $WorkspaceRegistry.Primary.CompatibilityContainerPath))
    }

    $commandParts = @((ConvertTo-PosixShellLiteral -Value $Command))
    foreach ($arg in @($Args)) {
        if ($null -eq $arg) {
            continue
        }
        $argText = [string]$arg
        if ([string]::IsNullOrEmpty($argText)) {
            continue
        }
        $commandParts += (ConvertTo-PosixShellLiteral -Value $argText)
    }
    $scriptParts += ("exec {0}" -f ($commandParts -join ' '))

    return [pscustomobject]@{
        Command = 'sh'
        Args    = @('-lc', ($scriptParts -join '; '))
    }
}

function Apply-WorkspaceAwareServerTransforms {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Settings,
        [Parameter(Mandatory = $true)]$ServerDefinitions,
        [Parameter(Mandatory = $true)]$WorkspaceRegistry
    )

    if (-not $Settings -or -not $Settings.mcpServers) {
        return $Settings
    }

    foreach ($prop in $Settings.mcpServers.PSObject.Properties) {
        $name = [string]$prop.Name
        $server = $prop.Value
        if (-not $server) { continue }

        $definition = Get-ServerDefinitionFromMap -ServerDefinitions $ServerDefinitions -Name $name
        if ([string]$definition.Kind -ne 'container-stdio') {
            continue
        }

        $binding = $definition.WorkspaceBinding
        if ([string]$binding.PathArgs -eq 'allRoots') {
            $resolvedArgs = @()
            if ($server.PSObject.Properties.Name -contains 'args') {
                foreach ($arg in @($server.args)) {
                    $argText = [string]$arg
                    if (-not (Test-WorkspaceContainerPathArgument -Value $argText -WorkspaceRegistry $WorkspaceRegistry)) {
                        $resolvedArgs += $argText
                    }
                }
            }
            foreach ($path in @($WorkspaceRegistry.ExposedContainerPaths)) {
                if ($resolvedArgs -notcontains $path) {
                    $resolvedArgs += $path
                }
            }
            $server.args = @($resolvedArgs)
        }

        $needsShellBinding = (
            [string]$binding.DefaultCwd -eq 'primary' -or
            [bool]$binding.IsolateHome
        )
        if (
            $needsShellBinding -and
            ($server.PSObject.Properties.Name -contains 'command') -and
            -not [string]::IsNullOrWhiteSpace([string]$server.command)
        ) {
            $wrapped = Wrap-ServerCommandWithShellBinding `
                -ServerName $name `
                -Command ([string]$server.command) `
                -Args $(if ($server.PSObject.Properties.Name -contains 'args') { @($server.args) } else { @() }) `
                -Binding $binding `
                -WorkspaceRegistry $WorkspaceRegistry
            $server.command = $wrapped.Command
            $server.args = @($wrapped.Args)
        }
    }

    return $Settings
}

function Get-PlatformInfo {
    [CmdletBinding()]
    param()

    $platformIsWindows = $false
    $platformIsLinux = $false
    $platformIsMacOS = $false

    $autoIsWindows = Get-Variable -Name IsWindows -ErrorAction SilentlyContinue
    $autoIsLinux = Get-Variable -Name IsLinux -ErrorAction SilentlyContinue
    $autoIsMacOS = Get-Variable -Name IsMacOS -ErrorAction SilentlyContinue

    if ($autoIsWindows -or $autoIsLinux -or $autoIsMacOS) {
        if ($autoIsWindows) { $platformIsWindows = [bool]$autoIsWindows.Value }
        if ($autoIsLinux) { $platformIsLinux = [bool]$autoIsLinux.Value }
        if ($autoIsMacOS) { $platformIsMacOS = [bool]$autoIsMacOS.Value }
    }
    else {
        switch ([System.Environment]::OSVersion.Platform) {
            'Win32NT' { $platformIsWindows = $true }
            'Unix'    { $platformIsLinux = $true }
            'MacOSX'  { $platformIsMacOS = $true }
        }
    }

    return [pscustomobject]@{
        IsWindows = $platformIsWindows
        IsLinux   = $platformIsLinux
        IsMacOS   = $platformIsMacOS
    }
}

function Get-PreferredPowerShellCommand {
    [CmdletBinding()]
    param()

    try {
        $currentShellPath = [string](Get-Process -Id $PID -ErrorAction Stop).Path
        $currentShellLeaf = [System.IO.Path]::GetFileName($currentShellPath)
        if ($currentShellLeaf -match '^pwsh(\.exe)?$') {
            return $currentShellPath
        }
    }
    catch {}

    $cmd = Get-Command pwsh -ErrorAction SilentlyContinue
    if ($cmd) {
        if (-not [string]::IsNullOrWhiteSpace([string]$cmd.Source)) {
            return [string]$cmd.Source
        }
        return [string]$cmd.Name
    }

    return $null
}

function Get-PreferredNpxCommand {
    [CmdletBinding()]
    param()

    $platform = Get-PlatformInfo
    $candidates = if ($platform.IsWindows) {
        @('npx.cmd', 'npx.exe', 'npx')
    }
    else {
        @('npx', 'npx.cmd')
    }

    foreach ($name in $candidates) {
        $cmd = Get-Command $name -ErrorAction SilentlyContinue
        if ($cmd) {
            if (-not [string]::IsNullOrWhiteSpace([string]$cmd.Source)) {
                return [string]$cmd.Source
            }
            return [string]$cmd.Name
        }
    }

    return $null
}

function Expand-EnvString {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyString()]
        [string]$Value,
        [Parameter(Mandatory = $false)]
        $Variables
    )

    $pattern = '\$\{([A-Za-z_][A-Za-z0-9_]*)(:-([^}]*))?\}'
    $matches = [regex]::Matches($Value, $pattern)
    if ($matches.Count -eq 0) {
        return $Value
    }

    $result = $Value
    $sorted = @($matches) | Sort-Object Index -Descending
    foreach ($match in $sorted) {
        $name = [string]$match.Groups[1].Value
        $defaultValue = ''
        if ($match.Groups[3].Success) {
            $defaultValue = [string]$match.Groups[3].Value
        }
        $hasCustomValue = $false
        $customValue = ''
        if ($Variables -and ($Variables.PSObject.Properties.Name -contains $name)) {
            $hasCustomValue = $true
            $customValue = [string]$Variables.PSObject.Properties[$name].Value
        }
        $envValue = [Environment]::GetEnvironmentVariable($name)
        $resolvedValue = if ($hasCustomValue) { $customValue } else { $envValue }
        $replacement = if ([string]::IsNullOrEmpty($resolvedValue)) { $defaultValue } else { $resolvedValue }
        $result = $result.Remove($match.Index, $match.Length).Insert($match.Index, $replacement)
    }

    return $result
}

function Expand-EnvPlaceholdersInValue {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Value,
        [Parameter(Mandatory = $false)]
        $Variables
    )

    if ($null -eq $Value) {
        return $null
    }

    if ($Value -is [string]) {
        return (Expand-EnvString -Value $Value -Variables $Variables)
    }

    if ($Value -is [System.Collections.IDictionary]) {
        $map = [ordered]@{}
        foreach ($key in $Value.Keys) {
            $childValue = $Value[$key]
            if (Test-JsonArrayLikeValue -Value $childValue) {
                $map[$key] = @(Expand-EnvPlaceholdersInValue -Value $childValue -Variables $Variables)
            }
            else {
                $map[$key] = Expand-EnvPlaceholdersInValue -Value $childValue -Variables $Variables
            }
        }
        return [pscustomobject]$map
    }

    if ($Value -is [pscustomobject]) {
        $props = @($Value.PSObject.Properties | Where-Object { $_.MemberType -in @('NoteProperty', 'Property') })
        $map = [ordered]@{}
        foreach ($prop in $props) {
            if (Test-JsonArrayLikeValue -Value $prop.Value) {
                $map[$prop.Name] = @(Expand-EnvPlaceholdersInValue -Value $prop.Value -Variables $Variables)
            }
            else {
                $map[$prop.Name] = Expand-EnvPlaceholdersInValue -Value $prop.Value -Variables $Variables
            }
        }
        return [pscustomobject]$map
    }

    if ($Value -is [System.Collections.IEnumerable] -and -not ($Value -is [string])) {
        $items = @()
        foreach ($item in $Value) {
            if (Test-JsonArrayLikeValue -Value $item) {
                $items += ,(@(Expand-EnvPlaceholdersInValue -Value $item -Variables $Variables))
            }
            else {
                $items += ,(Expand-EnvPlaceholdersInValue -Value $item -Variables $Variables)
            }
        }
        return $items
    }

    return $Value
}

function Test-JsonArrayLikeValue {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Value
    )

    if ($null -eq $Value) {
        return $false
    }

    if ($Value -is [string]) {
        return $false
    }

    if ($Value -is [System.Collections.IDictionary]) {
        return $false
    }

    if ($Value -is [pscustomobject]) {
        return $false
    }

    return ($Value -is [System.Collections.IEnumerable])
}

function Get-JsonLikePropertyValue {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        [object]
        $Value,
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    if ($null -eq $Value -or [string]::IsNullOrWhiteSpace($Name)) {
        return $null
    }

    if ($Value -is [System.Collections.IDictionary]) {
        if ($Value.Contains($Name)) {
            $candidate = $Value[$Name]
            if (Test-JsonArrayLikeValue -Value $candidate) {
                return ,$candidate
            }
            return $candidate
        }
        return $null
    }

    if ($Value -is [pscustomobject]) {
        $property = @($Value.PSObject.Properties | Where-Object {
            $_.MemberType -in @('NoteProperty', 'Property') -and [string]$_.Name -eq $Name
        } | Select-Object -First 1)
        if ($property) {
            $candidate = $property[0].Value
            if (Test-JsonArrayLikeValue -Value $candidate) {
                return ,$candidate
            }
            return $candidate
        }
    }

    return $null
}

function Get-JsonValueBySegments {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        [object]
        $Value,
        [Parameter(Mandatory = $false)]
        [object[]]$Segments = @()
    )

    $current = $Value
    foreach ($segment in @($Segments)) {
        if ($null -eq $current) {
            return $null
        }

        if ($segment -is [int]) {
            if (-not (Test-JsonArrayLikeValue -Value $current)) {
                return $null
            }

            $items = @($current)
            if ($segment -lt 0 -or $segment -ge $items.Count) {
                return $null
            }

            $current = $items[$segment]
            continue
        }

        $current = Get-JsonLikePropertyValue -Value $current -Name ([string]$segment)
    }

    if (Test-JsonArrayLikeValue -Value $current) {
        return ,$current
    }

    return $current
}

function Get-JsonArrayShapeViolations {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        [object]
        $Baseline,
        [Parameter(Mandatory = $false)]
        [object]
        $Candidate,
        [Parameter(Mandatory = $false)]
        [AllowEmptyString()]
        [string]$Path = 'settings',
        [Parameter(Mandatory = $false)]
        [object[]]$Segments = @()
    )

    $violations = New-Object System.Collections.Generic.List[string]

    if (Test-JsonArrayLikeValue -Value $Baseline) {
        $candidateValue = Get-JsonValueBySegments -Value $Candidate -Segments $Segments
        if (-not (Test-JsonArrayLikeValue -Value $candidateValue)) {
            $violations.Add(("{0} must remain a JSON array." -f $Path))
            return $violations.ToArray()
        }

        $baselineItems = @($Baseline)
        $candidateItems = @($candidateValue)
        $count = [Math]::Min($baselineItems.Count, $candidateItems.Count)
        for ($index = 0; $index -lt $count; $index++) {
            foreach ($item in @(Get-JsonArrayShapeViolations -Baseline $baselineItems[$index] -Candidate $Candidate -Path ("{0}[{1}]" -f $Path, $index) -Segments (@($Segments) + $index))) {
                $violations.Add([string]$item)
            }
        }

        return $violations.ToArray()
    }

    if ($Baseline -is [System.Collections.IDictionary]) {
        foreach ($key in @($Baseline.Keys)) {
            $childPath = if ([string]::IsNullOrWhiteSpace($Path)) { [string]$key } else { "{0}.{1}" -f $Path, [string]$key }
            foreach ($item in @(Get-JsonArrayShapeViolations -Baseline $Baseline[$key] -Candidate $Candidate -Path $childPath -Segments (@($Segments) + [string]$key))) {
                $violations.Add([string]$item)
            }
        }

        return $violations.ToArray()
    }

    if ($Baseline -is [pscustomobject]) {
        foreach ($property in @($Baseline.PSObject.Properties | Where-Object { $_.MemberType -in @('NoteProperty', 'Property') })) {
            $childPath = if ([string]::IsNullOrWhiteSpace($Path)) { [string]$property.Name } else { "{0}.{1}" -f $Path, [string]$property.Name }
            foreach ($item in @(Get-JsonArrayShapeViolations -Baseline $property.Value -Candidate $Candidate -Path $childPath -Segments (@($Segments) + [string]$property.Name))) {
                $violations.Add([string]$item)
            }
        }

        return $violations.ToArray()
    }

    return $violations.ToArray()
}

function Assert-EffectiveSettingsContract {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [object]
        $BaselineSettings,
        [Parameter(Mandatory = $true)]
        [object]
        $Settings,
        [Parameter(Mandatory = $false)]
        [string]$Label = 'effective settings'
    )

    $violations = New-Object System.Collections.Generic.List[string]
    foreach ($item in @(Get-JsonArrayShapeViolations -Baseline $BaselineSettings -Candidate $Settings -Path 'settings')) {
        $violations.Add([string]$item)
    }

    $bearerKeysValue = Get-JsonLikePropertyValue -Value $Settings -Name 'bearerKeys'
    $bearerKeys = @()
    if (Test-JsonArrayLikeValue -Value $bearerKeysValue) {
        $bearerKeys = @($bearerKeysValue)
    }
    if ($bearerKeys.Count -eq 0) {
        $violations.Add('settings.bearerKeys must contain at least one bearer key.')
    }

    $usersValue = Get-JsonLikePropertyValue -Value $Settings -Name 'users'
    $users = @()
    if (Test-JsonArrayLikeValue -Value $usersValue) {
        $users = @($usersValue)
    }
    if ($users.Count -eq 0) {
        $violations.Add('settings.users must contain at least one user.')
    }
    else {
        $adminUsers = @($users | Where-Object {
            $_ -and ($_.PSObject.Properties.Name -contains 'isAdmin') -and [bool]$_.isAdmin
        })
        if ($adminUsers.Count -eq 0) {
            $violations.Add('settings.users must contain an admin user.')
        }
    }

    $effectiveViolations = @($violations | Select-Object -Unique)
    if ($effectiveViolations.Count -gt 0) {
        throw ("{0} failed contract validation:`n- {1}" -f $Label, ($effectiveViolations -join "`n- "))
    }
}

function Get-CurrentPlatformName {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Platform
    )

    if ($Platform.IsWindows) { return 'windows' }
    if ($Platform.IsMacOS) { return 'macos' }
    return 'linux'
}

function Get-ConfiguredServerDefinitions {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Config
    )

    $definitions = [ordered]@{}
    if (-not $Config -or -not $Config.servers) {
        return [pscustomobject]$definitions
    }

    foreach ($prop in $Config.servers.PSObject.Properties) {
        $name = [string]$prop.Name
        $raw = $prop.Value
        if ([string]::IsNullOrWhiteSpace($name) -or -not $raw) { continue }

        $platforms = @()
        if ($raw.PSObject.Properties.Name -contains 'platforms') {
            foreach ($platformName in @($raw.platforms)) {
                $value = [string]$platformName
                if (-not [string]::IsNullOrWhiteSpace($value)) {
                    $platforms += $value.ToLowerInvariant()
                }
            }
        }

        $requiredCommands = @()
        if ($raw.PSObject.Properties.Name -contains 'requiredCommands') {
            foreach ($commandName in @($raw.requiredCommands)) {
                $value = [string]$commandName
                if (-not [string]::IsNullOrWhiteSpace($value)) {
                    $requiredCommands += $value
                }
            }
        }

        $definitions[$name] = [pscustomobject]@{
            Name             = $name
            Kind             = if ($raw.PSObject.Properties.Name -contains 'kind' -and -not [string]::IsNullOrWhiteSpace([string]$raw.kind)) { [string]$raw.kind } else { 'container-stdio' }
            Required         = if ($raw.PSObject.Properties.Name -contains 'required') { [bool]$raw.required } else { $false }
            Platforms        = $platforms
            AutoStart        = if ($raw.PSObject.Properties.Name -contains 'autoStart') { [bool]$raw.autoStart } else { $false }
            RequiredCommands = $requiredCommands
            HealthUrlTemplate = if ($raw.PSObject.Properties.Name -contains 'healthUrl') { [string]$raw.healthUrl } else { '' }
            Installer        = if ($raw.PSObject.Properties.Name -contains 'installer') { Get-ServerInstallerModel -Raw $raw.installer } else { Get-ServerInstallerModel }
            WorkspaceBinding = if ($raw.PSObject.Properties.Name -contains 'workspaceBinding') { Get-WorkspaceBindingModel -Raw $raw.workspaceBinding } else { Get-WorkspaceBindingModel }
        }
    }

    return [pscustomobject]$definitions
}

function Get-ServerDefinitionFromMap {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$ServerDefinitions,
        [Parameter(Mandatory = $true)][string]$Name
    )

    if ($ServerDefinitions -and ($ServerDefinitions.PSObject.Properties.Name -contains $Name)) {
        return $ServerDefinitions.PSObject.Properties[$Name].Value
    }

    return [pscustomobject]@{
        Name              = $Name
        Kind              = 'container-stdio'
        Required          = $false
        Platforms         = @('windows', 'linux', 'macos')
        AutoStart         = $false
        RequiredCommands  = @()
        HealthUrlTemplate = ''
        Installer         = Get-ServerInstallerModel
        WorkspaceBinding  = Get-WorkspaceBindingModel
    }
}

function Test-ServerSupportedOnPlatform {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Definition,
        [Parameter(Mandatory = $true)]$Platform
    )

    $supportedPlatforms = @($Definition.Platforms)
    if ($supportedPlatforms.Count -eq 0) {
        return [pscustomobject]@{
            Supported = $true
            Reason    = ''
        }
    }

    $currentPlatform = Get-CurrentPlatformName -Platform $Platform
    if ($supportedPlatforms -contains $currentPlatform) {
        return [pscustomobject]@{
            Supported = $true
            Reason    = ''
        }
    }

    return [pscustomobject]@{
        Supported = $false
        Reason    = ("supported platforms: {0}" -f ($supportedPlatforms -join ', '))
    }
}

function Test-ServerHasUnresolvedPlaceholders {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Server
    )

    if (-not $Server) {
        return $false
    }

    if ($Server.PSObject.Properties.Name -contains 'args') {
        foreach ($arg in @($Server.args)) {
            if ([string]$arg -match '<PUT_[A-Z0-9_]+_HERE>') {
                return $true
            }
        }
    }

    if ($Server.PSObject.Properties.Name -contains 'url') {
        if ([string]$Server.url -match '<PUT_[A-Z0-9_]+_HERE>') {
            return $true
        }
    }

    return $false
}

function Test-RemoteHttpServerNeedsOAuthApproval {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Definition,
        [Parameter(Mandatory = $false)]
        $Server
    )

    if (-not $Definition -or -not $Server) {
        return $false
    }

    if ([string]$Definition.Kind -ne 'remote-http') {
        return $false
    }

    if (-not ($Server.PSObject.Properties.Name -contains 'oauth')) {
        return $false
    }

    $oauth = $Server.oauth
    if (-not $oauth) {
        return $false
    }

    $hasAccessToken = $oauth.PSObject.Properties.Name -contains 'accessToken' -and -not [string]::IsNullOrWhiteSpace([string]$oauth.accessToken)
    $hasRefreshToken = $oauth.PSObject.Properties.Name -contains 'refreshToken' -and -not [string]::IsNullOrWhiteSpace([string]$oauth.refreshToken)

    return (-not $hasAccessToken -and -not $hasRefreshToken)
}

function Resolve-HostBridgePortFromSettingsServer {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]$Server
    )

    if (-not $Server) {
        return $null
    }

    foreach ($propName in @('args', 'url')) {
        if (-not ($Server.PSObject.Properties.Name -contains $propName)) {
            continue
        }

        $values = if ($propName -eq 'args') { @($Server.args) } else { @([string]$Server.url) }
        foreach ($value in $values) {
            $text = [string]$value
            if ($text -match ':(\d+)/mcp/?$') {
                return [int]$Matches[1]
            }
        }
    }

    return $null
}

function Resolve-HostBridgeUrlFromSettingsServer {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]$Server
    )

    if (-not $Server) {
        return ''
    }

    foreach ($propName in @('args', 'url')) {
        if (-not ($Server.PSObject.Properties.Name -contains $propName)) {
            continue
        }

        $values = if ($propName -eq 'args') { @($Server.args) } else { @([string]$Server.url) }
        foreach ($value in $values) {
            $text = [string]$value
            if ($text -match '^https?://[^/]+:(\d+)/mcp/?$') {
                return ("http://127.0.0.1:{0}/mcp" -f [int]$Matches[1])
            }
            if ($text -match '^https?://.+/mcp/?$') {
                return $text
            }
        }
    }

    return ''
}

function Resolve-HealthUrlTemplate {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Template,
        [Parameter(Mandatory = $true)][int]$AbpPort,
        [Parameter(Mandatory = $true)][int]$HubPort,
        [Parameter(Mandatory = $false)][Nullable[int]]$HostBridgePort
    )

    if ([string]::IsNullOrWhiteSpace($Template)) {
        return ''
    }

    $resolved = $Template.Replace('{abpPort}', "$AbpPort").Replace('{hubPort}', "$HubPort")
    if ($null -ne $HostBridgePort) {
        $resolved = $resolved.Replace('{hostBridgePort}', "$HostBridgePort")
    }

    return $resolved
}

function Parse-McpResponsePayload {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        [AllowEmptyString()]
        [string]$Text
    )

    if ([string]::IsNullOrWhiteSpace($Text)) {
        return $null
    }

    $lastData = ''
    foreach ($line in @($Text -split "`r?`n")) {
        if ($line.StartsWith('data:')) {
            $lastData = $line.Substring(5).Trim()
        }
    }

    foreach ($candidate in @($lastData, $Text)) {
        if ([string]::IsNullOrWhiteSpace($candidate)) { continue }
        try {
            return ($candidate | ConvertFrom-Json -ErrorAction Stop)
        }
        catch {
        }
    }

    return $null
}

function Invoke-McpHttpRequest {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [Parameter(Mandatory = $true)]$Body,
        [Parameter(Mandatory = $false)][AllowEmptyString()][string]$SessionId = '',
        [Parameter(Mandatory = $true)][int]$TimeoutSec
    )

    $headers = @{
        Accept = 'application/json, text/event-stream'
        'Content-Type' = 'application/json'
    }
    if (-not [string]::IsNullOrWhiteSpace($SessionId)) {
        $headers['mcp-session-id'] = $SessionId
    }

    $jsonBody = $Body | ConvertTo-Json -Depth 20 -Compress
    $response = Invoke-WebRequest -Uri $Url -Method Post -Headers $headers -Body $jsonBody -TimeoutSec $TimeoutSec -UseBasicParsing -ErrorAction Stop
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

function Test-HttpEndpointReachable {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)][AllowEmptyString()][string]$Url,
        [Parameter(Mandatory = $true)][int]$TimeoutSec
    )

    if ([string]::IsNullOrWhiteSpace($Url)) {
        return [pscustomobject]@{
            Passed = $true
            Detail = ''
        }
    }

    try {
        $response = Invoke-WebRequest -Uri $Url -Method Get -TimeoutSec $TimeoutSec -UseBasicParsing -ErrorAction Stop
        $statusCode = [int]$response.StatusCode
        if ($statusCode -ge 200 -and $statusCode -lt 500) {
            return [pscustomobject]@{
                Passed = $true
                Detail = ("HTTP {0}" -f $statusCode)
            }
        }

        return [pscustomobject]@{
            Passed = $false
            Detail = ("HTTP {0}" -f $statusCode)
        }
    }
    catch {
        return [pscustomobject]@{
            Passed = $false
            Detail = [string]$_.Exception.Message
        }
    }
}

function Test-HostBridgeMcpEndpoint {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)][AllowEmptyString()][string]$Url,
        [Parameter(Mandatory = $true)][int]$TimeoutSec
    )

    if ([string]::IsNullOrWhiteSpace($Url)) {
        return [pscustomobject]@{
            Passed = $false
            Detail = 'missing host bridge MCP URL.'
        }
    }

    try {
        $initialize = Invoke-McpHttpRequest -Url $Url -TimeoutSec $TimeoutSec -Body @{
            jsonrpc = '2.0'
            id = 0
            method = 'initialize'
            params = @{
                protocolVersion = '2025-06-18'
                capabilities = @{}
                clientInfo = @{
                    name = 'mcpace-launcher-preflight'
                    version = '1.0.0'
                }
            }
        }

        if ($initialize.StatusCode -ne 200) {
            return [pscustomobject]@{
                Passed = $false
                Detail = ("initialize returned HTTP {0}" -f $initialize.StatusCode)
            }
        }

        if (-not $initialize.Payload -or -not $initialize.Payload.result -or -not $initialize.Payload.result.serverInfo) {
            return [pscustomobject]@{
                Passed = $false
                Detail = 'initialize response is missing serverInfo.'
            }
        }

        $sessionId = [string]$initialize.SessionId
        if (-not [string]::IsNullOrWhiteSpace($sessionId)) {
            $notification = Invoke-McpHttpRequest -Url $Url -TimeoutSec $TimeoutSec -SessionId $sessionId -Body @{
                jsonrpc = '2.0'
                method = 'notifications/initialized'
            }

            if ($notification.StatusCode -ne 200 -and $notification.StatusCode -ne 202) {
                return [pscustomobject]@{
                    Passed = $false
                    Detail = ("notifications/initialized returned HTTP {0}" -f $notification.StatusCode)
                }
            }
        }

        $tools = Invoke-McpHttpRequest -Url $Url -TimeoutSec $TimeoutSec -SessionId $sessionId -Body @{
            jsonrpc = '2.0'
            id = 1
            method = 'tools/list'
            params = @{}
        }

        if ($tools.StatusCode -ne 200) {
            return [pscustomobject]@{
                Passed = $false
                Detail = ("tools/list returned HTTP {0}" -f $tools.StatusCode)
            }
        }

        $toolsValue = $null
        if ($tools.Payload -and $tools.Payload.result) {
            $toolsValue = $tools.Payload.result.tools
        }
        if ($null -ne $toolsValue -and -not (Test-JsonArrayLikeValue -Value $toolsValue)) {
            return [pscustomobject]@{
                Passed = $false
                Detail = 'tools/list response is missing a tools array.'
            }
        }

        $serverInfo = $initialize.Payload.result.serverInfo
        $serverName = if ($serverInfo.name) { [string]$serverInfo.name } else { 'unknown-server' }
        $serverVersion = if ($serverInfo.version) { [string]$serverInfo.version } else { 'unknown-version' }
        $toolsCount = if ($null -eq $toolsValue) { 0 } else { @($toolsValue).Count }
        $sessionMode = if ([string]::IsNullOrWhiteSpace($sessionId)) { 'stateless' } else { 'present' }

        return [pscustomobject]@{
            Passed = $true
            Detail = ("initialize+tools/list ok; server={0} {1}; sessionId={2}; tools={3}" -f $serverName, $serverVersion, $sessionMode, $toolsCount)
        }
    }
    catch {
        return [pscustomobject]@{
            Passed = $false
            Detail = [string]$_.Exception.Message
        }
    }
}

function Test-RequiredCommandsAvailable {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)][AllowEmptyCollection()][string[]]$RequiredCommands = @()
    )

    $missing = @()
    foreach ($commandName in @($RequiredCommands)) {
        if ([string]::IsNullOrWhiteSpace([string]$commandName)) { continue }
        if (-not (Get-Command $commandName -ErrorAction SilentlyContinue)) {
            $missing += [string]$commandName
        }
    }

    return [pscustomobject]@{
        Passed          = ($missing.Count -eq 0)
        MissingCommands = $missing
    }
}

function Get-HostBridgePreflight {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Definition,
        [Parameter(Mandatory = $false)]$SettingsServer,
        [Parameter(Mandatory = $true)]$Platform,
        [Parameter(Mandatory = $true)][string]$RootPath,
        [Parameter(Mandatory = $false)][string]$PowerShellCommand,
        [Parameter(Mandatory = $false)][AllowEmptyString()][string]$HealthUrl = '',
        [Parameter(Mandatory = $true)][int]$ProbeTimeoutSec
    )

    $reasons = @()
    $hostBridgeUrl = ''
    $healthDetail = ''
    $endpointDetail = ''
    $healthPassed = $true
    $endpointPassed = $true
    $commandsCheck = Test-RequiredCommandsAvailable -RequiredCommands @($Definition.RequiredCommands)
    foreach ($missingCommand in @($commandsCheck.MissingCommands)) {
        $reasons += ("missing command: {0}" -f $missingCommand)
    }

    switch ([string]$Definition.Name) {
        'windows-mcp' {
            if (-not $Platform.IsWindows) {
                $reasons += 'requires Windows UI automation host.'
            }

            $scriptPath = Join-Path $RootPath 'windows-mcp-host.ps1'
            if (-not (Test-Path -LiteralPath $scriptPath)) {
                $reasons += 'missing windows-mcp-host.ps1 launcher script.'
            }

            if ([string]::IsNullOrWhiteSpace([string]$PowerShellCommand)) {
                $reasons += 'no PowerShell executable available to start the Windows host bridge.'
            }
        }
        'browser' {
            if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
                $reasons += 'missing command: node'
            }
        }
    }

    if ($reasons.Count -eq 0) {
        $hostBridgeUrl = Resolve-HostBridgeUrlFromSettingsServer -Server $SettingsServer

        if (-not [string]::IsNullOrWhiteSpace($HealthUrl) -and [string]$HealthUrl -ne [string]$hostBridgeUrl) {
            $healthProbe = Test-HttpEndpointReachable -Url $HealthUrl -TimeoutSec $ProbeTimeoutSec
            $healthPassed = [bool]$healthProbe.Passed
            $healthDetail = [string]$healthProbe.Detail
            if (-not $healthPassed) {
                $reasons += ("health probe failed: {0}" -f $healthDetail)
            }
        }

        $endpointProbe = Test-HostBridgeMcpEndpoint -Url $hostBridgeUrl -TimeoutSec $ProbeTimeoutSec
        $endpointPassed = [bool]$endpointProbe.Passed
        $endpointDetail = [string]$endpointProbe.Detail
        if (-not $endpointPassed) {
            $reasons += ("MCP endpoint probe failed: {0}" -f $endpointDetail)
        }
    }

    return [pscustomobject]@{
        Passed          = ($reasons.Count -eq 0)
        MissingCommands = @($commandsCheck.MissingCommands)
        Reasons         = $reasons
        HealthPassed    = $healthPassed
        HealthDetail    = $healthDetail
        EndpointPassed  = $endpointPassed
        EndpointDetail  = $endpointDetail
        HostBridgeUrl   = $hostBridgeUrl
    }
}

function ConvertTo-PosixShellLiteral {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyString()]
        [string]$Value
    )

    $escaped = $Value.Replace("'", "'""'""'")
    return ("'{0}'" -f $escaped)
}

function Get-DockerContainerByName {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$ContainerName
    )

    try {
        $raw = docker inspect $ContainerName 2>$null
        if ($LASTEXITCODE -ne 0 -or -not $raw) {
            return $null
        }

        $parsed = ($raw -join "`n") | ConvertFrom-Json
        if ($parsed -is [System.Array]) {
            return $parsed[0]
        }
        return $parsed
    }
    catch {
        return $null
    }
}

function Invoke-HubContainerShellCommand {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$ContainerName,
        [Parameter(Mandatory = $true)]
        [string]$CommandText
    )

    $output = & docker exec $ContainerName sh -lc $CommandText 2>&1
    $exitCode = $LASTEXITCODE
    $text = ($output | Out-String).Trim()
    return [pscustomobject]@{
        ExitCode = $exitCode
        Output   = $text
    }
}

function Test-HostBinaryPresent {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$BinaryName
    )

    $cmd = Get-Command $BinaryName -ErrorAction SilentlyContinue
    return [pscustomobject]@{
        ProbeAttempted = $true
        Present        = ($null -ne $cmd)
        Detail         = if ($cmd) { [string]$cmd.Source } else { ("missing command: {0}" -f $BinaryName) }
    }
}

function Test-HubContainerBinaryPresent {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$ContainerName,
        [Parameter(Mandatory = $true)]
        [string]$BinaryName
    )

    $container = Get-DockerContainerByName -ContainerName $ContainerName
    if (-not $container) {
        return [pscustomobject]@{
            ProbeAttempted = $false
            Present        = $false
            Detail         = 'container not created'
        }
    }

    if (-not [bool]$container.State.Running) {
        return [pscustomobject]@{
            ProbeAttempted = $false
            Present        = $false
            Detail         = 'container not running'
        }
    }

    $commandText = "command -v {0} >/dev/null 2>&1" -f (ConvertTo-PosixShellLiteral -Value $BinaryName)
    $result = Invoke-HubContainerShellCommand -ContainerName $ContainerName -CommandText $commandText
    return [pscustomobject]@{
        ProbeAttempted = $true
        Present        = ($result.ExitCode -eq 0)
        Detail         = if ($result.ExitCode -eq 0) { ("binary present in container: {0}" -f $BinaryName) } elseif (-not [string]::IsNullOrWhiteSpace($result.Output)) { $result.Output } else { ("binary missing in container: {0}" -f $BinaryName) }
    }
}

function Get-InstallerBinaryProbe {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Installer,
        [Parameter(Mandatory = $true)][string]$HubContainerName
    )

    if (-not $Installer -or [string]::IsNullOrWhiteSpace([string]$Installer.BinaryName)) {
        return [pscustomobject]@{
            ProbeAttempted = $false
            Present        = $false
            Detail         = 'installer binaryName is not configured'
        }
    }

    switch ([string]$Installer.InstallTarget) {
        'host' {
            return (Test-HostBinaryPresent -BinaryName ([string]$Installer.BinaryName))
        }
        'hub-container' {
            return (Test-HubContainerBinaryPresent -ContainerName $HubContainerName -BinaryName ([string]$Installer.BinaryName))
        }
        default {
            return [pscustomobject]@{
                ProbeAttempted = $false
                Present        = $false
                Detail         = 'installer target does not support binary probing'
            }
        }
    }
}

function Ensure-OptionalServerDataPaths {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $gitEntry = Get-ServerRuntimeEntry -Context $Context -Name 'git'
    if ($gitEntry -and [bool]$gitEntry.ConfiguredEnabled) {
        $gitRepoRoot = Join-Path $Context.DataDir 'git-repo'
        New-Item -ItemType Directory -Force -Path $gitRepoRoot | Out-Null

        $gitDir = Join-Path $gitRepoRoot '.git'
        if (-not (Test-Path -LiteralPath $gitDir -PathType Container)) {
            $gitCommand = Get-Command git -ErrorAction SilentlyContinue
            if (-not $gitCommand) {
                throw 'git server is enabled but git CLI is not available to initialize the manager-owned repository.'
            }

            $output = & git -C $gitRepoRoot init 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw ("Failed to initialize manager-owned git repository: {0}" -f (($output | Out-String).Trim()))
            }
        }
    }
}

function Build-ServerRuntimeEntries {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Config,
        [Parameter(Mandatory = $true)]$Settings,
        [Parameter(Mandatory = $false)]$SourceSettings,
        [Parameter(Mandatory = $false)]$InstallState,
        [Parameter(Mandatory = $true)]$Platform,
        [Parameter(Mandatory = $true)][string]$RootPath,
        [Parameter(Mandatory = $false)][string]$PowerShellCommand,
        [Parameter(Mandatory = $true)][int]$AbpPort,
        [Parameter(Mandatory = $true)][int]$HubPort,
        [Parameter(Mandatory = $true)][int]$ProbeTimeoutSec
    )

    $entries = @()
    $serverDefinitions = Get-ConfiguredServerDefinitions -Config $Config
    if (-not $Settings.mcpServers) {
        return [pscustomobject]@{
            Definitions = $serverDefinitions
            Entries     = @()
        }
    }

    foreach ($prop in $Settings.mcpServers.PSObject.Properties) {
        $name = [string]$prop.Name
        $server = $prop.Value
        if (-not $server) { continue }

        $definition = Get-ServerDefinitionFromMap -ServerDefinitions $serverDefinitions -Name $name
        $installer = $definition.Installer
        $configuredEnabled = $false
        if ($server.PSObject.Properties.Name -contains 'enabled') {
            $configuredEnabled = [bool]$server.enabled
        }

        $sourceEnabled = $configuredEnabled
        $sourceServer = Get-SettingsServerByName -Settings $SourceSettings -Name $name
        if ($sourceServer -and ($sourceServer.PSObject.Properties.Name -contains 'enabled')) {
            $sourceEnabled = [bool]$sourceServer.enabled
        }
        $enabledSource = if ($configuredEnabled -ne $sourceEnabled) { 'local-override' } else { 'source' }

        $supported = Test-ServerSupportedOnPlatform -Definition $definition -Platform $Platform
        $hasPlaceholder = Test-ServerHasUnresolvedPlaceholders -Server $server
        $needsOAuthApproval = Test-RemoteHttpServerNeedsOAuthApproval -Definition $definition -Server $server
        $hostBridgePort = Resolve-HostBridgePortFromSettingsServer -Server $server
        $healthUrl = Resolve-HealthUrlTemplate -Template ([string]$definition.HealthUrlTemplate) -AbpPort $AbpPort -HubPort $HubPort -HostBridgePort $hostBridgePort

        $installRecipeDefined = Test-InstallerRecipeDefined -Installer $installer
        $managedAutoEnable = $false
        $preflight = [pscustomobject]@{
            Passed          = $true
            MissingCommands = @()
            Reasons         = @()
            HealthPassed    = $true
            HealthDetail    = ''
            EndpointPassed  = $true
            EndpointDetail  = ''
            HostBridgeUrl   = ''
        }
        $installRecord = Get-ServerInstallRecordFromMap -InstallState $InstallState -Name $name
        $binaryProbe = Get-InstallerBinaryProbe -Installer $installer -HubContainerName ([string]$Config.hub.containerName)
        $binaryPresent = [bool]$binaryProbe.Present
        $installStatus = ''
        $installError = [string]$installRecord.installError

        if (-not $configuredEnabled) {
            $installStatus = 'disabled'
            $installError = ''
        }
        elseif ($installRecipeDefined) {
            if ($binaryPresent) {
                $installStatus = 'ready'
                $installError = ''
            }
            elseif ([string]$installRecord.installStatus -eq 'failed') {
                $installStatus = 'failed'
            }
            else {
                $installStatus = 'missing'
            }
        }
        elseif ($configuredEnabled -or [bool]$definition.Required) {
            $installStatus = 'not-managed'
            $installError = ''
        }
        else {
            $installStatus = 'missing-install-recipe'
        }

        if ([string]$definition.Kind -eq 'host-bridge') {
            $preflight = Get-HostBridgePreflight `
                -Definition $definition `
                -SettingsServer $server `
                -Platform $Platform `
                -RootPath $RootPath `
                -PowerShellCommand $PowerShellCommand `
                -HealthUrl $healthUrl `
                -ProbeTimeoutSec $ProbeTimeoutSec
        }

        $disabledReasonCategory = ''
        $disabledReason = ''
        $effectiveEnabled = $configuredEnabled

        if (-not $configuredEnabled) {
            $disabledReasonCategory = 'configured-disabled'
            $disabledReason = if ($enabledSource -eq 'local-override') { 'disabled by local override' } else { 'disabled in source settings' }
            $effectiveEnabled = $false
        }
        elseif (-not $supported.Supported) {
            $disabledReasonCategory = 'platform'
            $disabledReason = [string]$supported.Reason
            $effectiveEnabled = $false
        }
        elseif ($hasPlaceholder) {
            $disabledReasonCategory = 'placeholder'
            $disabledReason = 'required placeholder value is missing'
            $effectiveEnabled = $false
        }
        elseif ($needsOAuthApproval) {
            $disabledReasonCategory = 'oauth'
            $disabledReason = 'oauth approval or tokens are required before the server can be enabled'
            $effectiveEnabled = $false
        }
        elseif ([string]$definition.Kind -eq 'host-bridge' -and -not $preflight.Passed) {
            $disabledReasonCategory = 'preflight'
            $disabledReason = ($preflight.Reasons -join '; ')
            $effectiveEnabled = $false
        }
        elseif ($installRecipeDefined -and -not $binaryPresent) {
            $disabledReasonCategory = 'install'
            if ($installStatus -eq 'failed' -and -not [string]::IsNullOrWhiteSpace($installError)) {
                $disabledReason = ("auto-install failed: {0}" -f $installError)
            }
            else {
                $disabledReason = ("binary missing for auto-install target {0}" -f [string]$installer.InstallTarget)
            }
            $effectiveEnabled = $false
        }
        $server.enabled = $effectiveEnabled
        $server | Add-Member -NotePropertyName '_manager' -NotePropertyValue ([pscustomobject]@{
            kind             = [string]$definition.Kind
            required         = [bool]$definition.Required
            platforms        = @($definition.Platforms)
            autoStart        = [bool]$definition.AutoStart
            requiredCommands = @($definition.RequiredCommands)
            healthUrl        = $healthUrl
            hostBridgeUrl    = [string]$preflight.HostBridgeUrl
            installer        = [pscustomobject]@{
                autoInstall     = [bool]$installer.AutoInstall
                installTarget   = [string]$installer.InstallTarget
                installMethod   = [string]$installer.InstallMethod
                installPackage  = [string]$installer.InstallPackage
                binaryName      = [string]$installer.BinaryName
                verifyCommand   = [string]$installer.VerifyCommand
                postInstallMode = [string]$installer.PostInstallMode
                recipeDefined   = $installRecipeDefined
            }
            enabledInSource  = $sourceEnabled
            configuredEnabled = $configuredEnabled
            enabledSource   = $enabledSource
            effectiveEnabled = $effectiveEnabled
            disabledReason   = $disabledReason
            disabledCategory = $disabledReasonCategory
            preflightPassed  = [bool]$preflight.Passed
            preflightSummary = if ($preflight.Passed) { [string]$preflight.EndpointDetail } else { ($preflight.Reasons -join '; ') }
            installStatus    = $installStatus
            installError     = $installError
            binaryPresent    = $binaryPresent
            binaryProbeDetail = [string]$binaryProbe.Detail
        }) -Force

        $entries += [pscustomobject]@{
            Name                 = $name
            Kind                 = [string]$definition.Kind
            Required             = [bool]$definition.Required
            Platforms            = @($definition.Platforms)
            AutoStart            = [bool]$definition.AutoStart
            RequiredCommands     = @($definition.RequiredCommands)
            HealthUrl            = $healthUrl
            HostBridgeUrl        = [string]$preflight.HostBridgeUrl
            SourceEnabled        = $sourceEnabled
            ConfiguredEnabled    = $configuredEnabled
            EnabledSource        = $enabledSource
            EffectiveEnabled     = $effectiveEnabled
            ManagedAutoEnable    = $managedAutoEnable
            Installer            = $installer
            InstallAuto          = [bool]$installer.AutoInstall
            InstallTarget        = [string]$installer.InstallTarget
            InstallMethod        = [string]$installer.InstallMethod
            InstallPackage       = [string]$installer.InstallPackage
            InstallBinaryName    = [string]$installer.BinaryName
            InstallVerifyCommand = [string]$installer.VerifyCommand
            InstallPostMode      = [string]$installer.PostInstallMode
            HasInstallRecipe     = $installRecipeDefined
            InstallStatus        = $installStatus
            InstallError         = $installError
            BinaryPresent        = $binaryPresent
            BinaryProbeDetail    = [string]$binaryProbe.Detail
            PlatformSupported    = [bool]$supported.Supported
            PlaceholderDetected  = $hasPlaceholder
            DisabledReason       = $disabledReason
            DisabledReasonCategory = $disabledReasonCategory
            PreflightPassed      = [bool]$preflight.Passed
            MissingCommands      = @($preflight.MissingCommands)
            PreflightReasons     = @($preflight.Reasons)
            PreflightSummary     = if ($preflight.Passed) { [string]$preflight.EndpointDetail } else { ($preflight.Reasons -join '; ') }
            HostBridgePort       = $hostBridgePort
        }
    }

    return [pscustomobject]@{
        Definitions = $serverDefinitions
        Entries     = $entries
    }
}

function New-McpAceContext {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$RootPath,
        [Parameter(Mandatory = $false)]
        [AllowEmptyString()]
        [string]$StateRoot = ''
    )

    $resolvedStateRoot = Resolve-ManagerStateRootPath -RootPath $RootPath -StateRoot $StateRoot
    $configPath = Join-Path $RootPath 'mcpace.config.json'
    $settingsPath = Join-Path $RootPath 'mcp_settings.json'
    $managerSettingsPath = Join-Path $RootPath 'manager.settings.json'
    $dataDir = Join-Path $resolvedStateRoot 'data'
    $logsDir = Join-Path $resolvedStateRoot 'logs'
    $runtimeDir = Join-Path $dataDir 'runtime'
    $serverStateDir = Join-Path $dataDir 'server-state'
    $localOverridesPath = Join-Path $runtimeDir 'mcp_settings.local-overrides.json'
    $settingsEffectivePath = Join-Path $runtimeDir 'mcp_settings.effective.json'
    $installStatePath = Join-Path $runtimeDir 'server-install-state.json'
    $hubStatePath = Join-Path $runtimeDir 'hub-runtime-state.json'
    $authStatePath = Join-Path $serverStateDir 'auth-state.json'

    New-Item -ItemType Directory -Force -Path $dataDir, $logsDir, $runtimeDir, $serverStateDir | Out-Null

    $config = Read-JsonFile -Path $configPath
    $settingsRaw = Read-JsonFile -Path $settingsPath
    Assert-SourceSettingsPolicy -Config $config -SettingsRaw $settingsRaw -SettingsPath $settingsPath
    $workspaceRegistry = Get-WorkspaceRegistry -Config $config -ManagerRoot $RootPath
    $settingsExpanded = Expand-EnvPlaceholdersInValue -Value $settingsRaw -Variables $workspaceRegistry.PlaceholderVariables
    $localServerOverrides = Read-LocalServerOverrides -Path $localOverridesPath
    $previousEffectiveSettings = $null
    if (Test-Path -LiteralPath $settingsEffectivePath -PathType Leaf) {
        try {
            $previousEffectiveSettings = Read-JsonFile -Path $settingsEffectivePath
        }
        catch {
            $previousEffectiveSettings = $null
        }
    }
    $mergedLocalOverrides = Merge-LocalServerOverridesFromEffectiveSettings `
        -BaselineSettings $settingsExpanded `
        -EffectiveSettings $previousEffectiveSettings `
        -ExistingOverrides $localServerOverrides
    if (-not (Test-JsonLikeEqual -Left $localServerOverrides -Right $mergedLocalOverrides)) {
        Write-LocalServerOverrides -Path $localOverridesPath -Overrides $mergedLocalOverrides | Out-Null
    }
    $localServerOverrides = $mergedLocalOverrides
    $settings = Apply-LocalServerOverridesToSettings -Settings (Copy-JsonLikeValue -Value $settingsExpanded) -Overrides $localServerOverrides
    $authMaterial = Resolve-LocalAuthMaterial -Path $authStatePath -ServerStateDir $serverStateDir
    $settings = Apply-ResolvedAuthMaterialToSettings -Settings $settings -AuthMaterial $authMaterial -ClientKeyName ([string]$config.client.keyName)
    Assert-ResolvedSettingsSecrets -Settings $settings
    $serverDefinitions = Get-ConfiguredServerDefinitions -Config $config
    $settings = Apply-WorkspaceAwareServerTransforms -Settings $settings -ServerDefinitions $serverDefinitions -WorkspaceRegistry $workspaceRegistry
    Assert-EffectiveSettingsContract -BaselineSettings $settingsExpanded -Settings $settings -Label 'In-memory effective settings'
    $installState = Read-ServerInstallState -Path $installStatePath
    $managerSettings = Read-ManagerSettings -Path $managerSettingsPath
    $platform = Get-PlatformInfo
    $powershellCommand = Get-PreferredPowerShellCommand
    $serverRuntime = Build-ServerRuntimeEntries `
        -Config $config `
        -Settings $settings `
        -InstallState $installState `
        -Platform $platform `
        -RootPath $RootPath `
        -PowerShellCommand $powershellCommand `
        -AbpPort ([int]$config.ports.abp) `
        -HubPort ([int]$config.ports.hub) `
        -ProbeTimeoutSec ([int]$config.health.probeTimeoutSec) `
        -SourceSettings $settingsExpanded
    $platformDisabledServers = @($serverRuntime.Entries | Where-Object { $_.DisabledReasonCategory -eq 'platform' } | ForEach-Object { $_.Name })
    $placeholderDisabledServers = @($serverRuntime.Entries | Where-Object { $_.DisabledReasonCategory -eq 'placeholder' } | ForEach-Object { $_.Name })
    $missingCommandDisabledServers = @($serverRuntime.Entries | Where-Object {
        $_.DisabledReasonCategory -eq 'preflight' -and @($_.MissingCommands).Count -gt 0
    } | ForEach-Object { $_.Name })
    $preflightDisabledServers = @($serverRuntime.Entries | Where-Object { $_.DisabledReasonCategory -eq 'preflight' } | ForEach-Object { $_.Name })
    Write-JsonFile -Path $settingsEffectivePath -Value $settings
    $writtenSettings = Read-JsonFile -Path $settingsEffectivePath
    Assert-EffectiveSettingsContract -BaselineSettings $settingsExpanded -Settings $writtenSettings -Label 'Generated effective settings file'

    $allKeys = @($settings.bearerKeys)
    if ($allKeys.Count -eq 0) {
        throw 'mcp_settings.json has no bearerKeys entries.'
    }

    $selectedKey = $null
    foreach ($key in $allKeys) {
        if ([string]$key.name -eq [string]$config.client.keyName) {
            $selectedKey = $key
            break
        }
    }
    if (-not $selectedKey) {
        $selectedKey = $allKeys[0]
    }
    if ([string]::IsNullOrWhiteSpace([string]$selectedKey.token)) {
        throw ("MCPACE_BEARER_TOKEN is required for bearer key '{0}'." -f [string]$selectedKey.name)
    }

    $npxCommand = Get-PreferredNpxCommand
    $dockerHostAlias = 'host.docker.internal'
    if ($config.platform -and -not [string]::IsNullOrWhiteSpace([string]$config.platform.dockerHostAlias)) {
        $dockerHostAlias = [string]$config.platform.dockerHostAlias
    }

    $sessionGateEnabled = $false
    $streamProbeMs = 750
    $idleDelayMs = 1500
    $logTailLines = 400
    if ($config.compatibility -and $config.compatibility.sessionGate) {
        if ($null -ne $config.compatibility.sessionGate.enabled) {
            $sessionGateEnabled = [bool]$config.compatibility.sessionGate.enabled
        }
        if ($null -ne $config.compatibility.sessionGate.streamProbeMs) {
            $streamProbeMs = [int]$config.compatibility.sessionGate.streamProbeMs
        }
        if ($null -ne $config.compatibility.sessionGate.idleDelayMs) {
            $idleDelayMs = [int]$config.compatibility.sessionGate.idleDelayMs
        }
        if ($null -ne $config.compatibility.sessionGate.logTailLines) {
            $logTailLines = [int]$config.compatibility.sessionGate.logTailLines
        }
    }

    $requiredServerNames = @($serverRuntime.Entries | Where-Object { $_.EffectiveEnabled -and $_.Required } | ForEach-Object { $_.Name })
    $resolvedBackupDir = [string]$managerSettings.maintenance.backupDir
    if (-not [System.IO.Path]::IsPathRooted($resolvedBackupDir)) {
        $resolvedBackupDir = Join-Path $resolvedStateRoot $resolvedBackupDir
    }

    return [pscustomobject]@{
        RootPath          = $RootPath
        StateRoot         = $resolvedStateRoot
        ConfigPath        = $configPath
        SettingsPath      = $settingsPath
        SettingsEffectivePath = $settingsEffectivePath
        LocalOverridesPath = $localOverridesPath
        ManagerSettingsPath = $managerSettingsPath
        InstallStatePath  = $installStatePath
        HubStatePath      = $hubStatePath
        AuthStatePath     = $authStatePath
        ManagerRoot       = $RootPath
        DataDir           = $dataDir
        LogsDir           = $logsDir
        RuntimeDir        = $runtimeDir
        ServerStateDir    = $serverStateDir
        AbpPidPath        = (Join-Path $runtimeDir 'abp.pid')
        BackupDir         = $resolvedBackupDir
        LogRetentionDays  = [int]$managerSettings.maintenance.logRetentionDays
        BackupRetentionCount = [int]$managerSettings.maintenance.backupRetentionCount
        AutostartTaskName = [string]$managerSettings.autostart.taskName
        AutostartEnabled  = [bool]$managerSettings.autostart.enabled
        SmokeTimeoutSec   = [int]$managerSettings.smokeTest.timeoutSec
        ManagerSettings   = $managerSettings
        Config            = $config
        Settings          = $settings
        SettingsRaw       = $settingsRaw
        SourceExpandedSettings = $settingsExpanded
        LocalServerOverrides = $localServerOverrides
        WorkspaceRegistry = $workspaceRegistry
        ServerInstallState = $installState
        ServerDefinitions = $serverRuntime.Definitions
        ServerRuntime     = $serverRuntime.Entries
        RequiredServerNames = $requiredServerNames
        PlatformDisabledServers = $platformDisabledServers
        PlaceholderDisabledServers = $placeholderDisabledServers
        MissingCommandDisabledServers = $missingCommandDisabledServers
        PreflightDisabledServers = $preflightDisabledServers
        IsWindows         = [bool]$platform.IsWindows
        IsLinux           = [bool]$platform.IsLinux
        IsMacOS           = [bool]$platform.IsMacOS
        DockerHostAlias   = $dockerHostAlias
        NpxCommand        = $npxCommand
        PowerShellCommand = $powershellCommand
        AbpPort           = [int]$config.ports.abp
        HubPort           = [int]$config.ports.hub
        HubContainerName  = [string]$config.hub.containerName
        HubImage          = [string]$config.hub.image
        AbpPackage        = [string]$config.abp.package
        AbpPackageArgs    = @($config.abp.packageArgs)
        McpRemotePackage  = [string]$config.packages.mcpRemote
        ProbeTimeoutSec   = [int]$config.health.probeTimeoutSec
        StartupTimeoutSec = [int]$config.health.startupTimeoutSec
        OfflineThreshold  = [int]$config.health.offlineThreshold
        SessionGateEnabled = $sessionGateEnabled
        SessionGateStreamProbeMs = $streamProbeMs
        SessionGateIdleDelayMs = $idleDelayMs
        SessionGateLogTailLines = $logTailLines
        KeyName           = [string]$selectedKey.name
        BearerToken       = [string]$selectedKey.token
        BearerTokenSource = [string]$authMaterial.BearerTokenSource
        AdminUsername     = [string]$authMaterial.AdminUsername
        AdminPasswordSource = [string]$authMaterial.AdminPasswordSource
        AdminPasswordKnown = [bool]$authMaterial.AdminPasswordKnown
    }
}

function Get-ServerRuntimeEntry {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $true)][string]$Name
    )

    foreach ($entry in @($Context.ServerRuntime)) {
        if ([string]$entry.Name -eq $Name) {
            return $entry
        }
    }

    return $null
}

function Get-HostBridgeRuntimeEntries {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    return @($Context.ServerRuntime | Where-Object { [string]$_.Kind -eq 'host-bridge' })
}

function Write-LauncherLog {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $true)][string]$Message
    )

    $logPath = Join-Path $Context.LogsDir 'launcher.log'
    $stamp = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
    Add-Content -LiteralPath $logPath -Value ("[{0}] {1}" -f $stamp, $Message)
}

function Get-DockerPortPublishers {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][int]$HostPort
    )

    $dockerCmd = Get-Command docker -ErrorAction SilentlyContinue
    if (-not $dockerCmd) {
        return @()
    }

    try {
        $rows = & docker ps --format '{{.Names}}|{{.Ports}}' 2>$null
        if ($LASTEXITCODE -ne 0 -or -not $rows) {
            return @()
        }

        $hits = @()
        foreach ($row in @($rows)) {
            $parts = [string]$row -split '\|', 2
            if ($parts.Count -lt 2) { continue }
            $name = [string]$parts[0]
            $ports = [string]$parts[1]
            if ($ports -match (':{0}->' -f $HostPort)) {
                $hits += [pscustomobject]@{
                    Name  = $name
                    Ports = $ports
                }
            }
        }

        return $hits
    }
    catch {
        return @()
    }
}

function Test-DockerReady {
    [CmdletBinding()]
    param()

    try {
        docker version --format '{{.Server.Version}}' 2>$null | Out-Null
        return ($LASTEXITCODE -eq 0)
    }
    catch {
        return $false
    }
}

function Get-NodeMajorVersion {
    [CmdletBinding()]
    param()

    try {
        $raw = (& node --version 2>$null).Trim()
        if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($raw)) {
            return $null
        }
        $trimmed = $raw.TrimStart('v')
        return [int](($trimmed.Split('.'))[0])
    }
    catch {
        return $null
    }
}

function Assert-Prerequisites {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    foreach ($name in @('docker', 'node')) {
        if (-not (Get-Command $name -ErrorAction SilentlyContinue)) {
            throw "Required command is missing: $name"
        }
    }

    if ([string]::IsNullOrWhiteSpace([string]$Context.NpxCommand)) {
        throw 'Required command is missing: npx'
    }

    $nodeMajor = Get-NodeMajorVersion
    if (-not $nodeMajor -or $nodeMajor -lt 18) {
        $current = (& node --version 2>$null)
        throw "Node.js 18+ is required. Current version: $current"
    }

    if (-not (Test-DockerReady)) {
        throw 'Docker daemon is not ready. Start Docker Desktop or Docker Engine and wait until docker commands succeed.'
    }

    if ([string]::IsNullOrWhiteSpace([string]$Context.PowerShellCommand)) {
        throw 'PowerShell 7 (pwsh) is required for MCPace runtime scripts and validation.'
    }
}

function Get-DateTimeOffsetSafe {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Value
    )

    if ($null -eq $Value) {
        return $null
    }

    try {
        if ($Value -is [DateTimeOffset]) {
            return $Value
        }

        if ($Value -is [datetime]) {
            return [DateTimeOffset]$Value
        }

        $text = [string]$Value
        if ([string]::IsNullOrWhiteSpace($text)) {
            return $null
        }

        return [DateTimeOffset]::Parse($text, [System.Globalization.CultureInfo]::InvariantCulture)
    }
    catch {
        return $null
    }
}

function Get-RelativeAgeLabel {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $StartedAt,
        [Parameter(Mandatory = $false)]
        $ReferenceTime = (Get-Date)
    )

    $start = Get-DateTimeOffsetSafe -Value $StartedAt
    $reference = Get-DateTimeOffsetSafe -Value $ReferenceTime
    if ($null -eq $start -or $null -eq $reference) {
        return ''
    }

    $age = $reference - $start
    if ($age.TotalSeconds -lt 0) {
        return ''
    }

    if ($age.TotalSeconds -lt 90) {
        return ("up {0}s" -f [int][Math]::Floor($age.TotalSeconds))
    }

    if ($age.TotalMinutes -lt 90) {
        return ("up {0}m" -f [int][Math]::Floor($age.TotalMinutes))
    }

    if ($age.TotalHours -lt 36) {
        return ("up {0}h {1}m" -f [int][Math]::Floor($age.TotalHours), $age.Minutes)
    }

    return ("up {0}d {1}h" -f [int][Math]::Floor($age.TotalDays), $age.Hours)
}

function Get-DockerContainerStartedAt {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Container
    )

    if (-not $Container) {
        return $null
    }

    foreach ($candidate in @(
        $Container.State.StartedAt,
        $Container.Created
    )) {
        $parsed = Get-DateTimeOffsetSafe -Value $candidate
        if ($null -ne $parsed) {
            return $parsed
        }
    }

    return $null
}

function Get-ServerStatusSummary {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)][AllowEmptyCollection()][array]$ServerStatuses = @()
    )

    $total = $ServerStatuses.Count
    $online = @($ServerStatuses | Where-Object { $_.status -eq 'connected' }).Count
    $disabled = @($ServerStatuses | Where-Object {
        ($_.status -eq 'disabled') -or ($null -ne $_.enabled -and $_.enabled -eq $false)
    }).Count
    $offline = @($ServerStatuses | Where-Object {
        $_.status -eq 'disconnected' -and -not (($null -ne $_.enabled) -and $_.enabled -eq $false)
    }).Count
    $connecting = @($ServerStatuses | Where-Object { $_.status -eq 'connecting' }).Count
    $other = $total - $online - $offline - $connecting - $disabled

    return [pscustomobject]@{
        Total      = $total
        Online     = $online
        Offline    = $offline
        Connecting = $connecting
        Disabled   = $disabled
        Other      = $other
    }
}

function Get-ServerFixHint {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Server
    )

    $status = [string]$Server.status
    $name = [string]$Server.name
    $errorText = [string]$Server.error

    switch -Regex ($name) {
        'github' {
            if ($status -eq 'disconnected' -and $errorText -match '403') {
                return 'GitHub MCP is optional and disabled by default. If you enable it, a 403 usually means the token is missing MCP/Copilot access or is invalid.'
            }
            break
        }
        'git' {
            if ($status -ne 'connected') {
                return 'git MCP is optional and disabled by default. If you enable it, it needs --repository pointing to a real folder with .git.'
            }
            break
        }
        'sentry' {
            if ($status -eq 'connecting' -or $status -eq 'oauth_required') {
                return 'Sentry needs OAuth approval. Open http://127.0.0.1:12223, open Sentry card, click Approve. Do not open /oauth/callback directly.'
            }
            break
        }
        'lean-ctx' {
            if ($status -ne 'connected') {
                return 'lean-ctx in this launcher runs inside the MCPace container. Boot/start should auto-install lean-ctx-bin there; if it stays unavailable, inspect installTarget/installStatus/installError below instead of installing it into Windows PATH.'
            }
            break
        }
        default {
            if ($status -eq 'disconnected' -and $errorText) {
                return ("Error: {0}" -f $errorText)
            }
        }
    }

    return $null
}

function Get-ServerIssueEntries {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]$Context,
        [Parameter(Mandatory = $false)][AllowEmptyCollection()][array]$ServerStatuses = @()
    )

    $issues = @()
    foreach ($item in @($ServerStatuses | Where-Object {
        $_.status -ne 'connected' -and -not (($null -ne $_.enabled) -and $_.enabled -eq $false)
    })) {
        $entry = if ($Context) { Get-ServerRuntimeEntry -Context $Context -Name ([string]$item.name) } else { $null }
        $compatibilityMessage = $null
        if (
            $entry -and
            [string]$entry.Kind -eq 'host-bridge' -and
            [bool]$entry.PreflightPassed -and
            [string]$item.status -eq 'disconnected' -and
            [string]$item.error -match 'Connection closed'
        ) {
            $compatibilityMessage = ("{0}: direct host MCP preflight passed, but MCPace reports Connection closed. This points to MCPace/mcp-remote transport compatibility, not host bridge startup." -f [string]$entry.Name)
        }

        $issues += [pscustomobject]@{
            Name                     = [string]$item.name
            Status                   = [string]$item.status
            Error                    = [string]$item.error
            Server                   = $item
            Entry                    = $entry
            FixHint                  = Get-ServerFixHint -Server $item
            CompatibilitySuspect     = (-not [string]::IsNullOrWhiteSpace([string]$compatibilityMessage))
            CompatibilityMessage     = $compatibilityMessage
        }
    }

    return @($issues)
}

function Get-NonStandardServerStatusSummary {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)][AllowEmptyCollection()][array]$ServerStatuses = @()
    )

    $groups = @(
        $ServerStatuses |
            Where-Object {
                $status = [string]$_.status
                -not (($null -ne $_.enabled) -and $_.enabled -eq $false) -and
                $status -notin @('connected', 'disconnected', 'connecting', 'disabled')
            } |
            Group-Object -Property status |
            Sort-Object Name
    )

    $parts = @()
    foreach ($group in $groups) {
        $parts += ("{0}={1}" -f [string]$group.Name, [int]$group.Count)
    }

    return $parts
}

function Get-ServerIssueActionText {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Issue
    )

    if ($Issue.CompatibilitySuspect) {
        return ("{0}: inspect MCPace/mcp-remote transport compatibility." -f $Issue.Name)
    }

    switch ([string]$Issue.Name) {
        'sentry' {
            if ([string]$Issue.Status -eq 'connecting' -or [string]$Issue.Status -eq 'oauth_required') {
                return 'sentry (oauth_required): approve OAuth in the MCPace web UI.'
            }
            break
        }
        'github' {
            if ([string]$Issue.Error -match '403') {
                return 'github (403): check MCP/Copilot token access.'
            }
            break
        }
        'git' {
            return 'git: point --repository to a real folder with .git.'
        }
        'lean-ctx' {
            return 'lean-ctx: inspect hub-container install status in check.ps1.'
        }
    }

    if (-not [string]::IsNullOrWhiteSpace([string]$Issue.Error)) {
        return ("{0} ({1}): {2}" -f $Issue.Name, $Issue.Status, $Issue.Error)
    }

    return ("{0} ({1})" -f $Issue.Name, $Issue.Status)
}

function Get-PortOwner {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [int]$Port
    )

    $netstatCmd = Get-Command netstat -ErrorAction SilentlyContinue
    if ($netstatCmd) {
        try {
            $pattern = ('^\s*TCP\s+\S+:{0}\s+\S+\s+LISTENING\s+(\d+)\s*$' -f $Port)
            $rows = & $netstatCmd.Source -ano -p TCP 2>$null
            foreach ($row in @($rows)) {
                $line = [string]$row
                if ($line -match $pattern) {
                    $pid = [int]$Matches[1]
                    $proc = Get-Process -Id $pid -ErrorAction SilentlyContinue
                    $startedAt = $null
                    try {
                        if ($proc) {
                            $startedAt = [DateTimeOffset]$proc.StartTime
                        }
                    }
                    catch {}
                    return [pscustomobject]@{
                        Port        = $Port
                        ProcessId   = $pid
                        Name        = if ($proc) { [string]$proc.ProcessName } else { '' }
                        CommandLine = if ($proc -and $proc.Path) { [string]$proc.Path } else { '' }
                        StartedAt   = $startedAt
                    }
                }
            }
        }
        catch {}
    }

    $netTcpCmd = Get-Command Get-NetTCPConnection -ErrorAction SilentlyContinue
    if ($netTcpCmd) {
        $connection = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1
        if (-not $connection) {
            return $null
        }

        $owningProcessId = [int]$connection.OwningProcess
        $proc = Get-Process -Id $owningProcessId -ErrorAction SilentlyContinue
        $startedAt = $null
        try {
            if ($proc) {
                $startedAt = [DateTimeOffset]$proc.StartTime
            }
        }
        catch {}

        return [pscustomobject]@{
            Port        = $Port
            ProcessId   = $owningProcessId
            Name        = if ($proc) { [string]$proc.ProcessName } else { '' }
            CommandLine = if ($proc -and $proc.Path) { [string]$proc.Path } else { '' }
            StartedAt   = $startedAt
        }
    }

    $lsofCmd = Get-Command lsof -ErrorAction SilentlyContinue
    if ($lsofCmd) {
        try {
            $rows = & $lsofCmd.Source -nP -iTCP:$Port -sTCP:LISTEN -Fpc 2>$null
            if ($LASTEXITCODE -eq 0 -and $rows) {
                $pid = $null
                $commandLine = ''
                foreach ($row in @($rows)) {
                    $value = [string]$row
                    if ($value.StartsWith('p')) { $pid = [int]($value.Substring(1)) }
                    elseif ($value.StartsWith('c')) { $commandLine = [string]$value.Substring(1) }
                    if ($pid -and $commandLine) { break }
                }
                if ($pid) {
                    $fullCommand = $commandLine
                    $proc = Get-Process -Id $pid -ErrorAction SilentlyContinue
                    $startedAt = $null
                    $psCmd = Get-Command ps -ErrorAction SilentlyContinue
                    if ($psCmd) {
                        try {
                            $psLine = (& $psCmd.Source -p $pid -o command= 2>$null | Select-Object -First 1)
                            if (-not [string]::IsNullOrWhiteSpace([string]$psLine)) {
                                $fullCommand = [string]$psLine
                            }
                        }
                        catch {}
                    }
                    try {
                        if ($proc) {
                            $startedAt = [DateTimeOffset]$proc.StartTime
                        }
                    }
                    catch {}
                    return [pscustomobject]@{
                        Port        = $Port
                        ProcessId   = $pid
                        Name        = $commandLine
                        CommandLine = $fullCommand
                        StartedAt   = $startedAt
                    }
                }
            }
        }
        catch {}
    }

    $ssCmd = Get-Command ss -ErrorAction SilentlyContinue
    if ($ssCmd) {
        try {
            $rows = & $ssCmd.Source -ltnp "sport = :$Port" 2>$null
            foreach ($row in @($rows)) {
                $line = [string]$row
                if ($line -match 'users:\(\("([^"]+)",pid=([0-9]+)') {
                    $pid = [int]$Matches[2]
                    $name = [string]$Matches[1]
                    $fullCommand = $name
                    $proc = Get-Process -Id $pid -ErrorAction SilentlyContinue
                    $startedAt = $null
                    $psCmd = Get-Command ps -ErrorAction SilentlyContinue
                    if ($psCmd) {
                        try {
                            $psLine = (& $psCmd.Source -p $pid -o command= 2>$null | Select-Object -First 1)
                            if (-not [string]::IsNullOrWhiteSpace([string]$psLine)) {
                                $fullCommand = [string]$psLine
                            }
                        }
                        catch {}
                    }
                    try {
                        if ($proc) {
                            $startedAt = [DateTimeOffset]$proc.StartTime
                        }
                    }
                    catch {}
                    return [pscustomobject]@{
                        Port        = $Port
                        ProcessId   = $pid
                        Name        = $name
                        CommandLine = $fullCommand
                        StartedAt   = $startedAt
                    }
                }
            }
        }
        catch {}
    }

    return $null
}

function Test-IsABPProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]$PortOwner,
        [Parameter(Mandatory = $false)]$Context
    )

    if (-not $PortOwner) {
        return $false
    }

    if ($Context -and -not [string]::IsNullOrWhiteSpace([string]$Context.AbpPidPath) -and (Test-Path -LiteralPath $Context.AbpPidPath)) {
        try {
            $managedPid = [int]((Get-Content -LiteralPath $Context.AbpPidPath -ErrorAction Stop | Select-Object -First 1).Trim())
            if ($managedPid -eq [int]$PortOwner.ProcessId) {
                return $true
            }
        }
        catch {}
    }

    $cmd = [string]$PortOwner.CommandLine
    $name = [string]$PortOwner.Name
    return (($cmd -match 'agent-browser-protocol') -or ($name -ieq 'abp.exe') -or ($name -ieq 'abp'))
}

function Test-ABPReady {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    try {
        Invoke-RestMethod -Uri "http://127.0.0.1:$($Context.AbpPort)/api/v1/tabs" -TimeoutSec $Context.ProbeTimeoutSec -ErrorAction Stop | Out-Null
        return $true
    }
    catch {
        try {
            $status = Invoke-RestMethod -Uri "http://127.0.0.1:$($Context.AbpPort)/api/v1/browser/status" -TimeoutSec $Context.ProbeTimeoutSec -ErrorAction Stop
            if ($null -ne $status.data -and $status.data.ready -eq $true) {
                return $true
            }
            if ($null -ne $status.ready -and $status.ready -eq $true) {
                return $true
            }
        }
        catch {
        }

        return $false
    }
}

function Get-ABPDiagnostics {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $owner = Get-PortOwner -Port $Context.AbpPort
    $ownerIsABP = $false
    $ownerDisplay = ''
    if ($owner) {
        $ownerIsABP = Test-IsABPProcess -PortOwner $owner -Context $Context
        if ($owner.Name) {
            $ownerDisplay = ("{0} ({1})" -f $owner.Name, $owner.ProcessId)
        }
        else {
            $ownerDisplay = ("PID {0}" -f $owner.ProcessId)
        }
    }

    $endpointReachable = Test-ABPReady -Context $Context

    return [pscustomobject]@{
        Endpoint          = "http://127.0.0.1:$($Context.AbpPort)/api/v1/tabs"
        Owner             = $owner
        OwnerDisplay      = $ownerDisplay
        OwnerIsABP        = $ownerIsABP
        EndpointReachable = $endpointReachable
    }
}

function Get-ABPStateFromDiagnostics {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $true)]$Diagnostics
    )

    if (-not $Diagnostics.Owner) {
        return [pscustomobject]@{ State = 'offline'; Detail = 'port closed' }
    }

    $ageLabel = Get-RelativeAgeLabel -StartedAt $Diagnostics.Owner.StartedAt
    $agePrefix = if ([string]::IsNullOrWhiteSpace($ageLabel)) { '' } else { ", $ageLabel" }

    if ($Diagnostics.EndpointReachable) {
        if ($Diagnostics.OwnerIsABP) {
            return [pscustomobject]@{ State = 'running'; Detail = ("PID {0}{1}, endpoint reachable" -f $Diagnostics.Owner.ProcessId, $agePrefix) }
        }
        return [pscustomobject]@{ State = 'running'; Detail = ("external owner {0}{1}, endpoint reachable" -f $Diagnostics.OwnerDisplay, $agePrefix) }
    }

    if (-not $Diagnostics.OwnerIsABP) {
        return [pscustomobject]@{ State = 'blocked'; Detail = ("{0}{1}, port blocked" -f $Diagnostics.OwnerDisplay, $agePrefix) }
    }

    return [pscustomobject]@{ State = 'starting'; Detail = ("PID {0}{1}, endpoint not ready" -f $Diagnostics.Owner.ProcessId, $agePrefix) }
}

function Get-ABPDisplayModelFromDiagnostics {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $true)]$Diagnostics
    )

    $stateModel = Get-ABPStateFromDiagnostics -Context $Context -Diagnostics $Diagnostics
    if (-not $Diagnostics.Owner) {
        return [pscustomobject]@{
            State          = $stateModel.State
            HeadlineMetric = 'port closed'
            Detail         = 'ABP is not listening on the configured port.'
        }
    }

    $ageLabel = Get-RelativeAgeLabel -StartedAt $Diagnostics.Owner.StartedAt
    $headlineMetric = if ([string]::IsNullOrWhiteSpace($ageLabel)) { '' } else { $ageLabel }
    $detail = switch ($stateModel.State) {
        'running' {
            if ($Diagnostics.OwnerIsABP) {
                "PID $($Diagnostics.Owner.ProcessId), endpoint reachable."
            }
            else {
                "External owner $($Diagnostics.OwnerDisplay), endpoint reachable."
            }
        }
        'blocked' {
            "Owner $($Diagnostics.OwnerDisplay) is using the configured port."
        }
        'starting' {
            "PID $($Diagnostics.Owner.ProcessId), endpoint is not ready yet."
        }
        default {
            $stateModel.Detail
        }
    }

    if ([string]::IsNullOrWhiteSpace($headlineMetric)) {
        $headlineMetric = switch ($stateModel.State) {
            'blocked' { 'port blocked' }
            'starting' { 'starting' }
            'running' { 'endpoint ready' }
            default { 'offline' }
        }
    }

    return [pscustomobject]@{
        State          = $stateModel.State
        HeadlineMetric = $headlineMetric
        Detail         = $detail
    }
}

function Get-ABPState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $diag = Get-ABPDiagnostics -Context $Context
    return (Get-ABPStateFromDiagnostics -Context $Context -Diagnostics $diag)
}

function Start-ABP {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $owner = Get-PortOwner -Port $Context.AbpPort
    if ($owner) {
        if (Test-IsABPProcess -PortOwner $owner -Context $Context) {
            if (Test-ABPReady -Context $Context) {
                Write-LauncherLog -Context $Context -Message ("ABP already ready on port {0} (PID {1}). Reusing existing process." -f $Context.AbpPort, $owner.ProcessId)
                return
            }
            Stop-Process -Id $owner.ProcessId -Force -ErrorAction Stop
            Start-Sleep -Milliseconds 800
        }
        else {
            if (Test-ABPReady -Context $Context) {
                Write-LauncherLog -Context $Context -Message ("ABP-compatible endpoint already reachable on port {0} via {1} ({2}). Reusing external owner." -f $Context.AbpPort, $owner.Name, $owner.ProcessId)
                return
            }

            $ownerName = [string]$owner.Name
            if ($ownerName -ieq 'wslrelay.exe') {
                Write-LauncherLog -Context $Context -Message ("Port {0} is blocked by stale {1} ({2}); endpoint is not reachable. Attempting automatic recovery." -f $Context.AbpPort, $ownerName, $owner.ProcessId)
                try {
                    Stop-Process -Id $owner.ProcessId -Force -ErrorAction Stop
                    Start-Sleep -Milliseconds 800
                }
                catch {
                    Write-LauncherLog -Context $Context -Message ("Failed to stop stale {0} ({1}): {2}" -f $ownerName, $owner.ProcessId, $_.Exception.Message)
                }

                $owner = Get-PortOwner -Port $Context.AbpPort
                if (-not $owner) {
                    Write-LauncherLog -Context $Context -Message ("Port {0} recovered after stopping stale {1}." -f $Context.AbpPort, $ownerName)
                }
                else {
                    if (Test-ABPReady -Context $Context) {
                        Write-LauncherLog -Context $Context -Message ("ABP endpoint became reachable on port {0} after recovery attempt. Reusing owner {1} ({2})." -f $Context.AbpPort, $owner.Name, $owner.ProcessId)
                        return
                    }
                    throw "Port $($Context.AbpPort) is still used by process $($owner.ProcessId) $($owner.Name) after automatic recovery. Free the port or change it in mcpace.config.json."
                }
            }
            
            if ($owner) {
                $dockerOwners = @(Get-DockerPortPublishers -HostPort $Context.AbpPort)
                if ($dockerOwners.Count -gt 0) {
                    $names = ($dockerOwners | ForEach-Object { $_.Name }) -join ', '
                    throw "Port $($Context.AbpPort) is used by process $($owner.ProcessId) $($owner.Name). Docker containers publishing this port: $names. Stop/remap them, then start MCPace again."
                }
                throw "Port $($Context.AbpPort) is already used by process $($owner.ProcessId) $($owner.Name). Free the port or change it in mcpace.config.json."
            }
        }
    }

    $stdoutPath = Join-Path $Context.LogsDir 'abp.stdout.log'
    $stderrPath = Join-Path $Context.LogsDir 'abp.stderr.log'
    $args = @('-y', $Context.AbpPackage, '--port', "$($Context.AbpPort)")
    foreach ($extra in @($Context.AbpPackageArgs)) {
        if ($null -ne $extra -and [string]$extra -ne '') {
            $args += [string]$extra
        }
    }

    Write-LauncherLog -Context $Context -Message ("Starting ABP on port {0}" -f $Context.AbpPort)
    $startParams = @{
        FilePath = $Context.NpxCommand
        ArgumentList = $args
        RedirectStandardOutput = $stdoutPath
        RedirectStandardError = $stderrPath
        PassThru = $true
    }
    if ($Context.IsWindows) {
        $startParams.WindowStyle = 'Hidden'
    }
    $process = Start-Process @startParams
    try {
        Set-Content -LiteralPath $Context.AbpPidPath -Value ([string]$process.Id) -Encoding ASCII
    }
    catch {}
}

function Stop-ABP {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $owner = Get-PortOwner -Port $Context.AbpPort
    if (-not $owner) {
        return $false
    }

    if (-not (Test-IsABPProcess -PortOwner $owner -Context $Context)) {
        throw "Port $($Context.AbpPort) is used by a different process: $($owner.Name) ($($owner.ProcessId))."
    }

    Write-LauncherLog -Context $Context -Message ("Stopping ABP process {0}" -f $owner.ProcessId)
    Stop-Process -Id $owner.ProcessId -Force -ErrorAction Stop
    try {
        if (Test-Path -LiteralPath $Context.AbpPidPath) {
            Remove-Item -LiteralPath $Context.AbpPidPath -Force -ErrorAction SilentlyContinue
        }
    }
    catch {}
    return $true
}

function Get-HubSettingsHash {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    try {
        return [string](Get-FileHash -LiteralPath $Context.SettingsEffectivePath -Algorithm SHA256).Hash
    }
    catch {
        return 'missing'
    }
}

function Get-HubSignature {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $mountParts = @()
    foreach ($mount in @($Context.WorkspaceRegistry.Mounts)) {
        $mode = if ($mount.ReadOnly) { 'ro' } else { 'rw' }
        $mountParts += ("{0}:{1}:{2}:{3}" -f $mount.WorkspaceName, $mount.HostPath, $mount.ContainerPath, $mode)
    }
    $workspaceHash = Get-TextSha256 -Text ($mountParts -join '|')
    $stateHash = Get-TextSha256 -Text ("state={0};settings={1};data={2}" -f $Context.StateRoot, $Context.SettingsEffectivePath, $Context.DataDir)
    return ("image={0};port={1};manager={2};workspaces={3};state={4}" -f $Context.HubImage, $Context.HubPort, $Context.ManagerRoot, $workspaceHash, $stateHash)
}

function Get-HubContainer {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    return (Get-DockerContainerByName -ContainerName $Context.HubContainerName)
}

function Test-HubContainerMatches {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $true)]$Container
    )

    if (-not $Container) {
        return $false
    }

    $labels = $Container.Config.Labels
    if (-not $labels) {
        return $false
    }

    if ([string]$Container.Config.Image -ne [string]$Context.HubImage) {
        return $false
    }

    return (
        [string]$labels.'mcpace.managed' -eq 'true' -and
        [string]$labels.'mcpace.signature' -eq (Get-HubSignature -Context $Context)
    )
}

function Start-Hub {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $settingsAbs = (Resolve-Path -LiteralPath $Context.SettingsEffectivePath).Path
    $dataAbs = (Resolve-Path -LiteralPath $Context.DataDir).Path
    $container = Get-HubContainer -Context $Context
    $desiredSettingsHash = Get-HubSettingsHash -Context $Context
    $hubRuntimeState = Read-HubRuntimeState -Path $Context.HubStatePath

    if ($container -and -not (Test-HubContainerMatches -Context $Context -Container $container)) {
        Write-LauncherLog -Context $Context -Message 'Existing MCPace container does not match current config. Recreating.'
        docker rm -f $Context.HubContainerName 2>$null | Out-Null
        $container = $null
    }

    if ($container) {
        if ([bool]$container.State.Running) {
            if ([string]$hubRuntimeState.appliedSettingsHash -ne $desiredSettingsHash) {
                Write-LauncherLog -Context $Context -Message 'Restarting existing MCPace container to apply updated effective settings.'
                $restartOutput = docker restart $Context.HubContainerName 2>&1
                if ($LASTEXITCODE -ne 0) {
                    throw ('Failed to restart MCPace container. docker error: {0}' -f (($restartOutput | Out-String).Trim()))
                }
            }
            else {
                Write-LauncherLog -Context $Context -Message 'MCPace container is already running with current effective settings.'
            }
        }
        else {
            Write-LauncherLog -Context $Context -Message 'Starting existing MCPace container.'
            $startOutput = docker start $Context.HubContainerName 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw ('Failed to start MCPace container. docker error: {0}' -f (($startOutput | Out-String).Trim()))
            }
        }
        Write-HubRuntimeState -Path $Context.HubStatePath -AppliedSettingsHash $desiredSettingsHash | Out-Null
        return
    }

    Write-LauncherLog -Context $Context -Message ("Creating MCPace container from image {0}" -f $Context.HubImage)
    docker pull $Context.HubImage 2>$null | Out-Null
    $signature = Get-HubSignature -Context $Context

    $portArgs = @(
        "-p", "127.0.0.1:$($Context.HubPort):3000"
    )

    $oauthPort = 3000
    $oauthHost = '127.0.0.1'
    $isOAuthPortBusy = $false
    try {
        $occupied = Get-PortOwner -Port $oauthPort
        if ($null -ne $occupied) {
            $isOAuthPortBusy = $true
        }
    }
    catch {
        $isOAuthPortBusy = $false
    }

    if (-not $isOAuthPortBusy -and $Context.HubPort -ne $oauthPort) {
        $portArgs += @('-p', ('{0}:{1}:3000' -f $oauthHost, $oauthPort))
        Write-LauncherLog -Context $Context -Message ("Published MCPace OAuth callback compatibility port {0}:{1}." -f $oauthHost, $oauthPort)
    }

    $dockerArgs = @(
        'run',
        '-d',
        '--name',
        $Context.HubContainerName,
        '--restart',
        'unless-stopped',
        '--label',
        'mcpace.managed=true',
        '--label',
        "mcpace.signature=$signature"
    )
    $dockerArgs += $portArgs
    if ($Context.IsLinux -and -not [string]::IsNullOrWhiteSpace([string]$Context.DockerHostAlias)) {
        $dockerArgs += @('--add-host', "$($Context.DockerHostAlias):host-gateway")
    }
    $dockerArgs += @(
        '-v',
        "${settingsAbs}:/app/mcp_settings.json",
        '-v',
        "${dataAbs}:/app/data"
    )
    foreach ($mount in @($Context.WorkspaceRegistry.Mounts)) {
        $mountSpec = ("{0}:{1}" -f $mount.HostPath, $mount.ContainerPath)
        if ($mount.ReadOnly) {
            $mountSpec = "{0}:ro" -f $mountSpec
        }
        $dockerArgs += @('-v', $mountSpec)
    }
    $dockerArgs += $Context.HubImage

    Write-LauncherLog -Context $Context -Message ("Starting MCPace command: docker {0}" -f ($dockerArgs -join ' '))

    $dockerOutput = & docker @dockerArgs 2>&1
    $startError = $dockerOutput | Out-String
    if ($LASTEXITCODE -ne 0) {
        Write-LauncherLog -Context $Context -Message ("docker run failed with exit code {0}: {1}" -f $LASTEXITCODE, $startError.Trim())
        throw ('Failed to start MCPace container. docker error: {0}' -f $startError.Trim())
    }

    if ($dockerOutput) {
        Write-LauncherLog -Context $Context -Message ("MCPace container start command returned: {0}" -f ($dockerOutput | Out-String).Trim())
    }
    Write-HubRuntimeState -Path $Context.HubStatePath -AppliedSettingsHash $desiredSettingsHash | Out-Null
}

function Stop-Hub {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    Write-LauncherLog -Context $Context -Message 'Stopping MCPace container.'
    docker stop $Context.HubContainerName 2>$null | Out-Null
}

function Remove-Hub {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    Write-LauncherLog -Context $Context -Message 'Removing MCPace container.'
    docker rm -f $Context.HubContainerName 2>$null | Out-Null
}

function Get-HubHealth {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    try {
        $response = Invoke-RestMethod -Uri "http://127.0.0.1:$($Context.HubPort)/health" -TimeoutSec $Context.ProbeTimeoutSec -ErrorAction Stop
        return [string]$response.status
    }
    catch {
        return 'offline'
    }
}

function Get-HubHealthInfo {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    try {
        $response = Invoke-RestMethod -Uri "http://127.0.0.1:$($Context.HubPort)/health" -TimeoutSec $Context.ProbeTimeoutSec -ErrorAction Stop
        $total = 0
        $connected = 0
        $disconnected = 0

        if ($null -ne $response.servers) {
            if ($null -ne $response.servers.total) { $total = [int]$response.servers.total }
            if ($null -ne $response.servers.connected) { $connected = [int]$response.servers.connected }
            if ($null -ne $response.servers.disconnected) { $disconnected = [int]$response.servers.disconnected }
        }

        return [pscustomobject]@{
            Status             = [string]$response.status
            Message            = [string]$response.message
            ServersTotal       = $total
            ServersConnected   = $connected
            ServersDisconnected = $disconnected
            Raw                = $response
        }
    }
    catch {
        return [pscustomobject]@{
            Status             = 'offline'
            Message            = [string]$_.Exception.Message
            ServersTotal       = 0
            ServersConnected   = 0
            ServersDisconnected = 0
            Raw                = $null
        }
    }
}

function Get-HubServerStatuses {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $headers = @{ Authorization = "Bearer $($Context.BearerToken)" }
    try {
        $servers = Invoke-RestMethod -Uri "http://127.0.0.1:$($Context.HubPort)/api/servers" -Headers $headers -TimeoutSec ([Math]::Max($Context.ProbeTimeoutSec, 5)) -ErrorAction Stop
        if ($null -eq $servers -or $null -eq $servers.data) {
            return @()
        }
        return @($servers.data)
    }
    catch {
        return @()
    }
}

function Get-RequiredServerConnectivity {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $false)][AllowEmptyCollection()][array]$ServerStatuses = @()
    )

    $statusMap = @{}
    foreach ($serverStatus in @($ServerStatuses)) {
        $statusMap[[string]$serverStatus.name] = [string]$serverStatus.status
    }

    $connected = @()
    $disconnected = @()
    foreach ($name in @($Context.RequiredServerNames)) {
        $serverName = [string]$name
        if ([string]::IsNullOrWhiteSpace($serverName)) { continue }

        if ($statusMap.ContainsKey($serverName) -and [string]$statusMap[$serverName] -eq 'connected') {
            $connected += $serverName
        }
        else {
            $disconnected += $serverName
        }
    }

    return [pscustomobject]@{
        Required     = @($Context.RequiredServerNames)
        Connected    = $connected
        Disconnected = $disconnected
    }
}

function Test-HubReady {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $health = Get-HubHealthInfo -Context $Context
    if ($health.Status -ne 'healthy' -and $health.Status -ne 'degraded') {
        return $false
    }

    if ($health.ServersTotal -gt 0 -and $health.ServersConnected -lt 1) {
        return $false
    }

    $serverStatuses = @(Get-HubServerStatuses -Context $Context)
    if ($serverStatuses.Count -eq 0) {
        return $false
    }

    $requiredConnectivity = Get-RequiredServerConnectivity -Context $Context -ServerStatuses $serverStatuses
    if ($requiredConnectivity.Required.Count -gt 0) {
        return ($requiredConnectivity.Disconnected.Count -eq 0)
    }

    return (@($serverStatuses | Where-Object { [string]$_.status -eq 'connected' }).Count -ge 1)
}

function Get-HubStateFromSnapshot {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $false)]$Container,
        [Parameter(Mandatory = $false)]$HealthInfo,
        [Parameter(Mandatory = $false)][AllowEmptyCollection()][array]$ServerStatuses = @()
    )

    if (-not $Container) {
        return [pscustomobject]@{ State = 'offline'; Detail = '' }
    }

    if (-not [bool]$Container.State.Running) {
        return [pscustomobject]@{ State = 'offline'; Detail = 'container stopped' }
    }

    $health = if ($HealthInfo) { $HealthInfo } else { Get-HubHealthInfo -Context $Context }
    $requiredConnectivity = Get-RequiredServerConnectivity -Context $Context -ServerStatuses $ServerStatuses
    $detailParts = @()
    $ageLabel = Get-RelativeAgeLabel -StartedAt (Get-DockerContainerStartedAt -Container $Container)

    switch ($health.Status) {
        'healthy' {
            if ($requiredConnectivity.Required.Count -gt 0 -and $requiredConnectivity.Disconnected.Count -gt 0) {
                $detailParts += ("required path broken; missing {0}" -f ($requiredConnectivity.Disconnected -join ', '))
            }
            else {
                $detailParts += 'required path ready'
            }
        }
        'degraded' {
            if ($requiredConnectivity.Required.Count -gt 0 -and $requiredConnectivity.Disconnected.Count -gt 0) {
                $detailParts += ("required path broken; missing {0}" -f ($requiredConnectivity.Disconnected -join ', '))
            }
            else {
                $detailParts += 'required path ready; optional degraded'
            }
        }
        'offline' {
            $detailParts += 'container running'
        }
        default {
            if (-not [string]::IsNullOrWhiteSpace([string]$health.Message)) {
                $detailParts += [string]$health.Message
            }
        }
    }

    if ($health.ServersTotal -gt 0) {
        $detailParts += ("servers {0}/{1}" -f $health.ServersConnected, $health.ServersTotal)
    }

    if (-not [string]::IsNullOrWhiteSpace($ageLabel)) {
        $detailParts += $ageLabel
    }

    $detail = ($detailParts | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) }) -join ', '
    $state = switch ($health.Status) {
        'healthy' { 'healthy' }
        'degraded' { 'degraded' }
        'offline' { 'starting' }
        default { [string]$health.Status }
    }

    return [pscustomobject]@{
        State  = $state
        Detail = $detail
    }
}

function Get-HubDisplayModelFromSnapshot {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $false)]$Container,
        [Parameter(Mandatory = $false)]$HealthInfo,
        [Parameter(Mandatory = $false)][AllowEmptyCollection()][array]$ServerStatuses = @()
    )

    $stateModel = Get-HubStateFromSnapshot -Context $Context -Container $Container -HealthInfo $HealthInfo -ServerStatuses $ServerStatuses
    if (-not $Container) {
        return [pscustomobject]@{
            State          = $stateModel.State
            HeadlineMetric = 'container offline'
            Detail         = 'MCPace container is not running.'
        }
    }

    if (-not [bool]$Container.State.Running) {
        return [pscustomobject]@{
            State          = $stateModel.State
            HeadlineMetric = 'container stopped'
            Detail         = 'MCPace container exists, but it is stopped.'
        }
    }

    $health = if ($HealthInfo) { $HealthInfo } else { Get-HubHealthInfo -Context $Context }
    $requiredConnectivity = Get-RequiredServerConnectivity -Context $Context -ServerStatuses $ServerStatuses
    $ageLabel = Get-RelativeAgeLabel -StartedAt (Get-DockerContainerStartedAt -Container $Container)
    $metricParts = @()
    if ($health.ServersTotal -gt 0) {
        $metricParts += ("servers {0}/{1}" -f $health.ServersConnected, $health.ServersTotal)
    }
    if (-not [string]::IsNullOrWhiteSpace($ageLabel)) {
        $metricParts += $ageLabel
    }

    $detail = switch ($stateModel.State) {
        'healthy' {
            'Required path is ready.'
        }
        'degraded' {
            if ($requiredConnectivity.Disconnected.Count -gt 0) {
                "Missing required servers: $($requiredConnectivity.Disconnected -join ', ')."
            }
            else {
                'Optional MCP servers need attention; required path is ready.'
            }
        }
        'starting' {
            'Container is running, but the health endpoint is not ready yet.'
        }
        'offline' {
            'MCPace container is not running.'
        }
        default {
            if ([string]::IsNullOrWhiteSpace([string]$health.Message)) {
                "Health status: $($health.Status)."
            }
            else {
                [string]$health.Message
            }
        }
    }

    return [pscustomobject]@{
        State          = $stateModel.State
        HeadlineMetric = if ($metricParts.Count -gt 0) { $metricParts -join ', ' } else { $stateModel.State }
        Detail         = $detail
    }
}

function Get-ManagerHealthModel {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $true)]$AbpState,
        [Parameter(Mandatory = $true)]$HubState,
        [Parameter(Mandatory = $true)]$HubHealth,
        [Parameter(Mandatory = $true)]$ServerSummary,
        [Parameter(Mandatory = $true)]$RequiredConnectivity,
        [Parameter(Mandatory = $false)][AllowEmptyCollection()][array]$ServerIssues = @(),
        [Parameter(Mandatory = $false)][AllowEmptyCollection()][array]$NonStandardStatuses = @()
    )

    $summaryState = 'warn'
    $summaryText = 'stack not ready'

    if ($RequiredConnectivity.Disconnected.Count -gt 0) {
        $summaryState = 'error'
        $summaryText = ("required path broken: {0}" -f ($RequiredConnectivity.Disconnected -join ', '))
    }
    elseif ($HubState.State -eq 'degraded') {
        $summaryState = 'warn'
        $summaryText = 'required path ready; optional degraded'
    }
    elseif ($HubState.State -eq 'healthy') {
        $summaryState = 'ok'
        $summaryText = 'required path ready'
    }
    elseif ($AbpState.State -eq 'blocked') {
        $summaryState = 'error'
        $summaryText = 'ABP port blocked'
    }
    elseif ($HubState.State -eq 'starting') {
        $summaryState = 'warn'
        $summaryText = 'MCPace health starting'
    }
    elseif ($AbpState.State -eq 'starting') {
        $summaryState = 'warn'
        $summaryText = 'ABP starting'
    }
    elseif ($HubState.State -eq 'offline' -and $AbpState.State -eq 'offline') {
        $summaryState = 'error'
        $summaryText = 'stack offline'
    }

    $actionItems = New-Object System.Collections.Generic.List[string]
    if ($AbpState.State -eq 'blocked') {
        $actionItems.Add('ABP: free the configured port or change it in mcpace.config.json.')
    }
    elseif ($AbpState.State -eq 'offline') {
        $actionItems.Add('ABP: start the Browser MCP service.')
    }
    elseif ($AbpState.State -eq 'starting') {
        $actionItems.Add('ABP: wait for the endpoint to finish starting.')
    }

    if ($HubState.State -eq 'offline') {
        $actionItems.Add('MCPace: start the Docker container.')
    }
    elseif ($HubState.State -eq 'starting') {
        $actionItems.Add('MCPace: wait for the health endpoint to become ready.')
    }

    foreach ($issue in $ServerIssues) {
        $actionItems.Add((Get-ServerIssueActionText -Issue $issue))
    }

    $uniqueActions = @()
    foreach ($item in $actionItems) {
        if (-not [string]::IsNullOrWhiteSpace([string]$item) -and ($uniqueActions -notcontains [string]$item)) {
            $uniqueActions += [string]$item
        }
        if ($uniqueActions.Count -ge 3) {
            break
        }
    }

    return [pscustomobject]@{
        SummaryState       = $summaryState
        SummaryText        = $summaryText
        CounterText        = ("total={0} online={1} offline={2} connecting={3} disabled={4}" -f $ServerSummary.Total, $ServerSummary.Online, $ServerSummary.Offline, $ServerSummary.Connecting, $ServerSummary.Disabled)
        ActionItems        = $uniqueActions
        NonStandardStatuses = $NonStandardStatuses
    }
}

function Get-ManagerDashboardSnapshot {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $collectedAt = Get-Date
    $abpDiagnostics = Get-ABPDiagnostics -Context $Context
    $hubContainer = Get-HubContainer -Context $Context
    $hubHealth = Get-HubHealthInfo -Context $Context
    $serverStatuses = @(Get-HubServerStatuses -Context $Context)
    $requiredConnectivity = Get-RequiredServerConnectivity -Context $Context -ServerStatuses $serverStatuses
    $serverIssues = @(Get-ServerIssueEntries -Context $Context -ServerStatuses $serverStatuses)
    $summary = Get-ServerStatusSummary -ServerStatuses $serverStatuses
    $nonStandardStatuses = @(Get-NonStandardServerStatusSummary -ServerStatuses $serverStatuses)
    $abpState = Get-ABPStateFromDiagnostics -Context $Context -Diagnostics $abpDiagnostics
    $hubState = Get-HubStateFromSnapshot -Context $Context -Container $hubContainer -HealthInfo $hubHealth -ServerStatuses $serverStatuses
    $snapshot = [pscustomobject]@{
        CollectedAt           = $collectedAt
        AbpDiagnostics        = $abpDiagnostics
        AbpState              = $abpState
        AbpDisplay            = Get-ABPDisplayModelFromDiagnostics -Context $Context -Diagnostics $abpDiagnostics
        HubContainer          = $hubContainer
        HubHealth             = $hubHealth
        HubState              = $hubState
        HubDisplay            = Get-HubDisplayModelFromSnapshot -Context $Context -Container $hubContainer -HealthInfo $hubHealth -ServerStatuses $serverStatuses
        ServerStatuses        = $serverStatuses
        RequiredConnectivity  = $requiredConnectivity
        ServerSummary         = $summary
        ServerIssues          = $serverIssues
        NonStandardStatuses   = $nonStandardStatuses
        ProbeSuccessful       = ($abpDiagnostics.EndpointReachable -or $hubHealth.Status -ne 'offline' -or $serverStatuses.Count -gt 0)
    }
    $snapshot | Add-Member -NotePropertyName HealthModel -NotePropertyValue (
        Get-ManagerHealthModel -Context $Context -AbpState $abpState -HubState $hubState -HubHealth $hubHealth -ServerSummary $summary -RequiredConnectivity $requiredConnectivity -ServerIssues $serverIssues -NonStandardStatuses $nonStandardStatuses
    )
    return $snapshot
}

function Get-HubState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $container = Get-HubContainer -Context $Context
    $health = Get-HubHealthInfo -Context $Context
    $serverStatuses = @(Get-HubServerStatuses -Context $Context)
    return (Get-HubStateFromSnapshot -Context $Context -Container $container -HealthInfo $health -ServerStatuses $serverStatuses)
}

function Wait-Until {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [int]$TimeoutSec,
        [Parameter(Mandatory = $true)]
        [scriptblock]$Test
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        if (& $Test) {
            return $true
        }
        Start-Sleep -Milliseconds 500
    }

    return $false
}

function Ensure-HubConnectivity {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $false)][switch]$AllowReconnect
    )

    $hubReady = Wait-Until -TimeoutSec $Context.StartupTimeoutSec -Test { Test-HubReady -Context $Context }
    if ($hubReady) {
        return $true
    }

    if (-not $AllowReconnect) {
        return $false
    }

    $abpReady = Test-ABPReady -Context $Context
    $health = Get-HubHealthInfo -Context $Context
    $serverStatuses = @(Get-HubServerStatuses -Context $Context)
    $requiredConnectivity = Get-RequiredServerConnectivity -Context $Context -ServerStatuses $serverStatuses
    if ($abpReady -and $requiredConnectivity.Disconnected.Count -gt 0) {
        Write-LauncherLog -Context $Context -Message ("MCPace required path is incomplete while ABP is ready. Missing: {0}. Restarting MCPace once." -f ($requiredConnectivity.Disconnected -join ', '))
        try { Stop-Hub -Context $Context } catch {}
        Start-Sleep -Seconds 1
        Start-Hub -Context $Context
        return (Wait-Until -TimeoutSec $Context.StartupTimeoutSec -Test { Test-HubReady -Context $Context })
    }

    if ($abpReady -and $health.Status -eq 'degraded') {
        Write-LauncherLog -Context $Context -Message ("MCPace remains degraded, but required path is ready. Optional servers disconnected: {0}" -f (($serverStatuses | Where-Object { [string]$_.status -ne 'connected' -and [string]$_.name -notin $requiredConnectivity.Disconnected } | ForEach-Object { [string]$_.name }) -join ', '))
    }

    return $false
}

function Ensure-WindowsMcpHostBridge {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $runtimeEntry = Get-ServerRuntimeEntry -Context $Context -Name 'windows-mcp'
    if (-not $runtimeEntry) {
        return [pscustomobject]@{
            Name      = 'windows-mcp'
            Attempted = $false
            Success   = $true
            Message   = 'windows-mcp is not configured.'
        }
    }

    if (-not $Context.IsWindows) {
        return [pscustomobject]@{
            Name      = 'windows-mcp'
            Attempted = $false
            Success   = $true
            Message   = 'windows-mcp skipped on non-Windows platform.'
        }
    }

    if (-not [bool]$runtimeEntry.AutoStart) {
        return [pscustomobject]@{
            Name      = 'windows-mcp'
            Attempted = $false
            Success   = $true
            Message   = 'windows-mcp autoStart is disabled.'
        }
    }

    if (-not [bool]$runtimeEntry.ConfiguredEnabled -or -not [bool]$runtimeEntry.EffectiveEnabled) {
        $message = if (-not [string]::IsNullOrWhiteSpace([string]$runtimeEntry.DisabledReason)) {
            [string]$runtimeEntry.DisabledReason
        }
        else {
            'windows-mcp is disabled.'
        }
        return [pscustomobject]@{
            Name      = 'windows-mcp'
            Attempted = $false
            Success   = $true
            Message   = $message
        }
    }

    $settingsServer = $null
    if ($Context.Settings -and $Context.Settings.mcpServers -and ($Context.Settings.mcpServers.PSObject.Properties.Name -contains 'windows-mcp')) {
        $settingsServer = $Context.Settings.mcpServers.'windows-mcp'
    }
    if (-not $settingsServer) {
        return [pscustomobject]@{
            Name      = 'windows-mcp'
            Attempted = $false
            Success   = $true
            Message   = 'windows-mcp is not configured.'
        }
    }

    $commandsCheck = Test-RequiredCommandsAvailable -RequiredCommands @($runtimeEntry.RequiredCommands)
    if (-not $commandsCheck.Passed) {
        return [pscustomobject]@{
            Name      = 'windows-mcp'
            Attempted = $false
            Success   = $true
            Message   = ("windows-mcp disabled: missing command(s): {0}" -f ($commandsCheck.MissingCommands -join ', '))
        }
    }

    $scriptPath = Join-Path $Context.RootPath 'windows-mcp-host.ps1'
    if (-not (Test-Path -LiteralPath $scriptPath)) {
        return [pscustomobject]@{
            Name      = 'windows-mcp'
            Attempted = $false
            Success   = $true
            Message   = 'windows-mcp disabled: missing windows-mcp-host.ps1 launcher script.'
        }
    }

    if ([string]::IsNullOrWhiteSpace([string]$Context.PowerShellCommand)) {
        return [pscustomobject]@{
            Name      = 'windows-mcp'
            Attempted = $false
            Success   = $true
            Message   = 'windows-mcp disabled: no PowerShell executable available to start the host bridge.'
        }
    }

    $targetPort = if ($null -ne $runtimeEntry.HostBridgePort) { [int]$runtimeEntry.HostBridgePort } else { 8233 }
    $hostBridgeUrl = if (-not [string]::IsNullOrWhiteSpace([string]$runtimeEntry.HostBridgeUrl)) {
        [string]$runtimeEntry.HostBridgeUrl
    }
    else {
        Resolve-HostBridgeUrlFromSettingsServer -Server $settingsServer
    }

    $probeTimeout = [Math]::Max($Context.ProbeTimeoutSec, 5)
    $probe = Test-HostBridgeMcpEndpoint -Url $hostBridgeUrl -TimeoutSec $probeTimeout
    if ($probe.Passed) {
        return [pscustomobject]@{
            Name      = 'windows-mcp'
            Attempted = $false
            Success   = $true
            Message   = ("windows-mcp already responding: {0}" -f $probe.Detail)
        }
    }

    $owner = Get-PortOwner -Port $targetPort
    if ($owner -and ([string]$owner.CommandLine -match 'windows-mcp')) {
        try {
            Stop-Process -Id $owner.ProcessId -Force -ErrorAction Stop
            Start-Sleep -Milliseconds 800
        }
        catch {
            return [pscustomobject]@{
                Name      = 'windows-mcp'
                Attempted = $true
                Success   = $false
                Message   = ("Windows-MCP host restart failed while stopping stale process {0}: {1}" -f $owner.ProcessId, $_.Exception.Message)
            }
        }
    }
    elseif ($owner) {
        return [pscustomobject]@{
            Name      = 'windows-mcp'
            Attempted = $false
            Success   = $true
            Message   = ("windows-mcp disabled: port {0} is used by {1} ({2})." -f $targetPort, $owner.Name, $owner.ProcessId)
        }
    }

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
            [string]([Math]::Max($Context.StartupTimeoutSec, 45))
        )
        Start-Process -FilePath $Context.PowerShellCommand -ArgumentList $argumentList -WindowStyle Hidden | Out-Null
    }
    catch {
        return [pscustomobject]@{
            Name      = 'windows-mcp'
            Attempted = $true
            Success   = $false
            Message   = ("Windows-MCP host start failed: {0}" -f $_.Exception.Message)
        }
    }

    $ready = Wait-Until -TimeoutSec ([Math]::Max($Context.StartupTimeoutSec, 45)) -Test {
        (Test-HostBridgeMcpEndpoint -Url $hostBridgeUrl -TimeoutSec $probeTimeout).Passed
    }

    if (-not $ready) {
        $finalProbe = Test-HostBridgeMcpEndpoint -Url $hostBridgeUrl -TimeoutSec $probeTimeout
        return [pscustomobject]@{
            Name      = 'windows-mcp'
            Attempted = $true
            Success   = $false
            Message   = ("Windows-MCP host did not become ready on {0}: {1}" -f $hostBridgeUrl, $finalProbe.Detail)
        }
    }

    return [pscustomobject]@{
        Name      = 'windows-mcp'
        Attempted = $true
        Success   = $true
        Message   = ("windows-mcp is ready on {0}." -f $hostBridgeUrl)
    }
}

function Invoke-HostInstaller {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Entry
    )

    switch ([string]$Entry.InstallMethod) {
        'npm-global' {
            $npm = Get-Command npm -ErrorAction SilentlyContinue
            if (-not $npm) {
                return [pscustomobject]@{
                    Success = $false
                    Message = 'missing command: npm'
                }
            }
            $output = & $npm.Source install -g $Entry.InstallPackage 2>&1
        }
        'cargo' {
            $cargo = Get-Command cargo -ErrorAction SilentlyContinue
            if (-not $cargo) {
                return [pscustomobject]@{
                    Success = $false
                    Message = 'missing command: cargo'
                }
            }
            $output = & $cargo.Source install $Entry.InstallPackage 2>&1
        }
        'uvx' {
            $uv = Get-Command uv -ErrorAction SilentlyContinue
            if (-not $uv) {
                return [pscustomobject]@{
                    Success = $false
                    Message = 'missing command: uv'
                }
            }
            $output = & $uv.Source tool install $Entry.InstallPackage 2>&1
        }
        'script' {
            if ([string]::IsNullOrWhiteSpace([string]$Entry.InstallPackage)) {
                return [pscustomobject]@{
                    Success = $false
                    Message = 'installer script is empty'
                }
            }
            $powerShellCommand = Get-PreferredPowerShellCommand
            if ([string]::IsNullOrWhiteSpace([string]$powerShellCommand)) {
                return [pscustomobject]@{
                    Success = $false
                    Message = 'missing command: pwsh'
                }
            }
            $output = & $powerShellCommand -NoProfile -ExecutionPolicy Bypass -Command $Entry.InstallPackage 2>&1
        }
        default {
            return [pscustomobject]@{
                Success = $false
                Message = ("unsupported host install method: {0}" -f [string]$Entry.InstallMethod)
            }
        }
    }

    $exitCode = $LASTEXITCODE
    $text = ($output | Out-String).Trim()
    return [pscustomobject]@{
        Success = ($exitCode -eq 0)
        Message = if ($exitCode -eq 0) { if ([string]::IsNullOrWhiteSpace($text)) { 'host install completed' } else { $text } } else { if ([string]::IsNullOrWhiteSpace($text)) { ("host install failed with exit code {0}" -f $exitCode) } else { $text } }
    }
}

function Invoke-HubContainerInstaller {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $true)]$Entry
    )

    $container = Get-HubContainer -Context $Context
    if (-not $container -or -not [bool]$container.State.Running) {
        return [pscustomobject]@{
            Success = $false
            Message = 'MCPace container is not running'
        }
    }

    switch ([string]$Entry.InstallMethod) {
        'npm-global' {
            $commandText = "npm install -g {0}" -f (ConvertTo-PosixShellLiteral -Value ([string]$Entry.InstallPackage))
        }
        'cargo' {
            $commandText = "cargo install {0}" -f (ConvertTo-PosixShellLiteral -Value ([string]$Entry.InstallPackage))
        }
        'uvx' {
            $commandText = "uv tool install {0}" -f (ConvertTo-PosixShellLiteral -Value ([string]$Entry.InstallPackage))
        }
        'script' {
            if ([string]::IsNullOrWhiteSpace([string]$Entry.InstallPackage)) {
                return [pscustomobject]@{
                    Success = $false
                    Message = 'installer script is empty'
                }
            }
            $commandText = [string]$Entry.InstallPackage
        }
        default {
            return [pscustomobject]@{
                Success = $false
                Message = ("unsupported hub-container install method: {0}" -f [string]$Entry.InstallMethod)
            }
        }
    }

    $result = Invoke-HubContainerShellCommand -ContainerName $Context.HubContainerName -CommandText $commandText
    return [pscustomobject]@{
        Success = ($result.ExitCode -eq 0)
        Message = if ($result.ExitCode -eq 0) { if ([string]::IsNullOrWhiteSpace($result.Output)) { 'container install completed' } else { $result.Output } } else { if ([string]::IsNullOrWhiteSpace($result.Output)) { ("container install failed with exit code {0}" -f $result.ExitCode) } else { $result.Output } }
    }
}

function Invoke-ManagedOptionalServerInstall {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $true)]$Entry
    )

    switch ([string]$Entry.InstallTarget) {
        'host' {
            return (Invoke-HostInstaller -Entry $Entry)
        }
        'hub-container' {
            return (Invoke-HubContainerInstaller -Context $Context -Entry $Entry)
        }
        default {
            return [pscustomobject]@{
                Success = $false
                Message = ("unsupported install target: {0}" -f [string]$Entry.InstallTarget)
            }
        }
    }
}

function Ensure-ManagedOptionalServerInstalls {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $results = @()
    $installState = $Context.ServerInstallState
    $stateChanged = $false
    $before = @{}
    foreach ($entry in @($Context.ServerRuntime)) {
        $before[[string]$entry.Name] = [pscustomobject]@{
            EffectiveEnabled = [bool]$entry.EffectiveEnabled
            BinaryPresent    = [bool]$entry.BinaryPresent
            InstallStatus    = [string]$entry.InstallStatus
        }
    }

    $candidates = @($Context.ServerRuntime | Where-Object {
        -not $_.Required -and
        $_.InstallAuto -and
        $_.HasInstallRecipe -and
        $_.PlatformSupported -and
        -not $_.PlaceholderDetected -and
        -not $_.BinaryPresent
    })

    foreach ($entry in $candidates) {
        Write-LauncherLog -Context $Context -Message ("Auto-installing optional MCP server {0} via {1}/{2}." -f $entry.Name, $entry.InstallTarget, $entry.InstallMethod)
        $installResult = Invoke-ManagedOptionalServerInstall -Context $Context -Entry $entry
        $verifyProbe = Get-InstallerBinaryProbe -Installer $entry.Installer -HubContainerName $Context.HubContainerName
        $binaryPresent = [bool]$verifyProbe.Present
        $status = if ($installResult.Success -and $binaryPresent) { 'installed' } elseif ($binaryPresent) { 'ready' } else { 'failed' }
        $errorText = if ($binaryPresent) { '' } else { [string]$installResult.Message }
        $record = [pscustomobject]@{
            installStatus   = $status
            installError    = $errorText
            binaryPresent   = $binaryPresent
            lastAttemptedAt = (Get-Date).ToString('o')
            lastUpdatedAt   = (Get-Date).ToString('o')
            installTarget   = [string]$entry.InstallTarget
            installMethod   = [string]$entry.InstallMethod
            installPackage  = [string]$entry.InstallPackage
        }
        $installState = Set-ServerInstallRecordInMap -InstallState $installState -Name ([string]$entry.Name) -Record $record
        $stateChanged = $true

        $message = if ($binaryPresent) {
            ("installed/verified; binary available ({0})" -f [string]$verifyProbe.Detail)
        }
        else {
            [string]$installResult.Message
        }
        $results += [pscustomobject]@{
            Name          = [string]$entry.Name
            Attempted     = $true
            Success       = $binaryPresent
            Message       = $message
            BinaryPresent = $binaryPresent
        }
        Write-LauncherLog -Context $Context -Message ("Auto-install result for {0}: success={1}; {2}" -f $entry.Name, $binaryPresent, $message)
    }

    if ($stateChanged) {
        Write-ServerInstallState -Path $Context.InstallStatePath -State $installState
    }

    $needsRefresh = (
        $stateChanged -or
        @($Context.ServerRuntime | Where-Object { $_.InstallAuto -and [string]$_.InstallTarget -eq 'hub-container' }).Count -gt 0
    )
    $refreshedContext = if ($needsRefresh) { New-McpAceContext -RootPath $Context.RootPath -StateRoot $Context.StateRoot } else { $Context }

    $restartRequired = $false
    foreach ($entry in @($refreshedContext.ServerRuntime | Where-Object { $_.InstallAuto })) {
        $name = [string]$entry.Name
        if (-not $before.ContainsKey($name)) {
            continue
        }

        $beforeEntry = $before[$name]
        if (
            [bool]$beforeEntry.EffectiveEnabled -ne [bool]$entry.EffectiveEnabled -or
            [bool]$beforeEntry.BinaryPresent -ne [bool]$entry.BinaryPresent -or
            [string]$beforeEntry.InstallStatus -ne [string]$entry.InstallStatus
        ) {
            $restartRequired = $true
            break
        }
    }

    return [pscustomobject]@{
        Context         = $refreshedContext
        Results         = @($results)
        RestartRequired = $restartRequired
    }
}

function Ensure-ManagedHostBridges {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $results = @()
    $results += Ensure-WindowsMcpHostBridge -Context $Context

    $refreshedContext = New-McpAceContext -RootPath $Context.RootPath -StateRoot $Context.StateRoot
    return [pscustomobject]@{
        Context = $refreshedContext
        Results = @($results)
    }
}

function Start-Stack {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    Write-ClientLauncher -Context $Context | Out-Null
    Start-ABP -Context $Context
    $abpReady = Wait-Until -TimeoutSec $Context.StartupTimeoutSec -Test { Test-ABPReady -Context $Context }
    if (-not $abpReady) {
        Write-LauncherLog -Context $Context -Message 'ABP readiness probe failed; MCPace start is skipped to avoid startup race.'
        return [pscustomobject]@{
            Context  = $Context
            ABPReady = $false
            HubReady = $false
            HostBridgeResults = @()
        }
    }

    $managedBridges = Ensure-ManagedHostBridges -Context $Context
    $effectiveContext = $managedBridges.Context
    Ensure-OptionalServerDataPaths -Context $effectiveContext

    Start-Hub -Context $effectiveContext
    $managedInstalls = Ensure-ManagedOptionalServerInstalls -Context $effectiveContext
    $effectiveContext = $managedInstalls.Context
    if ($managedInstalls.RestartRequired) {
        Start-Hub -Context $effectiveContext
    }
    $hubReady = Ensure-HubConnectivity -Context $effectiveContext -AllowReconnect

    return [pscustomobject]@{
        Context  = $effectiveContext
        ABPReady = $abpReady
        HubReady = $hubReady
        HostBridgeResults = @($managedBridges.Results)
    }
}

function Ensure-StackRunning {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    Write-ClientLauncher -Context $Context | Out-Null

    $abpReady = $false
    $hubReady = $false

    if ((Get-ABPState -Context $Context).State -eq 'running') {
        $abpReady = $true
    }
    else {
        Start-ABP -Context $Context
        $abpReady = Wait-Until -TimeoutSec $Context.StartupTimeoutSec -Test { Test-ABPReady -Context $Context }
    }
    if (-not $abpReady) {
        Write-LauncherLog -Context $Context -Message 'ABP is not ready; MCPace checks are skipped to avoid startup race.'
        return [pscustomobject]@{
            Context  = $Context
            ABPReady = $false
            HubReady = $false
            HostBridgeResults = @()
        }
    }

    $managedBridges = Ensure-ManagedHostBridges -Context $Context
    $effectiveContext = $managedBridges.Context
    Ensure-OptionalServerDataPaths -Context $effectiveContext
    $hubContainer = Get-HubContainer -Context $effectiveContext
    $hubMatchesCurrentConfig = ($hubContainer -and (Test-HubContainerMatches -Context $effectiveContext -Container $hubContainer))
    if ((Get-HubState -Context $effectiveContext).State -ne 'healthy' -or -not $hubMatchesCurrentConfig) {
        Start-Hub -Context $effectiveContext
    }

    $managedInstalls = Ensure-ManagedOptionalServerInstalls -Context $effectiveContext
    $effectiveContext = $managedInstalls.Context
    if ($managedInstalls.RestartRequired) {
        Start-Hub -Context $effectiveContext
    }
    $hubReady = Ensure-HubConnectivity -Context $effectiveContext -AllowReconnect

    return [pscustomobject]@{
        Context  = $effectiveContext
        ABPReady = $abpReady
        HubReady = $hubReady
        HostBridgeResults = @($managedBridges.Results)
    }
}

function Restart-Stack {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    try { Stop-Hub -Context $Context } catch {}
    try { Stop-ABP -Context $Context } catch {}
    Start-Sleep -Seconds 1

    return (Start-Stack -Context $Context)
}

function Get-SchtasksPath {
    [CmdletBinding()]
    param()

    $cmd = Get-Command 'schtasks.exe' -ErrorAction SilentlyContinue
    if ($cmd) {
        return [string]$cmd.Source
    }

    $fallback = Join-Path $env:WINDIR 'System32\schtasks.exe'
    if (Test-Path -LiteralPath $fallback) {
        return $fallback
    }

    return $null
}

function Invoke-SchtasksSafe {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    $schtasksPath = Get-SchtasksPath
    if (-not $schtasksPath) {
        return [pscustomobject]@{
            ExitCode = 9009
            Output   = @('schtasks.exe is not available')
        }
    }

    $oldNativePreference = $null
    $hasNativePreference = Test-Path variable:PSNativeCommandUseErrorActionPreference
    if ($hasNativePreference) {
        $oldNativePreference = $PSNativeCommandUseErrorActionPreference
        $PSNativeCommandUseErrorActionPreference = $false
    }

    try {
        $out = & $schtasksPath @Arguments 2>&1
        return [pscustomobject]@{
            ExitCode = $LASTEXITCODE
            Output   = @($out)
        }
    }
    catch {
        return [pscustomobject]@{
            ExitCode = 1
            Output   = @([string]$_.Exception.Message)
        }
    }
    finally {
        if ($hasNativePreference) {
            $PSNativeCommandUseErrorActionPreference = $oldNativePreference
        }
    }
}

function Get-AutostartStatus {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    if (-not $Context.IsWindows) {
        return [pscustomobject]@{
            Exists   = $false
            Enabled  = $false
            State    = 'NotSupported'
            TaskName = $Context.AutostartTaskName
        }
    }

    try {
        $task = Get-ScheduledTask -TaskName $Context.AutostartTaskName -ErrorAction Stop
        return [pscustomobject]@{
            Exists   = $true
            Enabled  = ($task.State -ne 'Disabled')
            State    = [string]$task.State
            TaskName = $Context.AutostartTaskName
        }
    }
    catch {
        $query = Invoke-SchtasksSafe -Arguments @('/Query', '/TN', "$($Context.AutostartTaskName)", '/FO', 'LIST', '/V')
        if ($query.ExitCode -eq 0) {
            $text = ($query.Output -join "`n")
                $enabled = $true
                if ($text -match '(?i)disabled') {
                    $enabled = $false
                }
                $state = 'Ready'
                if ($text -match '(?i)running') {
                    $state = 'Running'
                }
            return [pscustomobject]@{
                Exists   = $true
                Enabled  = $enabled
                State    = $state
                TaskName = $Context.AutostartTaskName
            }
        }
    }

    return [pscustomobject]@{
        Exists   = $false
        Enabled  = $false
        State    = 'NotInstalled'
        TaskName = $Context.AutostartTaskName
    }
}

function Enable-Autostart {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    if (-not $Context.IsWindows) {
        throw 'Autostart is only implemented on Windows in this package.'
    }

    $bootScriptPath = Join-Path $Context.RootPath 'boot.ps1'
    if (-not (Test-Path -LiteralPath $bootScriptPath)) {
        throw "Missing boot script: $bootScriptPath"
    }

    $escapedBoot = $bootScriptPath.Replace('"', '""')
    $powerShellCommand = [string]$Context.PowerShellCommand
    if ([string]::IsNullOrWhiteSpace($powerShellCommand)) {
        throw 'PowerShell 7 (pwsh) is required to register autostart.'
    }
    $escapedPowerShellCommand = $powerShellCommand.Replace('"', '""')
    $cmd = "`"$escapedPowerShellCommand`" -NoProfile -ExecutionPolicy Bypass -File `"$escapedBoot`""
    $registered = $false
    try {
        $action = New-ScheduledTaskAction -Execute $powerShellCommand -Argument "-NoProfile -ExecutionPolicy Bypass -File `"$escapedBoot`""
        $trigger = New-ScheduledTaskTrigger -AtLogOn
        $principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Limited
        $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
        Register-ScheduledTask -TaskName $Context.AutostartTaskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
        $registered = $true
    }
    catch {
        $create = Invoke-SchtasksSafe -Arguments @('/Create', '/TN', "$($Context.AutostartTaskName)", '/SC', 'ONLOGON', '/TR', "$cmd", '/F')
        if ($create.ExitCode -eq 9009) {
            throw 'schtasks.exe is not available on this system.'
        }
        if ($create.ExitCode -eq 0) {
            $registered = $true
        }
        else {
            throw ("Failed to enable autostart via ScheduledTasks and schtasks.exe: {0}" -f ($create.Output -join ' '))
        }
    }

    if (-not $registered) {
        throw 'Autostart task was not created.'
    }

    Write-LauncherLog -Context $Context -Message ("Autostart task enabled: {0}" -f $Context.AutostartTaskName)

    return (Save-ManagerSettings -Context $Context `
        -LogRetentionDays $Context.LogRetentionDays `
        -BackupRetentionCount $Context.BackupRetentionCount `
        -BackupDir $Context.BackupDir `
        -AutostartTaskName $Context.AutostartTaskName `
        -AutostartEnabled $true `
        -SmokeTimeoutSec $Context.SmokeTimeoutSec)
}

function Disable-Autostart {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    if (-not $Context.IsWindows) {
        return $Context
    }

    $removed = $false
    try {
        $task = Get-ScheduledTask -TaskName $Context.AutostartTaskName -ErrorAction Stop
        if ($task) {
            Unregister-ScheduledTask -TaskName $Context.AutostartTaskName -Confirm:$false -ErrorAction Stop
            $removed = $true
        }
    }
    catch {
        $delete = Invoke-SchtasksSafe -Arguments @('/Delete', '/TN', "$($Context.AutostartTaskName)", '/F')
        if ($delete.ExitCode -eq 0) {
            $removed = $true
        }
    }

    if ($removed) {
        Write-LauncherLog -Context $Context -Message ("Autostart task removed: {0}" -f $Context.AutostartTaskName)
    }

    return (Save-ManagerSettings -Context $Context `
        -LogRetentionDays $Context.LogRetentionDays `
        -BackupRetentionCount $Context.BackupRetentionCount `
        -BackupDir $Context.BackupDir `
        -AutostartTaskName $Context.AutostartTaskName `
        -AutostartEnabled $false `
        -SmokeTimeoutSec $Context.SmokeTimeoutSec)
}

function Rotate-Logs {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $false)][int]$Days = 0
    )

    $effectiveDays = $Days
    if ($effectiveDays -lt 1) {
        $effectiveDays = $Context.LogRetentionDays
    }

    $cutoff = (Get-Date).AddDays(-$effectiveDays)
    $removed = @()
    if (Test-Path -LiteralPath $Context.LogsDir) {
        $toDelete = Get-ChildItem -LiteralPath $Context.LogsDir -File -Filter '*.log' -ErrorAction SilentlyContinue | Where-Object { $_.LastWriteTime -lt $cutoff }
        foreach ($item in $toDelete) {
            Remove-Item -LiteralPath $item.FullName -Force -ErrorAction SilentlyContinue
            $removed += $item.FullName
        }
    }

    Write-LauncherLog -Context $Context -Message ("Log rotation complete. Removed {0} file(s) older than {1} day(s)." -f $removed.Count, $effectiveDays)
    return [pscustomobject]@{
        Days         = $effectiveDays
        RemovedCount = $removed.Count
        Removed      = $removed
    }
}

function New-DataBackup {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    $backupRoot = $Context.BackupDir
    if (-not [System.IO.Path]::IsPathRooted($backupRoot)) {
        $backupRoot = Join-Path $Context.RootPath $backupRoot
    }

    New-Item -ItemType Directory -Path $backupRoot -Force | Out-Null

    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $zipPath = Join-Path $backupRoot ("mcpace-data-{0}.zip" -f $stamp)

    if (-not (Test-Path -LiteralPath $Context.DataDir)) {
        throw "Missing data directory: $($Context.DataDir)"
    }

    $stagingRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("mcpace-backup-{0}" -f ([guid]::NewGuid().ToString('N')))
    $stagingData = Join-Path $stagingRoot 'data'
    New-Item -ItemType Directory -Path $stagingData -Force | Out-Null
    Copy-Item -Path (Join-Path $Context.DataDir '*') -Destination $stagingData -Recurse -Force -ErrorAction SilentlyContinue
    $hasAnyContent = (Get-ChildItem -LiteralPath $stagingData -Recurse -Force -ErrorAction SilentlyContinue | Select-Object -First 1)
    if (-not $hasAnyContent) {
        Set-Content -LiteralPath (Join-Path $stagingData '_EMPTY.txt') -Value ("Data directory was empty at {0}" -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss')) -Encoding UTF8
    }

    Compress-Archive -Path (Join-Path $stagingData '*') -DestinationPath $zipPath -CompressionLevel Optimal -Force
    Remove-Item -LiteralPath $stagingRoot -Recurse -Force -ErrorAction SilentlyContinue

    $deleted = @()
    $archives = @(Get-ChildItem -LiteralPath $backupRoot -File -Filter 'mcpace-data-*.zip' -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending)
    if ($archives.Count -gt $Context.BackupRetentionCount) {
        $toDelete = $archives | Select-Object -Skip $Context.BackupRetentionCount
        foreach ($old in $toDelete) {
            Remove-Item -LiteralPath $old.FullName -Force -ErrorAction SilentlyContinue
            $deleted += $old.FullName
        }
    }

    Write-LauncherLog -Context $Context -Message ("Backup created: {0}. Purged {1} old backup(s)." -f $zipPath, $deleted.Count)
    return [pscustomobject]@{
        BackupPath        = $zipPath
        BackupRoot        = $backupRoot
        PurgedCount       = $deleted.Count
        Purged            = $deleted
        RetentionCount    = $Context.BackupRetentionCount
    }
}

function Invoke-SmokeTest {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context
    )

    if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
        throw 'Node.js is required for smoke test.'
    }

    $oldEndpoint = $env:MCPACE_ENDPOINT
    $oldBaseUrl = $env:MCPACE_BASE_URL
    $oldAuth = $env:MCPACE_AUTH
    $oldTimeout = $env:MCPACE_TIMEOUT_SEC
    $oldSessionGate = $env:MCPACE_SESSION_GATE_ENABLED
    $oldStreamProbeMs = $env:MCPACE_STREAM_PROBE_MS
    $oldIdleDelayMs = $env:MCPACE_IDLE_DELAY_MS

    $env:MCPACE_ENDPOINT = "http://127.0.0.1:$($Context.HubPort)/mcp"
    $env:MCPACE_BASE_URL = "http://127.0.0.1:$($Context.HubPort)"
    $env:MCPACE_AUTH = "Bearer $($Context.BearerToken)"
    $env:MCPACE_TIMEOUT_SEC = [string]$Context.SmokeTimeoutSec
    $env:MCPACE_SESSION_GATE_ENABLED = if ($Context.SessionGateEnabled) { 'true' } else { 'false' }
    $env:MCPACE_STREAM_PROBE_MS = [string]$Context.SessionGateStreamProbeMs
    $env:MCPACE_IDLE_DELAY_MS = [string]$Context.SessionGateIdleDelayMs

    $nodeScript = @'
const endpoint = process.env.MCPACE_ENDPOINT;
const baseUrl = process.env.MCPACE_BASE_URL;
const auth = process.env.MCPACE_AUTH;
const timeoutSec = Number(process.env.MCPACE_TIMEOUT_SEC || "30");
const sessionGateEnabled = process.env.MCPACE_SESSION_GATE_ENABLED === "true";
const streamProbeMs = Number(process.env.MCPACE_STREAM_PROBE_MS || "750");
const idleDelayMs = Number(process.env.MCPACE_IDLE_DELAY_MS || "1500");

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function parsePayload(text) {
  let lastData = null;
  for (const line of text.split(/\r?\n/)) {
    if (line.startsWith("data:")) {
      lastData = line.slice(5).trim();
    }
  }
  if (lastData) {
    try { return JSON.parse(lastData); } catch {}
  }
  try { return JSON.parse(text); } catch {}
  return null;
}

async function rpc(body, sessionId) {
  const headers = {
    "authorization": auth,
    "content-type": "application/json",
    "accept": "application/json, text/event-stream"
  };
  if (sessionId) headers["mcp-session-id"] = sessionId;

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutSec * 1000);
  try {
    const res = await fetch(endpoint, {
      method: "POST",
      headers,
      body: JSON.stringify(body),
      signal: controller.signal
    });
    const text = await res.text();
    return {
      status: res.status,
      sessionId: res.headers.get("mcp-session-id") || sessionId || "",
      payload: parsePayload(text),
      raw: text
    };
  } finally {
    clearTimeout(timer);
  }
}

async function httpGetJson(url, includeAuth) {
  const headers = {
    "accept": "application/json"
  };
  if (includeAuth) {
    headers["authorization"] = auth;
  }

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutSec * 1000);
  try {
    const res = await fetch(url, {
      method: "GET",
      headers,
      signal: controller.signal
    });
    const text = await res.text();
    return {
      status: res.status,
      payload: parsePayload(text),
      raw: text
    };
  } finally {
    clearTimeout(timer);
  }
}

async function initializeSession(clientName) {
  const init = await rpc({
    jsonrpc: "2.0",
    id: 0,
    method: "initialize",
    params: {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: clientName, version: "1.0.0" }
    }
  });

  if (init.status !== 200) throw new Error(`initialize failed with status ${init.status}`);
  if (!init.sessionId) throw new Error("initialize did not return mcp-session-id");

  await rpc({
    jsonrpc: "2.0",
    method: "notifications/initialized"
  }, init.sessionId);

  return init;
}

async function expectArrayResponse(method, id, sessionId, fieldName) {
  const response = await rpc({
    jsonrpc: "2.0",
    id,
    method,
    params: {}
  }, sessionId);

  if (response.status !== 200) {
    throw new Error(`${method} failed with status ${response.status}`);
  }

  const result = response.payload?.result;
  const items = result?.[fieldName];
  if (items !== undefined && !Array.isArray(items)) {
    throw new Error(`${method} response field ${fieldName} is not an array`);
  }

  return Array.isArray(items) ? items.length : 0;
}

async function probeStream(sessionId) {
  const controller = new AbortController();
  const headers = {
    "authorization": auth,
    "accept": "text/event-stream",
    "mcp-session-id": sessionId
  };

  const response = await fetch(endpoint, {
    method: "GET",
    headers,
    signal: controller.signal
  });

  if (response.status !== 200) {
    throw new Error(`stream probe failed with status ${response.status}`);
  }

  await sleep(streamProbeMs);
  controller.abort();
  await sleep(100);
}

async function main() {
  const health = await httpGetJson(`${baseUrl}/health`, false);
  if (health.status !== 200) throw new Error(`/health failed with status ${health.status}`);
  const healthStatus = String(health.payload?.status || "");
  if (healthStatus !== "healthy" && healthStatus !== "degraded") {
    throw new Error(`/health returned unsupported status ${healthStatus || "<missing>"}`);
  }

  const servers = await httpGetJson(`${baseUrl}/api/servers`, true);
  if (servers.status !== 200) throw new Error(`/api/servers failed with status ${servers.status}`);
  if (!Array.isArray(servers.payload?.data)) {
    throw new Error("/api/servers response field data is not an array");
  }

  const primary = await initializeSession("mcpace-smoke-primary");
  const toolsCount = await expectArrayResponse("tools/list", 1, primary.sessionId, "tools");
  const resourcesCount = await expectArrayResponse("resources/list", 2, primary.sessionId, "resources");
  const resourceTemplatesCount = await expectArrayResponse("resources/templates/list", 3, primary.sessionId, "resourceTemplates");

  if (sessionGateEnabled && streamProbeMs > 0) {
    await probeStream(primary.sessionId);
  }

  await sleep(idleDelayMs);
  const repeatToolsCount = await expectArrayResponse("tools/list", 4, primary.sessionId, "tools");

  const reconnect = await initializeSession("mcpace-smoke-reconnect");
  const reconnectToolsCount = await expectArrayResponse("tools/list", 5, reconnect.sessionId, "tools");

  const out = {
    ok: true,
    sessionIds: [primary.sessionId, reconnect.sessionId],
    primarySessionId: primary.sessionId,
    reconnectSessionId: reconnect.sessionId,
    toolsCount,
    repeatToolsCount,
    reconnectToolsCount,
    resourcesCount,
    resourceTemplatesCount,
    healthStatus,
    serverCount: servers.payload.data.length,
    serverVersion: primary.payload?.result?.serverInfo?.version || ""
  };
  console.log(JSON.stringify(out));
}

main().catch((err) => {
  const out = { ok: false, error: String(err && err.message ? err.message : err) };
  console.log(JSON.stringify(out));
  process.exit(1);
});
'@

    $raw = $nodeScript | node -
    $exitCode = $LASTEXITCODE

    $env:MCPACE_ENDPOINT = $oldEndpoint
    $env:MCPACE_BASE_URL = $oldBaseUrl
    $env:MCPACE_AUTH = $oldAuth
    $env:MCPACE_TIMEOUT_SEC = $oldTimeout
    $env:MCPACE_SESSION_GATE_ENABLED = $oldSessionGate
    $env:MCPACE_STREAM_PROBE_MS = $oldStreamProbeMs
    $env:MCPACE_IDLE_DELAY_MS = $oldIdleDelayMs

    if ([string]::IsNullOrWhiteSpace($raw)) {
        throw 'Smoke test produced no output.'
    }

    $parsed = $null
    try {
        $parsed = $raw | ConvertFrom-Json
    }
    catch {
        throw ("Smoke test output is not JSON: {0}" -f [string]$raw)
    }

    if ($exitCode -ne 0 -or -not $parsed.ok) {
        $msg = [string]$parsed.error
        if ([string]::IsNullOrWhiteSpace($msg)) {
            $msg = 'Unknown smoke test failure.'
        }
        return [pscustomobject]@{
            Success    = $false
            Message    = $msg
            ToolsCount = 0
            SessionId  = ''
        }
    }

    $logIssues = @()
    if ($Context.SessionGateEnabled) {
        $logPath = Join-Path $Context.LogsDir 'mcpace.current.log'
        if (Test-Path -LiteralPath $logPath) {
            $tail = @(Get-Content -LiteralPath $logPath -Tail $Context.SessionGateLogTailLines -ErrorAction SilentlyContinue)
            foreach ($sessionId in @($parsed.sessionIds)) {
                $sessionText = [string]$sessionId
                if ([string]::IsNullOrWhiteSpace($sessionText)) { continue }

                $sessionErrors = @($tail | Where-Object {
                    $_ -match [regex]::Escape($sessionText) -and (
                        $_ -match '\[SESSION ERROR\]' -or
                        $_ -match 'Transport closed' -or
                        $_ -match 'Session .* not found'
                    )
                })
                foreach ($line in $sessionErrors) {
                    $logIssues += [string]$line
                }
            }
        }
    }

    if ($logIssues.Count -gt 0) {
        return [pscustomobject]@{
            Success    = $false
            Message    = ("Compatibility gate failed: session errors detected in MCPace logs for smoke-test sessions.`n{0}" -f (($logIssues | Select-Object -Unique) -join "`n"))
            ToolsCount = [int]$parsed.toolsCount
            SessionId  = [string]$parsed.primarySessionId
        }
    }

    return [pscustomobject]@{
        Success    = $true
        Message    = ("Smoke test passed. health={0}, servers={1}, tools={2}, repeat-tools={3}, reconnect-tools={4}, resources={5}, templates={6}." -f [string]$parsed.healthStatus, [int]$parsed.serverCount, [int]$parsed.toolsCount, [int]$parsed.repeatToolsCount, [int]$parsed.reconnectToolsCount, [int]$parsed.resourcesCount, [int]$parsed.resourceTemplatesCount)
        ToolsCount = [int]$parsed.toolsCount
        SessionId  = [string]$parsed.primarySessionId
    }
}

function Invoke-Install {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $false)][switch]$RunSmoke
    )

    Assert-Prerequisites -Context $Context
    $rotation = Rotate-Logs -Context $Context
    $stack = Ensure-StackRunning -Context $Context

    $smoke = [pscustomobject]@{
        Success = $false
        Message = 'Smoke test skipped.'
    }
    if ($RunSmoke) {
        $smoke = Invoke-SmokeTest -Context $Context
    }

    return [pscustomobject]@{
        Success      = ($stack.ABPReady -and $stack.HubReady -and ((-not $RunSmoke) -or $smoke.Success))
        ABPReady     = $stack.ABPReady
        HubReady     = $stack.HubReady
        SmokeSuccess = $smoke.Success
        SmokeMessage = $smoke.Message
        RotatedCount = $rotation.RemovedCount
    }
}
