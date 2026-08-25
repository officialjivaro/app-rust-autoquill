[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repository = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$output = Join-Path $repository "DEPENDENCY_LICENSES.md"
$metadata = cargo metadata --locked --format-version 1 | ConvertFrom-Json
$packages = @($metadata.packages | Where-Object source | Sort-Object name, version)

$lines = [System.Collections.Generic.List[string]]::new()
$lines.Add("# Locked dependency license inventory")
$lines.Add("")
$lines.Add("Generated from ``cargo metadata --locked`` for AutoQuill's complete target-specific dependency graph.")
$lines.Add("This inventory records package-declared SPDX expressions; consult each package source for full terms.")
$lines.Add("")
$lines.Add("| Package | Version | Declared license |")
$lines.Add("|---|---:|---|")
foreach ($package in $packages) {
    $license = if ([string]::IsNullOrWhiteSpace($package.license)) { "Not declared" } else { $package.license }
    $safeName = $package.name.Replace("|", "\|")
    $safeLicense = $license.Replace("|", "\|")
    $lines.Add("| $safeName | $($package.version) | $safeLicense |")
}
$lines.Add("")
$lines.Add("Package count: $($packages.Count)")
$lines | Set-Content -LiteralPath $output -Encoding utf8
Write-Output "Wrote $output with $($packages.Count) packages."
