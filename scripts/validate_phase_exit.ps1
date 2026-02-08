param(
  [string]$PhaseId = ""
)

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $scriptDir "..")).Path

if ([string]::IsNullOrWhiteSpace($PhaseId)) {
  Write-Error "PhaseId is required (e.g., GA-1)"
  exit 2
}

$phaseDir = Join-Path (Join-Path $repoRoot ".memory\\PHASES") $PhaseId
$indexPath = Join-Path $phaseDir "INDEX.md"
$digestPath = Join-Path $phaseDir "DIGEST.md"
$globalIndexPath = Join-Path $repoRoot ".memory\\GLOBAL_INDEX.md"

if (!(Test-Path -LiteralPath $indexPath)) {
  Write-Error "Missing INDEX.md: $indexPath"
  exit 2
}
if (!(Test-Path -LiteralPath $digestPath)) {
  Write-Error "Missing DIGEST.md: $digestPath"
  exit 2
}
if (!(Test-Path -LiteralPath $globalIndexPath)) {
  Write-Error "Missing GLOBAL_INDEX.md: $globalIndexPath"
  exit 2
}

$index = Get-Content -Raw -Encoding UTF8 $indexPath
$digest = Get-Content -Raw -Encoding UTF8 $digestPath
$globalIndex = Get-Content -Raw -Encoding UTF8 $globalIndexPath

if ($index -notmatch '(?im)^\s*-\s*State:\s*done\s*$') {
  Write-Error "Phase not marked done in INDEX.md (Status: done required)"
  exit 1
}

# Validate Exit Criteria (must exist and not contain TODO when phase is done)
$indexLines = $index -split "\r?\n"
$exitIdx = -1
for ($i = 0; $i -lt $indexLines.Length; $i++) {
  if ($indexLines[$i] -match '(?i)^\s*-\s*Exit Criteria:\s*$') {
    $exitIdx = $i
    break
  }
}
if ($exitIdx -lt 0) {
  Write-Error "Exit Criteria section not found in INDEX.md"
  exit 1
}
$exitItems = @()
for ($i = $exitIdx + 1; $i -lt $indexLines.Length; $i++) {
  $line = $indexLines[$i]
  if ($line -match '^##\s+') { break }
  if ($line -match '^\s*-\s+.+$') { $exitItems += $line }
}
if ($exitItems.Count -eq 0) {
  Write-Error "Exit Criteria has no items in INDEX.md"
  exit 1
}
foreach ($item in $exitItems) {
  if ($item -match '(?i)\bTODO\b') {
    Write-Error "Exit Criteria contains TODO in INDEX.md: $item"
    exit 1
  }
}

# Digest must be marked done when phase is done
if ($digest -notmatch '(?im)^\s*-\s*Status:\s*done\s*$') {
  Write-Error "Digest not marked done (expected '- Status: done')"
  exit 1
}

# Validate Evidence: must exist and point to existing artifacts
$evidenceIdx = -1
for ($i = 0; $i -lt $indexLines.Length; $i++) {
  if ($indexLines[$i] -match '^##\s+Evidence\s*$') {
    $evidenceIdx = $i
    break
  }
}
if ($evidenceIdx -lt 0) {
  Write-Error "Evidence section not found in INDEX.md"
  exit 1
}

function Normalize-EvidencePath([string]$raw) {
  $s = $raw.Trim()
  $s = $s -replace '^\s*-\s*', ''
  $s = $s.Replace('`','')
  $s = $s.Trim().TrimEnd('.',';',',')
  if ($s.Contains(' ')) { $s = ($s -split '\s+')[0] }
  if ($s.Contains('(')) { $s = $s.Split('(')[0] }
  if ($s.Contains('[')) { $s = $s.Split('[')[0] }
  $s = $s.Trim().TrimEnd('.',';',',')

  # Alias: PHASES/... means .memory/PHASES/...
  if ($s -match '^(?i)PHASES[\\/]') {
    $s = ".memory/" + $s
  }
  if ($s -eq 'LOGIC_PROTOCOL.md') {
    $s = ".memory/LOGIC_PROTOCOL.md"
  }

  return $s.Replace('/','\')
}

$evidence = @()
for ($i = $evidenceIdx + 1; $i -lt $indexLines.Length; $i++) {
  $line = $indexLines[$i]
  if ($line -match '^##\s+') { break }
  if ($line -match '^\s*-\s+.+$') { $evidence += $line }
}
if ($evidence.Count -eq 0) {
  Write-Error "Evidence has no items in INDEX.md"
  exit 1
}

$missing = @()
foreach ($ev in $evidence) {
  $p = Normalize-EvidencePath $ev
  if ([string]::IsNullOrWhiteSpace($p)) { continue }
  $full = Join-Path $repoRoot $p
  if (!(Test-Path -LiteralPath $full)) {
    $missing += $p
  }
}
if ($missing.Count -gt 0) {
  Write-Host "Missing evidence artifacts:"
  $missing | ForEach-Object { Write-Host " - $_" }
  exit 1
}

# GLOBAL_INDEX must mark phase as done
$giLine = ($globalIndex -split "\r?\n") | Where-Object { $_ -match ('^\|\s*' + [regex]::Escape($PhaseId) + '\s*\|') } | Select-Object -First 1
if ([string]::IsNullOrWhiteSpace($giLine)) {
  Write-Error "Phase row not found in GLOBAL_INDEX.md: $PhaseId"
  exit 1
}
$cols = $giLine.Split('|') | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne '' }
if ($cols.Count -lt 3) {
  Write-Error "GLOBAL_INDEX row parse error: $giLine"
  exit 1
}
$phaseStatus = $cols[2]
if ($phaseStatus -notmatch '^(?i)done$') {
  Write-Error "GLOBAL_INDEX status is not done for $PhaseId (found: '$phaseStatus')"
  exit 1
}

Write-Host "OK: phase exit criteria satisfied."
exit 0
