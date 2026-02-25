param(
    [Parameter(Mandatory = $true)]
    [string]$SummaryPath
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $SummaryPath)) {
    throw "summary file not found: $SummaryPath"
}

$raw = Get-Content -LiteralPath $SummaryPath -Raw -Encoding UTF8
$summary = $raw | ConvertFrom-Json

if ($null -eq $summary.schema_version -or $summary.schema_version -ne "1.0.0") {
    throw "invalid schema_version; expected 1.0.0"
}

if ([string]::IsNullOrWhiteSpace([string]$summary.ide_e2e_report_path)) {
    throw "missing ide_e2e_report_path"
}
if ([string]::IsNullOrWhiteSpace([string]$summary.release_gate_summary_path)) {
    throw "missing release_gate_summary_path"
}
if ([string]::IsNullOrWhiteSpace([string]$summary.branch)) {
    throw "missing branch"
}

$mode = [string]$summary.verify_mode
if ($mode -ne "github" -and $mode -ne "snapshot") {
    throw "invalid verify_mode; expected github|snapshot"
}

if ($summary.gate -ne "PASS" -and $summary.gate -ne "FAIL") {
    throw "invalid gate value; expected PASS or FAIL"
}

if ($null -eq $summary.steps -or $summary.steps.Count -lt 1) {
    throw "steps section must contain at least one entry"
}

$requiredCoreSteps = @("strict_release_gate", "validate_release_summary", "verify_required_status_checks")
foreach ($stepName in $requiredCoreSteps) {
    $step = $summary.steps | Where-Object { $_.name -eq $stepName } | Select-Object -First 1
    if ($null -eq $step) {
        throw "missing required step in summary: $stepName"
    }
    if ($step.status -ne "PASS" -and $step.status -ne "FAIL") {
        throw "invalid step status for $stepName"
    }
    if ([string]::IsNullOrWhiteSpace([string]$step.command)) {
        throw "empty command field for step $stepName"
    }
}

$logsProvided = [bool]$summary.real_ide_logs_provided
if ($logsProvided) {
    if ([string]::IsNullOrWhiteSpace([string]$summary.vscode_log_path)) {
        throw "real_ide_logs_provided=true requires vscode_log_path"
    }
    if ([string]::IsNullOrWhiteSpace([string]$summary.jetbrains_log_path)) {
        throw "real_ide_logs_provided=true requires jetbrains_log_path"
    }
    $logsStep = $summary.steps | Where-Object { $_.name -eq "validate_real_ide_artifacts" } | Select-Object -First 1
    if ($null -eq $logsStep) {
        throw "real_ide_logs_provided=true requires validate_real_ide_artifacts step"
    }
}

if ($summary.gate -eq "PASS") {
    foreach ($step in $summary.steps) {
        if ($step.status -ne "PASS") {
            throw "gate PASS requires PASS status for all steps"
        }
    }
}
else {
    if ([string]::IsNullOrWhiteSpace([string]$summary.failure)) {
        throw "gate FAIL requires non-empty failure field"
    }
}

Write-Host "[cabal] final readiness summary is valid." -ForegroundColor Green
