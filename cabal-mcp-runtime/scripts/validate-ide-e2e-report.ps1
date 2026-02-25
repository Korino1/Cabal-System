param(
    [Parameter(Mandatory = $true)]
    [string]$ReportPath,
    [int]$MaxReportAgeHours = 0
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $ReportPath)) {
    throw "report file not found: $ReportPath"
}

$raw = Get-Content -LiteralPath $ReportPath -Raw -Encoding UTF8
$report = $raw | ConvertFrom-Json

if ($null -eq $report.schema_version -or $report.schema_version -ne "1.0.0") {
    throw "invalid schema_version; expected 1.0.0"
}

if ($null -eq $report.profiles -or $report.profiles.Count -lt 2) {
    throw "profiles section must include at least vscode and jetbrains entries"
}

$requiredProfiles = @("vscode", "jetbrains")
$requiredChecks = @("IDE-P1", "IDE-P2", "IDE-P3", "IDE-P4", "IDE-P5")

foreach ($profileName in $requiredProfiles) {
    $profile = $report.profiles | Where-Object { $_.profile -eq $profileName } | Select-Object -First 1
    if ($null -eq $profile) {
        throw "missing profile entry: $profileName"
    }
    foreach ($checkId in $requiredChecks) {
        if ($null -eq $profile.checks.$checkId) {
            throw "missing check $checkId for profile $profileName"
        }
        if (-not [bool]$profile.checks.$checkId) {
            throw "failed check $checkId for profile $profileName"
        }
    }
}

if ($MaxReportAgeHours -gt 0) {
    $tsRaw = [string]$report.timestamp_utc
    if ([string]::IsNullOrWhiteSpace($tsRaw)) {
        throw "timestamp_utc is required when MaxReportAgeHours > 0"
    }

    try {
        $timestamp = [datetimeoffset]::Parse(
            $tsRaw,
            [System.Globalization.CultureInfo]::InvariantCulture,
            [System.Globalization.DateTimeStyles]::AssumeUniversal
        )
    }
    catch {
        throw "invalid timestamp_utc format: $tsRaw"
    }

    $now = [datetimeoffset]::UtcNow
    $ageHours = ($now - $timestamp).TotalHours

    if ($ageHours -lt -0.1) {
        throw "timestamp_utc cannot be in the future: $tsRaw"
    }
    if ($ageHours -gt $MaxReportAgeHours) {
        throw "report is too old: age=${ageHours}h exceeds MaxReportAgeHours=$MaxReportAgeHours"
    }
}

Write-Host "[cabal] IDE E2E report is valid and all required checks passed." -ForegroundColor Green
