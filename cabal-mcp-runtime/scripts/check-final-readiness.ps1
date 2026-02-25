param(
    [Parameter(Mandatory = $true)]
    [string]$IdeE2EReportPath,

    [int]$RealIdeReportMaxAgeHours = 24,
    [string]$VsCodeLogPath,
    [string]$JetBrainsLogPath,

    [string]$RepoOwner,
    [string]$RepoName,
    [string]$Branch = "main",

    [string]$StatusCheck = "stress-sla-gate",
    [string[]]$AdditionalStatusChecks = @(),
    [switch]$UseCabalRecommendedChecks = $true,

    [string]$ProtectionJsonPath,
    [string]$SummaryPath,
    [string]$FinalSummaryPath
)

$ErrorActionPreference = "Stop"
$scriptRoot = $PSScriptRoot
$runtimeRoot = (Resolve-Path (Join-Path $scriptRoot "..")).Path

if ([string]::IsNullOrWhiteSpace($SummaryPath)) {
    $SummaryPath = Join-Path $runtimeRoot ".cabal_runtime\final_readiness_summary.json"
}
if ([string]::IsNullOrWhiteSpace($FinalSummaryPath)) {
    $FinalSummaryPath = Join-Path $runtimeRoot ".cabal_runtime\final_readiness_result.json"
}

$resultDir = Split-Path -Parent $FinalSummaryPath
if (-not [string]::IsNullOrWhiteSpace($resultDir)) {
    New-Item -ItemType Directory -Force -Path $resultDir | Out-Null
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

    $item = [ordered]@{
        name = $Name
        status = $Status
        command = $Command
    }
    if (-not [string]::IsNullOrWhiteSpace($Message)) {
        $item.message = $Message
    }
    $script:steps += [pscustomobject]$item
}

$failureMessage = $null

Write-Host "[cabal] final readiness check started..." -ForegroundColor Cyan
try {
    if (-not [string]::IsNullOrWhiteSpace($VsCodeLogPath) -or -not [string]::IsNullOrWhiteSpace($JetBrainsLogPath)) {
        if ([string]::IsNullOrWhiteSpace($VsCodeLogPath) -or [string]::IsNullOrWhiteSpace($JetBrainsLogPath)) {
            throw "[cabal] both VsCodeLogPath and JetBrainsLogPath must be provided together"
        }
        Write-Host "[cabal] step 1/4: validate real IDE artifacts" -ForegroundColor DarkCyan
        $realCmd = "powershell -ExecutionPolicy Bypass -File `"$($scriptRoot)\validate-real-ide-e2e-artifacts.ps1`" -ReportPath `"$IdeE2EReportPath`" -VsCodeLogPath `"$VsCodeLogPath`" -JetBrainsLogPath `"$JetBrainsLogPath`" -MaxReportAgeHours $RealIdeReportMaxAgeHours"
        & powershell -ExecutionPolicy Bypass -File (Join-Path $scriptRoot "validate-real-ide-e2e-artifacts.ps1") `
            -ReportPath $IdeE2EReportPath `
            -VsCodeLogPath $VsCodeLogPath `
            -JetBrainsLogPath $JetBrainsLogPath `
            -MaxReportAgeHours $RealIdeReportMaxAgeHours
        if ($LASTEXITCODE -ne 0) {
            throw "[cabal] final readiness failed at real IDE artifacts validation"
        }
        Add-StepResult -Name "validate_real_ide_artifacts" -Status "PASS" -Command $realCmd
    }

    $stepNum = if ([string]::IsNullOrWhiteSpace($VsCodeLogPath)) { "1/3" } else { "2/4" }
    Write-Host "[cabal] step ${stepNum}: strict release gate (real IDE report)" -ForegroundColor DarkCyan
    $releaseCmd = "powershell -ExecutionPolicy Bypass -File `"$($scriptRoot)\check-release-gates.ps1`" -WithIntegration -IdeE2EReportPath `"$IdeE2EReportPath`" -RequireRealIdeReport -RealIdeReportMaxAgeHours $RealIdeReportMaxAgeHours -SummaryPath `"$SummaryPath`""
    $releaseArgs = @(
        "-ExecutionPolicy", "Bypass",
        "-File", (Join-Path $scriptRoot "check-release-gates.ps1"),
        "-WithIntegration",
        "-IdeE2EReportPath", $IdeE2EReportPath,
        "-RequireRealIdeReport",
        "-RealIdeReportMaxAgeHours", $RealIdeReportMaxAgeHours,
        "-SummaryPath", $SummaryPath
    )
    & powershell @releaseArgs
    if ($LASTEXITCODE -ne 0) {
        throw "[cabal] final readiness failed at strict release gate"
    }
    Add-StepResult -Name "strict_release_gate" -Status "PASS" -Command $releaseCmd

    $stepNum = if ([string]::IsNullOrWhiteSpace($VsCodeLogPath)) { "2/3" } else { "3/4" }
    Write-Host "[cabal] step ${stepNum}: validate generated release summary" -ForegroundColor DarkCyan
    $validateCmd = "powershell -ExecutionPolicy Bypass -File `"$($scriptRoot)\validate-release-gate-summary.ps1`" -SummaryPath `"$SummaryPath`""
    & powershell -ExecutionPolicy Bypass -File (Join-Path $scriptRoot "validate-release-gate-summary.ps1") -SummaryPath $SummaryPath
    if ($LASTEXITCODE -ne 0) {
        throw "[cabal] final readiness failed at release summary validation"
    }
    Add-StepResult -Name "validate_release_summary" -Status "PASS" -Command $validateCmd

    $stepNum = if ([string]::IsNullOrWhiteSpace($VsCodeLogPath)) { "3/3" } else { "4/4" }
    Write-Host "[cabal] step ${stepNum}: verify GitHub required status checks" -ForegroundColor DarkCyan
    $verifyArgs = @(
        "-ExecutionPolicy", "Bypass",
        "-File", (Join-Path $scriptRoot "verify-required-status-checks.ps1"),
        "-Branch", $Branch,
        "-StatusCheck", $StatusCheck
    )
    $verifyCmd = "powershell -ExecutionPolicy Bypass -File `"$($scriptRoot)\verify-required-status-checks.ps1`" -Branch $Branch -StatusCheck $StatusCheck"

    if ($UseCabalRecommendedChecks) {
        $verifyArgs += "-UseCabalRecommendedChecks"
        $verifyCmd += " -UseCabalRecommendedChecks"
    }
    foreach ($item in $AdditionalStatusChecks) {
        if (-not [string]::IsNullOrWhiteSpace($item)) {
            $verifyArgs += "-AdditionalStatusChecks"
            $verifyArgs += $item
            $verifyCmd += " -AdditionalStatusChecks `"$item`""
        }
    }

    if (-not [string]::IsNullOrWhiteSpace($ProtectionJsonPath)) {
        $verifyArgs += "-ProtectionJsonPath"
        $verifyArgs += $ProtectionJsonPath
        $verifyCmd += " -ProtectionJsonPath `"$ProtectionJsonPath`""
    }
    else {
        if ([string]::IsNullOrWhiteSpace($RepoOwner) -or [string]::IsNullOrWhiteSpace($RepoName)) {
            throw "[cabal] RepoOwner and RepoName are required when ProtectionJsonPath is not provided"
        }
        $verifyArgs += "-RepoOwner"
        $verifyArgs += $RepoOwner
        $verifyArgs += "-RepoName"
        $verifyArgs += $RepoName
        $verifyCmd += " -RepoOwner `"$RepoOwner`" -RepoName `"$RepoName`""
    }

    & powershell @verifyArgs
    if ($LASTEXITCODE -ne 0) {
        throw "[cabal] final readiness failed at required status checks verification"
    }
    Add-StepResult -Name "verify_required_status_checks" -Status "PASS" -Command $verifyCmd
}
catch {
    $failureMessage = $_.Exception.Message
    Add-StepResult -Name "final_readiness" -Status "FAIL" -Command "check-final-readiness.ps1" -Message $failureMessage
}
finally {
    $summary = [ordered]@{
        schema_version = "1.0.0"
        timestamp_utc = (Get-Date).ToUniversalTime().ToString("o")
        gate = if ($null -eq $failureMessage) { "PASS" } else { "FAIL" }
        ide_e2e_report_path = $IdeE2EReportPath
        release_gate_summary_path = $SummaryPath
        real_ide_logs_provided = (
            (-not [string]::IsNullOrWhiteSpace($VsCodeLogPath)) -and
            (-not [string]::IsNullOrWhiteSpace($JetBrainsLogPath))
        )
        vscode_log_path = if ([string]::IsNullOrWhiteSpace($VsCodeLogPath)) { $null } else { $VsCodeLogPath }
        jetbrains_log_path = if ([string]::IsNullOrWhiteSpace($JetBrainsLogPath)) { $null } else { $JetBrainsLogPath }
        verify_mode = if ([string]::IsNullOrWhiteSpace($ProtectionJsonPath)) { "github" } else { "snapshot" }
        repo_owner = if ([string]::IsNullOrWhiteSpace($RepoOwner)) { $null } else { $RepoOwner }
        repo_name = if ([string]::IsNullOrWhiteSpace($RepoName)) { $null } else { $RepoName }
        branch = $Branch
        steps = $steps
    }

    if ($null -ne $failureMessage) {
        $summary.failure = $failureMessage
    }

    $summary | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 -Path $FinalSummaryPath
    Write-Host "[cabal] final readiness summary written: $FinalSummaryPath" -ForegroundColor DarkCyan
}

if ($null -ne $failureMessage) {
    throw $failureMessage
}
Write-Host "[cabal] final readiness check passed." -ForegroundColor Green
