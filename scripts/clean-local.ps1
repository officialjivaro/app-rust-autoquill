param(
    [switch]$SkipExecutableBackup
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$TargetDirectory = Join-Path $ProjectRoot "target"
$ReleaseExecutable = Join-Path $TargetDirectory "release-size\autoquill.exe"
$LocalBuildDirectory = Join-Path $ProjectRoot "local-builds"
$LocalExecutable = Join-Path $LocalBuildDirectory "AutoQuill-windows-x64.exe"

$ResolvedProject = [System.IO.Path]::GetFullPath($ProjectRoot)
$ResolvedTarget = [System.IO.Path]::GetFullPath($TargetDirectory)
if (-not $ResolvedTarget.StartsWith($ResolvedProject + [System.IO.Path]::DirectorySeparatorChar)) {
    throw "Refusing to clean a target directory outside the AutoQuill project."
}

if (-not $SkipExecutableBackup -and (Test-Path -LiteralPath $ReleaseExecutable)) {
    New-Item -ItemType Directory -Path $LocalBuildDirectory -Force | Out-Null
    Copy-Item -LiteralPath $ReleaseExecutable -Destination $LocalExecutable -Force
    $SourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $ReleaseExecutable).Hash
    $BackupHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $LocalExecutable).Hash
    if ($SourceHash -ne $BackupHash) {
        throw "The preserved executable failed its SHA-256 verification."
    }
    Write-Host "Preserved $LocalExecutable"
    Write-Host "SHA-256 $BackupHash"
}

$Cargo = Get-Command cargo -ErrorAction SilentlyContinue
if ($null -eq $Cargo) {
    $CargoPath = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
} else {
    $CargoPath = $Cargo.Source
}
if (-not (Test-Path -LiteralPath $CargoPath)) {
    throw "Cargo was not found."
}

& $CargoPath clean --manifest-path (Join-Path $ProjectRoot "Cargo.toml")
if ($LASTEXITCODE -ne 0) {
    throw "Cargo clean failed."
}
