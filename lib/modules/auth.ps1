function Resolve-ManagerStateRootPath {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$RootPath,
        [Parameter(Mandatory = $false)]
        [AllowEmptyString()]
        [string]$StateRoot = ''
    )

    $candidate = [string]$StateRoot
    if ([string]::IsNullOrWhiteSpace($candidate)) {
        $candidate = [string][Environment]::GetEnvironmentVariable('MCPACE_STATE_ROOT')
    }

    if ([string]::IsNullOrWhiteSpace($candidate)) {
        return ([System.IO.Path]::GetFullPath($RootPath))
    }

    if (-not [System.IO.Path]::IsPathRooted($candidate)) {
        $candidate = Join-Path $RootPath $candidate
    }

    return ([System.IO.Path]::GetFullPath($candidate))
}

function Get-LocalAuthStateModel {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Raw
    )

    $createdAt = ''
    $updatedAt = ''
    $bearerToken = ''
    $adminUsername = 'admin'
    $adminPassword = ''
    $adminPasswordBcrypt = ''
    $bearerTokenSeededFromEnv = $false
    $adminPasswordSeededFromEnv = $false

    if ($Raw) {
        if (-not [string]::IsNullOrWhiteSpace([string]$Raw.createdAt)) {
            $createdAt = [string]$Raw.createdAt
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$Raw.updatedAt)) {
            $updatedAt = [string]$Raw.updatedAt
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$Raw.bearerToken)) {
            $bearerToken = [string]$Raw.bearerToken
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$Raw.adminUsername)) {
            $adminUsername = [string]$Raw.adminUsername
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$Raw.adminPassword)) {
            $adminPassword = [string]$Raw.adminPassword
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$Raw.adminPasswordBcrypt)) {
            $adminPasswordBcrypt = [string]$Raw.adminPasswordBcrypt
        }
        if ($null -ne $Raw.bearerTokenSeededFromEnv) {
            $bearerTokenSeededFromEnv = [bool]$Raw.bearerTokenSeededFromEnv
        }
        if ($null -ne $Raw.adminPasswordSeededFromEnv) {
            $adminPasswordSeededFromEnv = [bool]$Raw.adminPasswordSeededFromEnv
        }
    }

    return [pscustomobject]@{
        version                    = 1
        createdAt                  = $createdAt
        updatedAt                  = $updatedAt
        bearerToken                = $bearerToken
        adminUsername              = $adminUsername
        adminPassword              = $adminPassword
        adminPasswordBcrypt        = $adminPasswordBcrypt
        bearerTokenSeededFromEnv   = $bearerTokenSeededFromEnv
        adminPasswordSeededFromEnv = $adminPasswordSeededFromEnv
    }
}

function Read-LocalAuthState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return (Get-LocalAuthStateModel)
    }

    try {
        $raw = [System.IO.File]::ReadAllText($Path, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
        return (Get-LocalAuthStateModel -Raw $raw)
    }
    catch {
        return (Get-LocalAuthStateModel)
    }
}

function Write-LocalAuthState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        $State
    )

    $directory = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($directory)) {
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
    }

    $json = $State | ConvertTo-Json -Depth 10
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    for ($attempt = 1; $attempt -le 20; $attempt++) {
        try {
            [System.IO.File]::WriteAllText($Path, $json, $utf8NoBom)
            return
        }
        catch {
            if ($attempt -eq 20) {
                throw
            }
            Start-Sleep -Milliseconds 200
        }
    }
}

function New-LocalBearerToken {
    [CmdletBinding()]
    param()

    $bytes = New-Object byte[] 24
    [System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
    $token = [Convert]::ToBase64String($bytes).TrimEnd('=').Replace('+', '-').Replace('/', '_')
    return $token
}

function New-LocalAdminPassword {
    [CmdletBinding()]
    param()

    $bytes = New-Object byte[] 18
    [System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
    $suffix = [Convert]::ToBase64String($bytes).TrimEnd('=').Replace('+', 'A').Replace('/', 'B')
    return ('mcpace-{0}' -f $suffix)
}

function Get-NodeCommandPathForAuth {
    [CmdletBinding()]
    param()

    $command = Get-Command node -ErrorAction SilentlyContinue
    if ($command) {
        if (-not [string]::IsNullOrWhiteSpace([string]$command.Source)) {
            return [string]$command.Source
        }
        return [string]$command.Name
    }

    return $null
}

function Get-NpmCommandPathForAuth {
    [CmdletBinding()]
    param()

    foreach ($name in @('npm.cmd', 'npm.exe', 'npm')) {
        $command = Get-Command $name -ErrorAction SilentlyContinue
        if ($command) {
            if (-not [string]::IsNullOrWhiteSpace([string]$command.Source)) {
                return [string]$command.Source
            }
            return [string]$command.Name
        }
    }

    return $null
}

function Get-BcryptHelperScriptPath {
    [CmdletBinding()]
    param()

    return (Join-Path (Join-Path (Split-Path $PSScriptRoot -Parent) 'helpers') 'generate-bcrypt.js')
}

function Ensure-BcryptJsPackage {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$ToolRoot
    )

    $entryPath = Join-Path $ToolRoot 'node_modules\bcryptjs\index.js'
    if (Test-Path -LiteralPath $entryPath -PathType Leaf) {
        return $entryPath
    }

    $npmPath = Get-NpmCommandPathForAuth
    if ([string]::IsNullOrWhiteSpace($npmPath)) {
        throw 'npm is required to bootstrap local admin credentials. Install Node.js/npm or set MCPACE_ADMIN_PASSWORD_BCRYPT manually.'
    }

    New-Item -ItemType Directory -Force -Path $ToolRoot | Out-Null
    $installOutput = & $npmPath install --no-save --prefix $ToolRoot bcryptjs@2.4.3 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw ("Failed to install bcryptjs for local auth bootstrap. npm error: {0}" -f (($installOutput | Out-String).Trim()))
    }

    if (-not (Test-Path -LiteralPath $entryPath -PathType Leaf)) {
        throw 'bcryptjs install completed but the package entrypoint is missing.'
    }

    return $entryPath
}

function Get-BcryptHashForPassword {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyString()]
        [string]$Password,
        [Parameter(Mandatory = $true)]
        [string]$ServerStateDir
    )

    $nodePath = Get-NodeCommandPathForAuth
    if ([string]::IsNullOrWhiteSpace($nodePath)) {
        throw 'Node.js is required to bootstrap local admin credentials.'
    }

    $helperScript = Get-BcryptHelperScriptPath
    if (-not (Test-Path -LiteralPath $helperScript -PathType Leaf)) {
        throw ("Missing bcrypt helper script: {0}" -f $helperScript)
    }

    $toolRoot = Join-Path $ServerStateDir 'bcryptjs-tool'
    $packageEntry = Ensure-BcryptJsPackage -ToolRoot $toolRoot
    $output = & $nodePath $helperScript $packageEntry $Password 12 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw ("Failed to compute bcrypt hash for local admin password. node error: {0}" -f (($output | Out-String).Trim()))
    }

    $hash = (($output | Out-String).Trim())
    if ([string]::IsNullOrWhiteSpace($hash)) {
        throw 'bcrypt helper returned an empty hash.'
    }

    return $hash
}

function Reset-LocalAuthState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    Remove-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
}

function Resolve-LocalAuthMaterial {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        [string]$ServerStateDir,
        [Parameter(Mandatory = $false)]
        [switch]$ForceReset
    )

    $existing = if ($ForceReset) { Get-LocalAuthStateModel } else { Read-LocalAuthState -Path $Path }
    $stateExists = (Test-Path -LiteralPath $Path -PathType Leaf)
    $now = (Get-Date).ToString('o')
    $envBearer = [string][Environment]::GetEnvironmentVariable('MCPACE_BEARER_TOKEN')
    $envBcrypt = [string][Environment]::GetEnvironmentVariable('MCPACE_ADMIN_PASSWORD_BCRYPT')

    $adminUsername = if ([string]::IsNullOrWhiteSpace([string]$existing.adminUsername)) { 'admin' } else { [string]$existing.adminUsername }

    $bearerToken = ''
    $bearerTokenSource = ''
    if (-not [string]::IsNullOrWhiteSpace($envBearer)) {
        $bearerToken = $envBearer
        $bearerTokenSource = 'env'
    }
    elseif (-not [string]::IsNullOrWhiteSpace([string]$existing.bearerToken)) {
        $bearerToken = [string]$existing.bearerToken
        $bearerTokenSource = 'local-state'
    }
    else {
        $bearerToken = New-LocalBearerToken
        $bearerTokenSource = 'bootstrap'
    }

    $adminPasswordBcrypt = ''
    $adminPassword = ''
    $adminPasswordKnown = $false
    $adminPasswordSource = ''
    if (-not [string]::IsNullOrWhiteSpace($envBcrypt)) {
        $adminPasswordBcrypt = $envBcrypt
        $adminPasswordSource = 'env'
        if (
            -not [string]::IsNullOrWhiteSpace([string]$existing.adminPassword) -and
            [string]$existing.adminPasswordBcrypt -eq $envBcrypt
        ) {
            $adminPassword = [string]$existing.adminPassword
            $adminPasswordKnown = $true
        }
    }
    elseif (-not [string]::IsNullOrWhiteSpace([string]$existing.adminPasswordBcrypt)) {
        $adminPasswordBcrypt = [string]$existing.adminPasswordBcrypt
        $adminPassword = [string]$existing.adminPassword
        $adminPasswordKnown = -not [string]::IsNullOrWhiteSpace($adminPassword)
        $adminPasswordSource = 'local-state'
    }
    else {
        $adminPassword = if ([string]::IsNullOrWhiteSpace([string]$existing.adminPassword)) { New-LocalAdminPassword } else { [string]$existing.adminPassword }
        $adminPasswordBcrypt = Get-BcryptHashForPassword -Password $adminPassword -ServerStateDir $ServerStateDir
        $adminPasswordKnown = $true
        $adminPasswordSource = 'bootstrap'
    }

    $createdAt = if ($ForceReset -or [string]::IsNullOrWhiteSpace([string]$existing.createdAt)) { $now } else { [string]$existing.createdAt }
    $persistedState = [pscustomobject]@{
        version                    = 1
        createdAt                  = $createdAt
        updatedAt                  = $now
        bearerToken                = $bearerToken
        adminUsername              = $adminUsername
        adminPassword              = if ($adminPasswordKnown) { $adminPassword } else { '' }
        adminPasswordBcrypt        = $adminPasswordBcrypt
        bearerTokenSeededFromEnv   = ($bearerTokenSource -eq 'env')
        adminPasswordSeededFromEnv = ($adminPasswordSource -eq 'env')
    }

    $shouldWriteState = (
        $ForceReset -or
        -not $stateExists -or
        [string]$existing.bearerToken -ne [string]$persistedState.bearerToken -or
        [string]$existing.adminUsername -ne [string]$persistedState.adminUsername -or
        [string]$existing.adminPassword -ne [string]$persistedState.adminPassword -or
        [string]$existing.adminPasswordBcrypt -ne [string]$persistedState.adminPasswordBcrypt -or
        [bool]$existing.bearerTokenSeededFromEnv -ne [bool]$persistedState.bearerTokenSeededFromEnv -or
        [bool]$existing.adminPasswordSeededFromEnv -ne [bool]$persistedState.adminPasswordSeededFromEnv
    )
    if ($shouldWriteState) {
        Write-LocalAuthState -Path $Path -State $persistedState
    }

    return [pscustomobject]@{
        AuthStatePath        = $Path
        StateExists          = (Test-Path -LiteralPath $Path -PathType Leaf)
        BearerToken          = $bearerToken
        BearerTokenSource    = $bearerTokenSource
        AdminUsername        = $adminUsername
        AdminPassword        = $adminPassword
        AdminPasswordKnown   = $adminPasswordKnown
        AdminPasswordBcrypt  = $adminPasswordBcrypt
        AdminPasswordSource  = $adminPasswordSource
        CreatedAt            = $createdAt
        UpdatedAt            = $now
    }
}

function Apply-ResolvedAuthMaterialToSettings {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        $Settings,
        [Parameter(Mandatory = $true)]
        $AuthMaterial,
        [Parameter(Mandatory = $false)]
        [AllowEmptyString()]
        [string]$ClientKeyName = ''
    )

    $keys = @($Settings.bearerKeys)
    if ($keys.Count -eq 0) {
        return $Settings
    }

    $selected = $null
    foreach ($key in $keys) {
        if (-not [string]::IsNullOrWhiteSpace($ClientKeyName) -and [string]$key.name -eq $ClientKeyName) {
            $selected = $key
            break
        }
    }
    if (-not $selected) {
        $selected = $keys[0]
    }
    $selected.token = [string]$AuthMaterial.BearerToken

    $adminUser = @($Settings.users | Where-Object { [bool]$_.isAdmin } | Select-Object -First 1)
    if ($adminUser) {
        $adminUser[0].username = [string]$AuthMaterial.AdminUsername
        $adminUser[0].password = [string]$AuthMaterial.AdminPasswordBcrypt
    }

    return $Settings
}
