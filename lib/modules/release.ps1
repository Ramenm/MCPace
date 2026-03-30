function Read-ReleaseManifest {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$RootPath
    )

    $manifestPath = Join-Path $RootPath 'release-manifest.json'
    if (-not (Test-Path -LiteralPath $manifestPath)) {
        throw "Missing file: $manifestPath"
    }

    return (Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json)
}

function Copy-ReleasePath {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$SourceRoot,
        [Parameter(Mandatory = $true)]
        [string]$StagingRoot,
        [Parameter(Mandatory = $true)]
        [string]$RelativePath
    )

    $sourcePath = Join-Path $SourceRoot $RelativePath
    if (-not (Test-Path -LiteralPath $sourcePath)) {
        throw "Release manifest path is missing: $RelativePath"
    }

    $destinationPath = Join-Path $StagingRoot $RelativePath
    if (Test-Path -LiteralPath $sourcePath -PathType Container) {
        New-Item -ItemType Directory -Force -Path $destinationPath | Out-Null
        Copy-Item -Path (Join-Path $sourcePath '*') -Destination $destinationPath -Recurse -Force
        return
    }

    $destinationDir = Split-Path -Parent $destinationPath
    if (-not [string]::IsNullOrWhiteSpace($destinationDir)) {
        New-Item -ItemType Directory -Force -Path $destinationDir | Out-Null
    }
    Copy-Item -LiteralPath $sourcePath -Destination $destinationPath -Force
}

function New-ReleaseBundle {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$RootPath,
        [Parameter(Mandatory = $true)]
        [string]$OutputDir
    )

    $manifest = Read-ReleaseManifest -RootPath $RootPath
    $config = Get-Content -LiteralPath (Join-Path $RootPath 'mcpace.config.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    $version = if (-not [string]::IsNullOrWhiteSpace([string]$config.version)) { [string]$config.version } else { (Get-Date -Format 'yyyyMMddHHmmss') }
    $bundleName = "mcpace-$version"
    $stagingRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("$bundleName-" + [System.Guid]::NewGuid().ToString('N'))
    $bundleRoot = Join-Path $stagingRoot $bundleName

    New-Item -ItemType Directory -Force -Path $OutputDir, $bundleRoot | Out-Null
    foreach ($relativePath in @($manifest.includePaths)) {
        Copy-ReleasePath -SourceRoot $RootPath -StagingRoot $bundleRoot -RelativePath ([string]$relativePath)
    }

    foreach ($relativePath in @($manifest.runtimeDirectories)) {
        $runtimePath = Join-Path $bundleRoot ([string]$relativePath)
        New-Item -ItemType Directory -Force -Path $runtimePath | Out-Null
        $keepPath = Join-Path $runtimePath '.gitkeep'
        Set-Content -LiteralPath $keepPath -Value '' -Encoding ASCII
    }

    $archivePath = Join-Path $OutputDir "$bundleName.zip"
    if (Test-Path -LiteralPath $archivePath) {
        Remove-Item -LiteralPath $archivePath -Force
    }

    Compress-Archive -Path (Join-Path $bundleRoot '*') -DestinationPath $archivePath -CompressionLevel Optimal

    return [pscustomobject]@{
        ArchivePath = $archivePath
        BundleRoot  = $bundleRoot
        Version     = $version
        Included    = @($manifest.includePaths)
        RuntimeDirs = @($manifest.runtimeDirectories)
    }
}
