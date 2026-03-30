function Test-SettingsTreeHasProperty {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $false)]
        $Value,
        [Parameter(Mandatory = $true)]
        [string]$PropertyName
    )

    if ($null -eq $Value) {
        return $false
    }

    if ($Value -is [System.Collections.IDictionary]) {
        foreach ($key in $Value.Keys) {
            if ([string]$key -eq $PropertyName) {
                return $true
            }
            if (Test-SettingsTreeHasProperty -Value $Value[$key] -PropertyName $PropertyName) {
                return $true
            }
        }
        return $false
    }

    if ($Value -is [pscustomobject]) {
        foreach ($prop in @($Value.PSObject.Properties | Where-Object { $_.MemberType -in @('NoteProperty', 'Property') })) {
            if ([string]$prop.Name -eq $PropertyName) {
                return $true
            }
            if (Test-SettingsTreeHasProperty -Value $prop.Value -PropertyName $PropertyName) {
                return $true
            }
        }
        return $false
    }

    if ($Value -is [System.Collections.IEnumerable] -and -not ($Value -is [string])) {
        foreach ($item in $Value) {
            if (Test-SettingsTreeHasProperty -Value $item -PropertyName $PropertyName) {
                return $true
            }
        }
    }

    return $false
}

function Get-SourceSettingsPolicyViolations {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Config,
        [Parameter(Mandatory = $true)]$SettingsRaw,
        [Parameter(Mandatory = $false)]
        [switch]$EnforceOptionalDefaults
    )

    $violations = @()

    if (Test-SettingsTreeHasProperty -Value $SettingsRaw -PropertyName 'pendingAuthorization') {
        $violations += 'mcp_settings.json must not contain pendingAuthorization runtime state.'
    }

    $allKeys = @($SettingsRaw.bearerKeys)
    if ($allKeys.Count -eq 0) {
        $violations += 'mcp_settings.json must define at least one bearer key.'
    }
    else {
        $index = 0
        foreach ($key in $allKeys) {
            $tokenValue = [string]$key.token
            if ($tokenValue -ne '${MCPACE_BEARER_TOKEN}') {
                $violations += ("bearerKeys[{0}].token must be `${MCPACE_BEARER_TOKEN}` in source config." -f $index)
            }
            $index += 1
        }
    }

    $adminUser = @($SettingsRaw.users | Where-Object { [bool]$_.isAdmin }) | Select-Object -First 1
    if (-not $adminUser) {
        $violations += 'mcp_settings.json must declare an admin user.'
    }
    elseif ([string]$adminUser.password -ne '${MCPACE_ADMIN_PASSWORD_BCRYPT}') {
        $violations += 'admin user password must be `${MCPACE_ADMIN_PASSWORD_BCRYPT}` in source config.'
    }

    if ($EnforceOptionalDefaults -and $Config -and $Config.servers -and $SettingsRaw -and $SettingsRaw.mcpServers) {
        foreach ($prop in @($Config.servers.PSObject.Properties)) {
            if ([bool]$prop.Value.required) {
                continue
            }

            $name = [string]$prop.Name
            $serverProperty = @($SettingsRaw.mcpServers.PSObject.Properties | Where-Object { [string]$_.Name -eq $name } | Select-Object -First 1)
            if (-not $serverProperty) {
                continue
            }

            $expectedEnabled = $false
            if ($prop.Value.PSObject.Properties.Name -contains 'defaultEnabled') {
                $expectedEnabled = [bool]$prop.Value.defaultEnabled
            }

            $actualEnabled = $false
            if ($null -ne $serverProperty[0].Value.enabled) {
                $actualEnabled = [bool]$serverProperty[0].Value.enabled
            }

            if ($actualEnabled -ne $expectedEnabled) {
                $expectedLabel = if ($expectedEnabled) { 'enabled' } else { 'disabled' }
                $violations += ("optional server '{0}' must be {1} by default in source config." -f $name, $expectedLabel)
            }
        }
    }

    return $violations
}

function Assert-SourceSettingsPolicy {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Config,
        [Parameter(Mandatory = $true)]$SettingsRaw,
        [Parameter(Mandatory = $false)]
        [string]$SettingsPath = 'mcp_settings.json',
        [Parameter(Mandatory = $false)]
        [switch]$EnforceOptionalDefaults
    )

    $violations = @(Get-SourceSettingsPolicyViolations -Config $Config -SettingsRaw $SettingsRaw -EnforceOptionalDefaults:$EnforceOptionalDefaults)
    if ($violations.Count -eq 0) {
        return
    }

    throw ("{0} violates source policy:`n- {1}" -f $SettingsPath, ($violations -join "`n- "))
}

function Assert-ResolvedSettingsSecrets {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Settings
    )

    $hasBearerToken = $false
    foreach ($key in @($Settings.bearerKeys)) {
        if (-not [string]::IsNullOrWhiteSpace([string]$key.token)) {
            $hasBearerToken = $true
            break
        }
    }

    if (-not $hasBearerToken) {
        throw 'MCPACE_BEARER_TOKEN is required. Set it before running the launcher or checks.'
    }

    $adminUser = @($Settings.users | Where-Object { [bool]$_.isAdmin }) | Select-Object -First 1
    if (-not $adminUser -or [string]::IsNullOrWhiteSpace([string]$adminUser.password)) {
        throw 'MCPACE_ADMIN_PASSWORD_BCRYPT is required. Set it before running the launcher or checks.'
    }
}
