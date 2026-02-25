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
    [string]$SummaryPath
)

$ErrorActionPreference = "Stop"
$scriptRoot = $PSScriptRoot
$runtimeRoot = (Resolve-Path (Join-Path $scriptRoot "..")).Path

if ([string]::IsNullOrWhiteSpace($SummaryPath)) {
    $SummaryPath = Join-Path $runtimeRoot ".cabal_runtime\final_readiness_summary.json"
}

Write-Host "[cabal] final readiness check started..." -ForegroundColor Cyan

if (-not [string]::IsNullOrWhiteSpace($VsCodeLogPath) -or -not [string]::IsNullOrWhiteSpace($JetBrainsLogPath)) {
    if ([string]::IsNullOrWhiteSpace($VsCodeLogPath) -or [string]::IsNullOrWhiteSpace($JetBrainsLogPath)) {
        throw "[cabal] both VsCodeLogPath and JetBrainsLogPath must be provided together"
    }
    Write-Host "[cabal] step 1/4: validate real IDE artifacts" -ForegroundColor DarkCyan
    & powershell -ExecutionPolicy Bypass -File (Join-Path $scriptRoot "validate-real-ide-e2e-artifacts.ps1") `
        -ReportPath $IdeE2EReportPath `
        -VsCodeLogPath $VsCodeLogPath `
        -JetBrainsLogPath $JetBrainsLogPath `
        -MaxReportAgeHours $RealIdeReportMaxAgeHours
    if ($LASTEXITCODE -ne 0) {
        throw "[cabal] final readiness failed at real IDE artifacts validation"
    }
}

$stepNum = if ([string]::IsNullOrWhiteSpace($VsCodeLogPath)) { "1/3" } else { "2/4" }
Write-Host "[cabal] step ${stepNum}: strict release gate (real IDE report)" -ForegroundColor DarkCyan
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

$stepNum = if ([string]::IsNullOrWhiteSpace($VsCodeLogPath)) { "2/3" } else { "3/4" }
Write-Host "[cabal] step ${stepNum}: validate generated release summary" -ForegroundColor DarkCyan
& powershell -ExecutionPolicy Bypass -File (Join-Path $scriptRoot "validate-release-gate-summary.ps1") -SummaryPath $SummaryPath
if ($LASTEXITCODE -ne 0) {
    throw "[cabal] final readiness failed at release summary validation"
}

$stepNum = if ([string]::IsNullOrWhiteSpace($VsCodeLogPath)) { "3/3" } else { "4/4" }
Write-Host "[cabal] step ${stepNum}: verify GitHub required status checks" -ForegroundColor DarkCyan
$verifyArgs = @(
    "-ExecutionPolicy", "Bypass",
    "-File", (Join-Path $scriptRoot "verify-required-status-checks.ps1"),
    "-Branch", $Branch,
    "-StatusCheck", $StatusCheck
)

if ($UseCabalRecommendedChecks) {
    $verifyArgs += "-UseCabalRecommendedChecks"
}
foreach ($item in $AdditionalStatusChecks) {
    if (-not [string]::IsNullOrWhiteSpace($item)) {
        $verifyArgs += "-AdditionalStatusChecks"
        $verifyArgs += $item
    }
}

if (-not [string]::IsNullOrWhiteSpace($ProtectionJsonPath)) {
    $verifyArgs += "-ProtectionJsonPath"
    $verifyArgs += $ProtectionJsonPath
}
else {
    if ([string]::IsNullOrWhiteSpace($RepoOwner) -or [string]::IsNullOrWhiteSpace($RepoName)) {
        throw "[cabal] RepoOwner and RepoName are required when ProtectionJsonPath is not provided"
    }
    $verifyArgs += "-RepoOwner"
    $verifyArgs += $RepoOwner
    $verifyArgs += "-RepoName"
    $verifyArgs += $RepoName
}

& powershell @verifyArgs
if ($LASTEXITCODE -ne 0) {
    throw "[cabal] final readiness failed at required status checks verification"
}

Write-Host "[cabal] final readiness check passed." -ForegroundColor Green
