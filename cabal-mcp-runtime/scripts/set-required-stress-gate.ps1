param(
    [Parameter(Mandatory = $true)]
    [string]$RepoOwner,

    [Parameter(Mandatory = $true)]
    [string]$RepoName,

    [string]$Branch = "main",

    [string]$StatusCheck = "stress-sla-gate",

    [string[]]$AdditionalStatusChecks = @(),

    [switch]$UseCabalRecommendedChecks,

    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

if (-not $DryRun -and -not (Get-Command gh -ErrorAction SilentlyContinue)) {
    throw "gh CLI is required. Install GitHub CLI first."
}

$contexts = @()
if (-not [string]::IsNullOrWhiteSpace($StatusCheck)) {
    $contexts += $StatusCheck.Trim()
}
if ($UseCabalRecommendedChecks) {
    $contexts += @(
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
    $parts = $check -split ","
    foreach ($part in $parts) {
        if (-not [string]::IsNullOrWhiteSpace($part)) {
            $contexts += $part.Trim()
        }
    }
}
$contexts = $contexts | Sort-Object -Unique
if ($contexts.Count -eq 0) {
    throw "At least one status check is required."
}

$payload = @{
    required_status_checks           = @{
        strict   = $true
        contexts = $contexts
    }
    enforce_admins                   = $false
    required_pull_request_reviews    = $null
    restrictions                     = $null
    required_conversation_resolution = $true
} | ConvertTo-Json -Depth 8

Write-Host "[cabal] target repo: $RepoOwner/$RepoName" -ForegroundColor Cyan
Write-Host "[cabal] target branch: $Branch" -ForegroundColor Cyan
Write-Host "[cabal] required checks: $($contexts -join ', ')" -ForegroundColor Cyan

if ($DryRun) {
    Write-Host "[cabal] dry-run payload:" -ForegroundColor Yellow
    Write-Host $payload
    exit 0
}

$tmp = New-TemporaryFile
try {
    Set-Content -Path $tmp -Value $payload -NoNewline -Encoding UTF8
    gh api `
        --method PUT `
        -H "Accept: application/vnd.github+json" `
        "/repos/$RepoOwner/$RepoName/branches/$Branch/protection" `
        --input "$tmp"

    Write-Host "[cabal] branch protection updated." -ForegroundColor Green
}
finally {
    Remove-Item -Path $tmp -ErrorAction SilentlyContinue
}
