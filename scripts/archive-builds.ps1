[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [Parameter(Mandatory = $true)]
    [string[]]$SourcePath,
    [ValidateRange(3, 20)]
    [int]$Keep = 3
)

$ErrorActionPreference = "Stop"
$repository = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$repositoryPrefix = $repository.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
$archiveRoot = [System.IO.Path]::GetFullPath((Join-Path $repository "dist/builds"))
$distRoot = [System.IO.Path]::GetFullPath((Join-Path $repository "dist"))
if (-not $archiveRoot.StartsWith($distRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Archive root escaped the AutoQuill dist directory."
}
if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$') {
    throw "Version must be a semantic version without a leading v."
}

$destination = [System.IO.Path]::GetFullPath((Join-Path $archiveRoot $Version))
if (-not $destination.StartsWith(($archiveRoot + [System.IO.Path]::DirectorySeparatorChar), [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Version destination escaped the build archive."
}
New-Item -ItemType Directory -Force -Path $destination | Out-Null

foreach ($source in $SourcePath) {
    $resolved = [System.IO.Path]::GetFullPath((Join-Path $repository $source))
    if (-not $resolved.StartsWith($repositoryPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Archive source escaped the AutoQuill repository: $source"
    }
    if (-not (Test-Path -LiteralPath $resolved)) {
        throw "Archive source does not exist: $resolved"
    }
    Copy-Item -LiteralPath $resolved -Destination $destination -Recurse -Force
}

[ordered]@{
    version = $Version
    archived_utc = [DateTime]::UtcNow.ToString("o")
    files = @(Get-ChildItem -File -Recurse -LiteralPath $destination | Where-Object Name -ne "archive-manifest.json" | ForEach-Object {
        [ordered]@{
            name = $_.Name
            bytes = $_.Length
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()
        }
    })
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $destination "archive-manifest.json") -Encoding utf8

$archives = @(Get-ChildItem -Directory -LiteralPath $archiveRoot | Sort-Object { [semver]$_.Name } -Descending)
foreach ($old in ($archives | Select-Object -Skip $Keep)) {
    $oldResolved = [System.IO.Path]::GetFullPath($old.FullName)
    if (-not $oldResolved.StartsWith(($archiveRoot + [System.IO.Path]::DirectorySeparatorChar), [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to remove an archive outside the validated archive root: $oldResolved"
    }
    Remove-Item -LiteralPath $oldResolved -Recurse -Force
}

Write-Output "Archived AutoQuill $Version. Preserved the newest $Keep build sets in $archiveRoot."
