param(
    [string]$RepoOwner,
    [string]$RepoName,
    [string]$Branch = "main",
    [string]$StatusCheck = "stress-sla-gate",
    [string[]]$AdditionalStatusChecks = @(),
    [switch]$UseCabalRecommendedChecks,
    [string]$ProtectionJsonPath
)

$ErrorActionPreference = "Stop"

$expected = @()
if (-not [string]::IsNullOrWhiteSpace($StatusCheck)) {
    $expected += $StatusCheck.Trim()
}
if ($UseCabalRecommendedChecks) {
    $expected += @(
        "ide-contract-gate",
        "ide-e2e-report-schema-gate",
        "release-summary-schema-gate",
        "release-gate"
    )
}
foreach ($check in $AdditionalStatusChecks) {
    if ([string]::IsNullOrWhiteSpace($check)) {
        continue
    }
    foreach ($part in ($check -split ",")) {
        if (-not [string]::IsNullOrWhiteSpace($part)) {
            $expected += $part.Trim()
        }
    }
}
$expected = $expected | Sort-Object -Unique
if ($expected.Count -eq 0) {
    throw "at least one expected status check is required"
}

$protection = $null
if (-not [string]::IsNullOrWhiteSpace($ProtectionJsonPath)) {
    if (-not (Test-Path -LiteralPath $ProtectionJsonPath)) {
        throw "protection json file not found: $ProtectionJsonPath"
    }
    $protection = (Get-Content -LiteralPath $ProtectionJsonPath -Raw -Encoding UTF8) | ConvertFrom-Json
}
else {
    if ([string]::IsNullOrWhiteSpace($RepoOwner) -or [string]::IsNullOrWhiteSpace($RepoName)) {
        throw "RepoOwner and RepoName are required when ProtectionJsonPath is not provided"
    }
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        throw "gh CLI is required. Install GitHub CLI first."
    }
    $route = "/repos/$RepoOwner/$RepoName/branches/$Branch/protection"
    $json = gh api -H "Accept: application/vnd.github+json" $route
    $protection = $json | ConvertFrom-Json
}

$actual = @()
if ($null -ne $protection.required_status_checks -and $null -ne $protection.required_status_checks.contexts) {
    $actual = @($protection.required_status_checks.contexts | ForEach-Object { [string]$_ })
}
$actual = $actual | Sort-Object -Unique

$missing = @($expected | Where-Object { $_ -notin $actual })
$extra = @($actual | Where-Object { $_ -notin $expected })

Write-Host "[cabal] expected checks: $($expected -join ', ')" -ForegroundColor Cyan
Write-Host "[cabal] actual checks: $($actual -join ', ')" -ForegroundColor Cyan

if ($missing.Count -gt 0) {
    throw "[cabal] missing required status checks: $($missing -join ', ')"
}

if ($extra.Count -gt 0) {
    Write-Host "[cabal] extra checks present (allowed): $($extra -join ', ')" -ForegroundColor Yellow
}

Write-Host "[cabal] required status checks are configured correctly." -ForegroundColor Green
