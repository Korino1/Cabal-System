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
    $gh = Get-Command gh -ErrorAction SilentlyContinue
    if ($null -ne $gh) {
        $route = "/repos/$RepoOwner/$RepoName/branches/$Branch/protection"
        $json = gh api -H "Accept: application/vnd.github+json" $route
        $protection = $json | ConvertFrom-Json
    }
    else {
        $token = $env:GITHUB_TOKEN
        if ([string]::IsNullOrWhiteSpace($token)) {
            $token = $env:GH_TOKEN
        }
        if ([string]::IsNullOrWhiteSpace($token)) {
            throw "gh CLI is not installed and GITHUB_TOKEN/GH_TOKEN is not set."
        }
        $uri = "https://api.github.com/repos/$RepoOwner/$RepoName/branches/$Branch/protection"
        $headers = @{
            "Accept" = "application/vnd.github+json"
            "Authorization" = "Bearer $token"
            "X-GitHub-Api-Version" = "2022-11-28"
        }
        $protection = Invoke-RestMethod -Method Get -Uri $uri -Headers $headers
    }
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
