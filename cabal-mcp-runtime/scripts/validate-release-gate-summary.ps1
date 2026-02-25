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

if ($summary.gate -ne "PASS" -and $summary.gate -ne "FAIL") {
    throw "invalid gate value; expected PASS or FAIL"
}

if ($null -eq $summary.require_real_ide_report) {
    throw "missing require_real_ide_report field"
}
$requireRealIdeReport = [bool]$summary.require_real_ide_report

if ($null -eq $summary.steps -or $summary.steps.Count -lt 1) {
    throw "steps section must contain at least one entry"
}

$requiredSteps = @("stress_sla_gate", "ide_contract_gate", "ide_e2e_schema_gate")
foreach ($stepName in $requiredSteps) {
    $step = $summary.steps | Where-Object { $_.name -eq $stepName } | Select-Object -First 1
    if ($null -eq $step) {
        throw "missing required step in summary: $stepName"
    }
    if ($step.status -ne "PASS" -and $step.status -ne "FAIL") {
        throw "invalid step status for $stepName; expected PASS or FAIL"
    }
    if ([string]::IsNullOrWhiteSpace([string]$step.command)) {
        throw "empty command field for step $stepName"
    }
}

if ($summary.gate -eq "PASS") {
    foreach ($stepName in $requiredSteps) {
        $step = $summary.steps | Where-Object { $_.name -eq $stepName } | Select-Object -First 1
        if ($step.status -ne "PASS") {
            throw "gate PASS requires PASS status for step $stepName"
        }
    }
    if ([string]::IsNullOrWhiteSpace([string]$summary.ide_e2e_report_path)) {
        throw "gate PASS requires non-empty ide_e2e_report_path"
    }
}
else {
    if ([string]::IsNullOrWhiteSpace([string]$summary.failure)) {
        throw "gate FAIL requires non-empty failure field"
    }
}

$source = [string]$summary.ide_e2e_report_source
if ($source -and $source -ne "fixture" -and $source -ne "user" -and $source -ne "unknown") {
    throw "invalid ide_e2e_report_source; expected fixture|user|unknown"
}

if ($requireRealIdeReport) {
    if ($source -ne "user") {
        throw "require_real_ide_report=true requires ide_e2e_report_source=user"
    }
    $maxAge = $summary.real_ide_report_max_age_hours
    if ($null -eq $maxAge -or [int]$maxAge -le 0) {
        throw "require_real_ide_report=true requires positive real_ide_report_max_age_hours"
    }
}

Write-Host "[cabal] release gate summary is valid." -ForegroundColor Green
