param(
    [Parameter(Mandatory = $true)]
    [string]$ReportPath,
    [Parameter(Mandatory = $true)]
    [string]$VsCodeLogPath,
    [Parameter(Mandatory = $true)]
    [string]$JetBrainsLogPath,
    [int]$MaxReportAgeHours = 24
)

$ErrorActionPreference = "Stop"
$scriptRoot = $PSScriptRoot

if (-not (Test-Path -LiteralPath $VsCodeLogPath)) {
    throw "VS Code log file not found: $VsCodeLogPath"
}
if (-not (Test-Path -LiteralPath $JetBrainsLogPath)) {
    throw "JetBrains log file not found: $JetBrainsLogPath"
}

$vsLen = (Get-Item -LiteralPath $VsCodeLogPath).Length
$jbLen = (Get-Item -LiteralPath $JetBrainsLogPath).Length
if ($vsLen -le 0) {
    throw "VS Code log file is empty: $VsCodeLogPath"
}
if ($jbLen -le 0) {
    throw "JetBrains log file is empty: $JetBrainsLogPath"
}

& powershell -ExecutionPolicy Bypass -File (Join-Path $scriptRoot "validate-ide-e2e-report.ps1") -ReportPath $ReportPath -MaxReportAgeHours $MaxReportAgeHours
if ($LASTEXITCODE -ne 0) {
    throw "real IDE E2E report validation failed"
}

$raw = Get-Content -LiteralPath $ReportPath -Raw -Encoding UTF8
$report = $raw | ConvertFrom-Json

$vscode = $report.profiles | Where-Object { $_.profile -eq "vscode" } | Select-Object -First 1
$jetbrains = $report.profiles | Where-Object { $_.profile -eq "jetbrains" } | Select-Object -First 1

if ($null -eq $vscode -or $null -eq $jetbrains) {
    throw "report must contain both vscode and jetbrains profile entries"
}

$requiredChecks = @("IDE-P1", "IDE-P2", "IDE-P3", "IDE-P4", "IDE-P5")
foreach ($check in $requiredChecks) {
    if (-not [bool]$vscode.checks.$check) {
        throw "vscode profile check not passed: $check"
    }
    if (-not [bool]$jetbrains.checks.$check) {
        throw "jetbrains profile check not passed: $check"
    }
}

$vsText = Get-Content -LiteralPath $VsCodeLogPath -Raw -Encoding UTF8
$jbText = Get-Content -LiteralPath $JetBrainsLogPath -Raw -Encoding UTF8

if ($vsText -notmatch "initialize" -or $jbText -notmatch "initialize") {
    throw "both logs must contain initialize traces"
}
if ($vsText -notmatch "route_consult" -or $jbText -notmatch "route_consult") {
    throw "both logs must contain route_consult traces"
}
if ($vsText -notmatch "ack_cross_rules" -or $jbText -notmatch "ack_cross_rules") {
    throw "both logs must contain ack_cross_rules traces"
}

Write-Host "[cabal] real IDE E2E artifacts are valid." -ForegroundColor Green
