[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [string]$BinaryPath = "target/release-size/autoquill.exe",
    [string]$OutputDirectory = "artifacts/windows"
)

$ErrorActionPreference = "Stop"
$repository = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$repositoryPrefix = $repository.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
$binary = [System.IO.Path]::GetFullPath((Join-Path $repository $BinaryPath))
$output = [System.IO.Path]::GetFullPath((Join-Path $repository $OutputDirectory))
if (-not $binary.StartsWith($repositoryPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Binary path must stay inside the AutoQuill repository."
}
if (-not $output.StartsWith($repositoryPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Output directory must stay inside the AutoQuill repository."
}
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw "Release binary not found: $binary"
}
$userProfile = [Environment]::GetFolderPath("UserProfile")
if (-not [string]::IsNullOrWhiteSpace($userProfile)) {
    $binaryText = [System.Text.Encoding]::Latin1.GetString([System.IO.File]::ReadAllBytes($binary))
    $profileMarkers = @($userProfile, $userProfile.Replace('\', '/')) | Select-Object -Unique
    foreach ($marker in $profileMarkers) {
        if ($binaryText.Contains($marker)) {
            throw "The Windows release contains the local user profile path; rebuild it with path remapping."
        }
    }
}

New-Item -ItemType Directory -Force -Path $output | Out-Null
$artifactName = "AutoQuill-$Version-windows-x64.exe"
$artifact = Join-Path $output $artifactName
Copy-Item -LiteralPath $binary -Destination $artifact -Force
Copy-Item -LiteralPath (Join-Path $repository "THIRD_PARTY_NOTICES.md") -Destination $output -Force
Copy-Item -LiteralPath (Join-Path $repository "DEPENDENCY_LICENSES.md") -Destination $output -Force
$hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $artifact).Hash
$bytes = (Get-Item -LiteralPath $artifact).Length
"$hash  $artifactName" | Set-Content -LiteralPath (Join-Path $output "SHA256SUMS-windows.txt") -Encoding ascii
[ordered]@{
    version = $Version
    platform = "windows-x64"
    verification = "local-and-native-runner"
    signed = $false
    artifacts = @(
        [ordered]@{ name = $artifactName; bytes = $bytes; sha256 = $hash.ToLowerInvariant() }
    )
} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $output "manifest-windows.json") -Encoding utf8

Write-Output "Packaged $artifactName ($bytes bytes, SHA-256 $hash)"
