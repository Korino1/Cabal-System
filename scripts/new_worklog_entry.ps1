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

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
function Write-Utf8NoBomLines([string]$path, $lines) {
  $text = ($lines | ForEach-Object { $_.ToString() }) -join "`n"
  if ($text -notmatch "`n$") { $text += "`n" }
  [System.IO.File]::WriteAllText($path, $text, $utf8NoBom)
}
function Append-Utf8NoBomText([string]$path, [string]$text) {
  if ($text -notmatch "`n$") { $text += "`n" }
  [System.IO.File]::AppendAllText($path, $text, $utf8NoBom)
}

$phaseDir = Join-Path (Join-Path $repoRoot ".memory\\PHASES") $PhaseId
$indexPath = Join-Path $phaseDir "WORKLOG.md"

if (!(Test-Path -LiteralPath $phaseDir)) {
  Write-Error "Phase dir not found: $phaseDir"
  exit 2
}

if (!(Test-Path -LiteralPath $indexPath)) {
  Write-Utf8NoBomLines $indexPath @('---','id: worklog',"updated: $(Get-Date -Format 'yyyy-MM-dd')","phase: $PhaseId",'---')
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
Write-Utf8NoBomLines $worklogPath $header

$indexLine = "- $(Get-Date -Format 'yyyy-MM-dd HH:mm'): $fileName — $Title"
Append-Utf8NoBomText $indexPath $indexLine

# Optional rotation check
$lines = (Get-Content -Encoding UTF8 -Path $worklogPath).Length
if ($lines -gt $MaxLines) {
  Write-Host "WARN: $fileName exceeds $MaxLines lines. Update DIGEST.md."
}

Write-Host "OK: created $fileName and indexed in WORKLOG.md"
exit 0
