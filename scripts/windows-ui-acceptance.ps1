[CmdletBinding()]
param(
    [string]$Executable = "dist/AutoQuill-windows-x64.exe",
    [ValidateRange(10, 120)]
    [int]$StartupTimeoutSeconds = 30
)

$ErrorActionPreference = "Stop"
$repository = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$repositoryPrefix = $repository.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
$executablePath = [System.IO.Path]::GetFullPath((Join-Path $repository $Executable))
if (-not $executablePath.StartsWith($repositoryPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Executable must stay inside the AutoQuill repository."
}
if (-not (Test-Path -LiteralPath $executablePath -PathType Leaf)) {
    throw "AutoQuill executable not found: $executablePath"
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms

function Find-NamedElement {
    param(
        [System.Windows.Automation.AutomationElement]$Root,
        [string]$Name,
        [System.Windows.Automation.ControlType]$ControlType
    )

    $nameCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::NameProperty,
        $Name
    )
    $condition = $nameCondition
    if ($null -ne $ControlType) {
        $typeCondition = [System.Windows.Automation.PropertyCondition]::new(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            $ControlType
        )
        $condition = [System.Windows.Automation.AndCondition]::new($nameCondition, $typeCondition)
    }
    return $Root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
}

function Wait-NamedElement {
    param(
        [System.Windows.Automation.AutomationElement]$Root,
        [string]$Name,
        [System.Windows.Automation.ControlType]$ControlType,
        [int]$TimeoutMilliseconds = 5000
    )

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    do {
        $element = Find-NamedElement -Root $Root -Name $Name -ControlType $ControlType
        if ($null -ne $element) {
            return $element
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UI Automation element was not found: $Name"
}

function Invoke-Element {
    param([System.Windows.Automation.AutomationElement]$Element)
    $pattern = $Element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
    ([System.Windows.Automation.InvokePattern]$pattern).Invoke()
}

function Set-ElementValue {
    param(
        [System.Windows.Automation.AutomationElement]$Element,
        [string]$Value
    )
    $pattern = $Element.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
    ([System.Windows.Automation.ValuePattern]$pattern).SetValue($Value)
}

function Find-HeaderProfileButton {
    param([System.Windows.Automation.AutomationElement]$Root)
    $buttons = $Root.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::Button
        )
    )
    $windowTop = $Root.Current.BoundingRectangle.Top
    foreach ($button in $buttons) {
        $bounds = $button.Current.BoundingRectangle
        if ($bounds.Top -lt ($windowTop + 110) -and $bounds.Width -ge 120 -and $button.Current.Name -ne "") {
            return $button
        }
    }
    throw "The profile button was not found in the AutoQuill header."
}

$process = Start-Process -FilePath $executablePath -PassThru
try {
    $deadline = [DateTime]::UtcNow.AddSeconds($StartupTimeoutSeconds)
    do {
        $process.Refresh()
        if ($process.MainWindowHandle -ne 0) {
            break
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($process.MainWindowHandle -eq 0) {
        throw "AutoQuill did not expose a main window within $StartupTimeoutSeconds seconds."
    }

    # AccessKit attaches its provider immediately after the native window exists. Reacquire the
    # root after that short initialization window instead of holding a pre-provider UIA element.
    Start-Sleep -Seconds 2
    $process.Refresh()
    $window = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $editor = Wait-NamedElement -Root $window -Name "Text to preview" -ControlType ([System.Windows.Automation.ControlType]::Edit)
    Set-ElementValue -Element $editor -Value "AutoQuill UI acceptance text"
    Start-Sleep -Milliseconds 200
    $editor = Wait-NamedElement -Root $window -Name "Text to preview" -ControlType ([System.Windows.Automation.ControlType]::Edit)
    if (([System.Windows.Automation.ValuePattern]$editor.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)).Current.Value -ne "AutoQuill UI acceptance text") {
        throw "The editor ValuePattern did not accept text."
    }
    [void](Wait-NamedElement -Root $window -Name "Words per minute" -ControlType ([System.Windows.Automation.ControlType]::Spinner))

    $profileButton = Find-HeaderProfileButton -Root $window
    $settings = Wait-NamedElement -Root $window -Name "SETTINGS" -ControlType ([System.Windows.Automation.ControlType]::Button)
    Invoke-Element -Element $settings
    $done = Wait-NamedElement -Root $window -Name "DONE" -ControlType ([System.Windows.Automation.ControlType]::Button)
    if (-not $done.Current.HasKeyboardFocus) {
        throw "The Settings drawer did not move focus to its first Done action."
    }
    if ($profileButton.Current.IsEnabled) {
        throw "The dimmed main workspace remained enabled while Settings was modal."
    }
    [void](Wait-NamedElement -Root $window -Name "Active seconds" -ControlType ([System.Windows.Automation.ControlType]::Spinner))
    Invoke-Element -Element (Wait-NamedElement -Root $window -Name "Appearance and support" -ControlType ([System.Windows.Automation.ControlType]::Button))
    Start-Sleep -Milliseconds 150
    foreach ($name in @("Wait seconds minimum", "Wait seconds maximum")) {
        [void](Wait-NamedElement -Root $window -Name $name -ControlType ([System.Windows.Automation.ControlType]::Spinner))
    }
    Invoke-Element -Element (Wait-NamedElement -Root $window -Name "Timing and session settings" -ControlType ([System.Windows.Automation.ControlType]::Button))
    Start-Sleep -Milliseconds 150
    foreach ($name in @("Pause ms minimum", "Pause ms maximum")) {
        [void](Wait-NamedElement -Root $window -Name $name -ControlType ([System.Windows.Automation.ControlType]::Spinner))
    }
    foreach ($iteration in 1..36) {
        [System.Windows.Forms.SendKeys]::SendWait("{TAB}")
        Start-Sleep -Milliseconds 20
        $focused = [System.Windows.Automation.AutomationElement]::FocusedElement
        if ($null -ne $focused -and $focused.Current.Name -in @("Text to preview", "Simulation preview", "SETTINGS", $profileButton.Current.Name)) {
            throw "Keyboard focus escaped from the Settings drawer to '$($focused.Current.Name)'."
        }
    }
    Invoke-Element -Element (Wait-NamedElement -Root $window -Name "DONE" -ControlType ([System.Windows.Automation.ControlType]::Button))

    $profileButton = Find-HeaderProfileButton -Root $window
    Invoke-Element -Element $profileButton
    $search = Wait-NamedElement -Root $window -Name "Search profiles" -ControlType ([System.Windows.Automation.ControlType]::Edit)
    if (-not $search.Current.HasKeyboardFocus) {
        throw "The Profiles dialog did not move focus to Search profiles."
    }
    Set-ElementValue -Element $search -Value "__autoquill_019_no_match__"
    [void](Wait-NamedElement -Root $window -Name "NO MATCHING PROFILES" -ControlType $null)
    Set-ElementValue -Element $search -Value ""

    Write-Output "PASS: AutoQuill Windows UI acceptance checks completed successfully."
} finally {
    if (-not $process.HasExited) {
        Stop-Process -Id $process.Id -Force
        $process.WaitForExit()
    }
}
