param(
  [string]$PhaseId = "",
  [int]$MaxLines = 200
)

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $scriptDir "..")).Path
if ([string]::IsNullOrWhiteSpace($PhaseId)) {
  Write-Error "PhaseId is required"
  exit 2
}

$phaseDir = Join-Path (Join-Path $repoRoot ".memory\\PHASES") $PhaseId
if (!(Test-Path -LiteralPath $phaseDir)) {
  Write-Error "Phase dir not found: $phaseDir"
  exit 2
}

$files = Get-ChildItem -Path $phaseDir -Filter 'WORKLOG-*.md' -File -ErrorAction SilentlyContinue
$bad = @()
foreach ($f in $files) {
  $count = (Get-Content -Encoding UTF8 -Path $f.FullName).Length
  if ($count -gt $MaxLines) {
    $bad += "${($f.Name)} ($count)"
  }
}

if ($bad.Count -gt 0) {
  Write-Host "WORKLOG entries exceeding $MaxLines lines:"
  $bad | ForEach-Object { Write-Host $_ }
  exit 1
}

Write-Host "OK: all WORKLOG entries <= $MaxLines lines."
exit 0
