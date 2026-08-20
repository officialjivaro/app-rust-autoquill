param(
    [ValidateSet("femtovg", "software")]
    [string]$Renderer = "software"
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$Cargo = Get-Command cargo -ErrorAction SilentlyContinue

if ($null -eq $Cargo) {
    $CargoPath = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
} else {
    $CargoPath = $Cargo.Source
}

if (-not (Test-Path -LiteralPath $CargoPath)) {
    throw "Cargo was not found. Install Rust through rustup before measuring a release."
}

$TargetDir = Join-Path $ProjectRoot "target\size-$Renderer"
$Feature = "renderer-$Renderer"

& $CargoPath build `
    --manifest-path (Join-Path $ProjectRoot "Cargo.toml") `
    --profile release-size `
    --no-default-features `
    --features $Feature `
    --target-dir $TargetDir

if ($LASTEXITCODE -ne 0) {
    throw "The $Renderer release build failed."
}

$Executable = Join-Path $TargetDir "release-size\autoquill.exe"
if (-not (Test-Path -LiteralPath $Executable)) {
    throw "Release executable was not produced at $Executable"
}

$File = Get-Item -LiteralPath $Executable
$PreviousSmokeValue = $env:AUTOQUILL_SMOKE_TEST
$env:AUTOQUILL_SMOKE_TEST = "1"
try {
    $Elapsed = Measure-Command {
        Start-Process -FilePath $Executable -WindowStyle Hidden -Wait
    }
} finally {
    $env:AUTOQUILL_SMOKE_TEST = $PreviousSmokeValue
}

[pscustomobject]@{
    Renderer = $Renderer
    Executable = $File.FullName
    Bytes = $File.Length
    MiB = [math]::Round($File.Length / 1MB, 2)
    SmokeTestMilliseconds = [math]::Round($Elapsed.TotalMilliseconds, 0)
}
