[CmdletBinding()]
param(
    [string]$ArchiveRoot = "dist/builds",
    [string]$MetadataPath = "data/release-notes.json",
    [string]$OutputPath = "data/releases.json",
    [ValidateRange(1, 3)]
    [int]$Keep = 3,
    [string]$Repository = "officialjivaro/app-rust-autoquill"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$archiveDirectory = [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot $ArchiveRoot))
$metadataFile = [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot $MetadataPath))
$outputFile = [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot $OutputPath))
$repositoryPrefix = $repositoryRoot + [System.IO.Path]::DirectorySeparatorChar

foreach ($candidate in @($archiveDirectory, $metadataFile, $outputFile)) {
    if (-not $candidate.StartsWith($repositoryPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Release catalog path escaped the AutoQuill repository: $candidate"
    }
}
if (-not (Test-Path -LiteralPath $archiveDirectory -PathType Container)) {
    throw "AutoQuill build archive was not found: $archiveDirectory"
}
if (-not (Test-Path -LiteralPath $metadataFile -PathType Leaf)) {
    throw "AutoQuill release metadata was not found: $metadataFile"
}

$metadata = Get-Content -LiteralPath $metadataFile -Raw | ConvertFrom-Json
if ($metadata.schemaVersion -ne 1) {
    throw "Unsupported release metadata schema: $($metadata.schemaVersion)"
}

function Get-ReleaseMetadata([string]$Version) {
    $property = $metadata.versions.PSObject.Properties[$Version]
    if ($null -eq $property) {
        throw "Release metadata is missing for AutoQuill $Version."
    }
    return $property.Value
}

function Get-Channel([string]$Version) {
    if ($Version -match '-([A-Za-z]+)') { return $Matches[1].ToLowerInvariant() }
    return "stable"
}

function Get-Format([string]$Name) {
    if ($Name.EndsWith('.AppImage', [System.StringComparison]::OrdinalIgnoreCase)) { return "AppImage" }
    if ($Name.EndsWith('.tar.gz', [System.StringComparison]::OrdinalIgnoreCase)) { return "tar.gz" }
    if ($Name.EndsWith('.exe', [System.StringComparison]::OrdinalIgnoreCase)) { return "EXE" }
    if ($Name.EndsWith('.dmg', [System.StringComparison]::OrdinalIgnoreCase)) { return "DMG" }
    if ($Name.EndsWith('.zip', [System.StringComparison]::OrdinalIgnoreCase)) { return "ZIP" }
    return "binary"
}

function New-PublicArtifact([string]$Version, $Artifact, [string]$LocalPath, [string]$PublicName = "") {
    $name = if ([string]::IsNullOrWhiteSpace($PublicName)) { [string]$Artifact.name } else { $PublicName }
    $resolved = [System.IO.Path]::GetFullPath($LocalPath)
    if (-not $resolved.StartsWith(($archiveDirectory + [System.IO.Path]::DirectorySeparatorChar), [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Artifact escaped the AutoQuill build archive: $resolved"
    }
    if (-not (Test-Path -LiteralPath $resolved -PathType Leaf)) {
        throw "Recorded artifact is missing: $resolved"
    }
    $file = Get-Item -LiteralPath $resolved
    $hash = (Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash.ToLowerInvariant()
    if ([int64]$Artifact.bytes -ne $file.Length -or [string]$Artifact.sha256 -ne $hash) {
        throw "Artifact integrity metadata disagrees with $resolved."
    }
    return [ordered]@{
        name = $name
        format = Get-Format $name
        bytes = [int64]$Artifact.bytes
        sha256 = $hash
        url = "https://github.com/$Repository/releases/download/v$Version/$name"
    }
}

function Read-Platform([string]$Version, [string]$ReleaseDirectory, [string]$Id, [string]$Name, [string]$Architecture, [string]$ManifestName) {
    $manifestFile = Get-ChildItem -LiteralPath $ReleaseDirectory -Recurse -File -Filter $ManifestName | Select-Object -First 1
    if ($null -eq $manifestFile) {
        return [ordered]@{ id = $Id; name = $Name; architecture = $Architecture; status = "coming-soon"; artifacts = @() }
    }
    $manifest = Get-Content -LiteralPath $manifestFile.FullName -Raw | ConvertFrom-Json
    if ([string]$manifest.version -ne $Version) {
        throw "$($manifestFile.FullName) records version $($manifest.version), expected $Version."
    }
    $artifacts = @($manifest.artifacts | ForEach-Object {
        New-PublicArtifact $Version $_ (Join-Path $manifestFile.Directory.FullName $_.name)
    })
    $platform = [ordered]@{
        id = $Id
        name = $Name
        architecture = $Architecture
        status = "available"
        verification = [string]$manifest.verification
        artifacts = $artifacts
    }
    if ($manifest.PSObject.Properties['signed']) { $platform.signed = [bool]$manifest.signed }
    if ($manifest.PSObject.Properties['wayland']) { $platform.wayland = [string]$manifest.wayland }
    return $platform
}

$directories = @(Get-ChildItem -LiteralPath $archiveDirectory -Directory | ForEach-Object {
    try { [pscustomobject]@{ Directory = $_; Version = [semver]$_.Name } } catch { $null }
} | Where-Object { $null -ne $_ } | Sort-Object Version -Descending | Select-Object -First $Keep)

if ($directories.Count -eq 0) {
    throw "No semantic-version AutoQuill build archives were found."
}

$releases = foreach ($entry in $directories) {
    $version = $entry.Directory.Name
    $releaseMetadata = Get-ReleaseMetadata $version
    $archiveManifestFile = Join-Path $entry.Directory.FullName "archive-manifest.json"
    if (-not (Test-Path -LiteralPath $archiveManifestFile -PathType Leaf)) {
        throw "Archive manifest is missing for AutoQuill $version."
    }
    $archiveManifest = Get-Content -LiteralPath $archiveManifestFile -Raw | ConvertFrom-Json
    if ([string]$archiveManifest.version -ne $version) {
        throw "$archiveManifestFile records version $($archiveManifest.version), expected $version."
    }
    $archivedUtc = if ($archiveManifest.archived_utc -is [datetime]) {
        $archiveManifest.archived_utc.ToUniversalTime().ToString("o")
    } else {
        [string]$archiveManifest.archived_utc
    }

    $windows = Read-Platform $version $entry.Directory.FullName "windows-x64" "Windows" "x64" "manifest-windows.json"
    if ($windows.status -eq "coming-soon") {
        $legacy = @($archiveManifest.files | Where-Object { $_.name -eq "AutoQuill-windows-x64.exe" }) | Select-Object -First 1
        if ($null -ne $legacy) {
            $windows = [ordered]@{
                id = "windows-x64"
                name = "Windows"
                architecture = "x64"
                status = "available"
                verification = "local-windows-release"
                signed = $false
                artifacts = @(
                    New-PublicArtifact $version $legacy (Join-Path $entry.Directory.FullName $legacy.name) "AutoQuill-$version-windows-x64.exe"
                )
            }
        }
    }

    [ordered]@{
        version = $version
        channel = Get-Channel $version
        date = [string]$releaseMetadata.date
        title = [string]$releaseMetadata.title
        summary = [string]$releaseMetadata.summary
        changes = @($releaseMetadata.changes)
        limitations = @($releaseMetadata.limitations)
        tag = "v$version"
        releaseUrl = "https://github.com/$Repository/releases/tag/v$version"
        archivedUtc = $archivedUtc
        platforms = @(
            $windows
            Read-Platform $version $entry.Directory.FullName "linux-x64" "Linux" "x64" "manifest-linux.json"
            Read-Platform $version $entry.Directory.FullName "macos-universal2" "macOS" "Universal 2" "manifest-macos.json"
        )
    }
}

$catalog = [ordered]@{
    schemaVersion = 1
    product = [ordered]@{
        id = "autoquill"
        name = "AutoQuill"
        repository = $Repository
    }
    policy = [ordered]@{
        maximumVersions = $Keep
        includePrereleases = $true
        sort = "semantic-version-descending"
    }
    generatedFrom = "dist/builds"
    updated = ($releases | Select-Object -First 1).date
    releases = @($releases)
}

$parent = Split-Path -Parent $outputFile
New-Item -ItemType Directory -Path $parent -Force | Out-Null
$json = $catalog | ConvertTo-Json -Depth 12
[System.IO.File]::WriteAllText($outputFile, ($json + [Environment]::NewLine), [System.Text.UTF8Encoding]::new($false))
Write-Output "Wrote the newest $($releases.Count) AutoQuill releases to $outputFile."
