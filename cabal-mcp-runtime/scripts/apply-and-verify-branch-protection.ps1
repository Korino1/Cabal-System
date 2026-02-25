param(
    [Parameter(Mandatory = $true)]
    [string]$RepoOwner,
    [Parameter(Mandatory = $true)]
    [string]$RepoName,
    [string]$Branch = "main",
    [string]$StatusCheck = "stress-sla-gate",
    [string[]]$AdditionalStatusChecks = @(),
    [switch]$UseCabalRecommendedChecks = $true
)

$ErrorActionPreference = "Stop"
$scriptRoot = $PSScriptRoot

Write-Host "[cabal] apply+verify branch protection started..." -ForegroundColor Cyan

Write-Host "[cabal] step 1/2: apply required status checks" -ForegroundColor DarkCyan
$applyArgs = @(
    "-ExecutionPolicy", "Bypass",
    "-File", (Join-Path $scriptRoot "set-required-stress-gate.ps1"),
    "-RepoOwner", $RepoOwner,
    "-RepoName", $RepoName,
    "-Branch", $Branch,
    "-StatusCheck", $StatusCheck
)
if ($UseCabalRecommendedChecks) {
    $applyArgs += "-UseCabalRecommendedChecks"
}
foreach ($item in $AdditionalStatusChecks) {
    if (-not [string]::IsNullOrWhiteSpace($item)) {
        $applyArgs += "-AdditionalStatusChecks"
        $applyArgs += $item
    }
}
& powershell @applyArgs
if ($LASTEXITCODE -ne 0) {
    throw "[cabal] apply+verify failed at branch protection apply"
}

Write-Host "[cabal] step 2/2: verify required status checks" -ForegroundColor DarkCyan
$verifyArgs = @(
    "-ExecutionPolicy", "Bypass",
    "-File", (Join-Path $scriptRoot "verify-required-status-checks.ps1"),
    "-RepoOwner", $RepoOwner,
    "-RepoName", $RepoName,
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
& powershell @verifyArgs
if ($LASTEXITCODE -ne 0) {
    throw "[cabal] apply+verify failed at required status checks verification"
}

Write-Host "[cabal] apply+verify branch protection passed." -ForegroundColor Green
