param(
    [switch]$WithIntegration,
    [string]$SummaryPath,
    [string]$IdeE2EReportPath,
    [switch]$RequireRealIdeReport,
    [int]$RealIdeReportMaxAgeHours = 72
)

$ErrorActionPreference = "Stop"

$scriptRoot = $PSScriptRoot
$runtimeRoot = (Resolve-Path (Join-Path $scriptRoot "..")).Path
$repoRoot = (Resolve-Path (Join-Path $runtimeRoot "..")).Path

if ([string]::IsNullOrWhiteSpace($SummaryPath)) {
    $SummaryPath = Join-Path $runtimeRoot ".cabal_runtime\release_gate_summary.json"
}

$summaryDir = Split-Path -Parent $SummaryPath
if (-not [string]::IsNullOrWhiteSpace($summaryDir)) {
    New-Item -ItemType Directory -Force -Path $summaryDir | Out-Null
}

$steps = @()
function Add-StepResult {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [ValidateSet("PASS", "FAIL")]
        [string]$Status,
        [Parameter(Mandatory = $true)]
        [string]$Command,
        [string]$Message
    )

    $entry = [ordered]@{
        name = $Name
        status = $Status
        command = $Command
    }

    if (-not [string]::IsNullOrWhiteSpace($Message)) {
        $entry.message = $Message
    }

    $script:steps += [pscustomobject]$entry
}

$failureMessage = $null
$resolvedIdeReportPath = $null
$ideReportSource = "unknown"
Write-Host "[cabal] running unified release gates..." -ForegroundColor Cyan

try {
    if ([string]::IsNullOrWhiteSpace($IdeE2EReportPath)) {
        if ($RequireRealIdeReport) {
            throw "[cabal] ide_e2e_report_path is required when -RequireRealIdeReport is set"
        }
        $resolvedIdeReportPath = Join-Path $repoRoot "spec/contracts/ide_e2e_report.pass.json"
        $ideReportSource = "fixture"
    }
    else {
        if ([System.IO.Path]::IsPathRooted($IdeE2EReportPath)) {
            $resolvedIdeReportPath = (Resolve-Path -LiteralPath $IdeE2EReportPath).Path
        }
        else {
            $resolvedIdeReportPath = (Resolve-Path -LiteralPath (Join-Path $repoRoot $IdeE2EReportPath)).Path
        }
        $ideReportSource = "user"
    }

    if (-not (Test-Path -LiteralPath $resolvedIdeReportPath)) {
        throw "[cabal] IDE E2E report file not found: $resolvedIdeReportPath"
    }
    Write-Host "[cabal] IDE E2E report source: $ideReportSource" -ForegroundColor DarkCyan
    Write-Host "[cabal] IDE E2E report path: $resolvedIdeReportPath" -ForegroundColor DarkCyan

    Write-Host "[cabal] step 1/3: stress SLA gate" -ForegroundColor DarkCyan
    $stressCmd = "powershell -ExecutionPolicy Bypass -File `"$($scriptRoot)\check-stress-sla.ps1`""
    & powershell -ExecutionPolicy Bypass -File (Join-Path $scriptRoot "check-stress-sla.ps1")
    if ($LASTEXITCODE -ne 0) {
        throw "[cabal] unified release gate failed: stress SLA gate"
    }
    Add-StepResult -Name "stress_sla_gate" -Status "PASS" -Command $stressCmd

    Write-Host "[cabal] step 2/3: IDE contract gate" -ForegroundColor DarkCyan
    $ideScript = Join-Path $scriptRoot "check-ide-contract-gate.ps1"
    $ideCmd = "powershell -ExecutionPolicy Bypass -File `"$ideScript`""
    $ideArgs = @(
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        $ideScript
    )
    if ($WithIntegration) {
        $ideCmd += " -WithIntegration"
        $ideArgs += "-WithIntegration"
    }

    & powershell @ideArgs
    if ($LASTEXITCODE -ne 0) {
        throw "[cabal] unified release gate failed: IDE contract gate"
    }
    Add-StepResult -Name "ide_contract_gate" -Status "PASS" -Command $ideCmd

    Write-Host "[cabal] step 3/3: IDE E2E schema gate" -ForegroundColor DarkCyan
    $validatorScript = Join-Path $scriptRoot "validate-ide-e2e-report.ps1"
    $schemaCmd = "powershell -ExecutionPolicy Bypass -File `"$validatorScript`" -ReportPath `"$resolvedIdeReportPath`""
    $schemaArgs = @(
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        $validatorScript,
        "-ReportPath",
        $resolvedIdeReportPath
    )
    if ($RequireRealIdeReport) {
        if ($RealIdeReportMaxAgeHours -le 0) {
            throw "[cabal] RealIdeReportMaxAgeHours must be > 0 when -RequireRealIdeReport is set"
        }
        $schemaCmd += " -MaxReportAgeHours $RealIdeReportMaxAgeHours"
        $schemaArgs += "-MaxReportAgeHours"
        $schemaArgs += $RealIdeReportMaxAgeHours
    }

    & powershell @schemaArgs
    if ($LASTEXITCODE -ne 0) {
        throw "[cabal] unified release gate failed: IDE E2E schema gate"
    }
    Add-StepResult -Name "ide_e2e_schema_gate" -Status "PASS" -Command $schemaCmd
}
catch {
    $failureMessage = $_.Exception.Message
    Add-StepResult -Name "unified_release_gate" -Status "FAIL" -Command "check-release-gates.ps1" -Message $failureMessage
}
finally {
    $summary = [ordered]@{
        schema_version = "1.0.0"
        timestamp_utc = (Get-Date).ToUniversalTime().ToString("o")
        with_integration = [bool]$WithIntegration
        require_real_ide_report = [bool]$RequireRealIdeReport
        real_ide_report_max_age_hours = if ($RequireRealIdeReport) { $RealIdeReportMaxAgeHours } else { $null }
        ide_e2e_report_source = $ideReportSource
        ide_e2e_report_path = $resolvedIdeReportPath
        gate = if ($null -eq $failureMessage) { "PASS" } else { "FAIL" }
        steps = $steps
    }

    if ($null -ne $failureMessage) {
        $summary.failure = $failureMessage
    }

    $summary | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 -Path $SummaryPath
    Write-Host "[cabal] release gate summary written: $SummaryPath" -ForegroundColor DarkCyan
}

if ($null -ne $failureMessage) {
    throw $failureMessage
}

Write-Host "[cabal] unified release gates passed." -ForegroundColor Green
