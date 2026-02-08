param(
  [string]$PhaseId = "",
  [string]$Title = "",
  [int]$MaxLines = 200
)

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $scriptDir "..")).Path
if ([string]::IsNullOrWhiteSpace($PhaseId)) {
  Write-Error "PhaseId is required"
  exit 2
}

$phaseDir = Join-Path (Join-Path $repoRoot ".memory\\PHASES") $PhaseId
$indexPath = Join-Path $phaseDir "WORKLOG.md"

if (!(Test-Path -LiteralPath $phaseDir)) {
  Write-Error "Phase dir not found: $phaseDir"
  exit 2
}

if (!(Test-Path -LiteralPath $indexPath)) {
  @('---','id: worklog',"updated: $(Get-Date -Format 'yyyy-MM-dd')","phase: $PhaseId",'---') | Set-Content -Encoding UTF8 -Path $indexPath
}

$stamp = Get-Date -Format "yyyyMMdd-HHmm"
$fileName = "WORKLOG-$stamp.md"
$worklogPath = Join-Path $phaseDir $fileName

$header = @(
  '---',
  "id: worklog_entry",
  "phase: $PhaseId",
  "created: $(Get-Date -Format 'yyyy-MM-dd HH:mm')",
  '---',
  "# $fileName",
  "",
  "## Summary",
  "- ${Title}",
  "",
  "## Details",
  "- "
)
$header | Set-Content -Encoding UTF8 -Path $worklogPath

$indexLine = "- $(Get-Date -Format 'yyyy-MM-dd HH:mm'): $fileName — $Title"
Add-Content -Encoding UTF8 -Path $indexPath -Value $indexLine

# Optional rotation check
$lines = (Get-Content -Encoding UTF8 -Path $worklogPath).Length
if ($lines -gt $MaxLines) {
  Write-Host "WARN: $fileName exceeds $MaxLines lines. Update DIGEST.md."
}

Write-Host "OK: created $fileName and indexed in WORKLOG.md"
exit 0
