param(
  [string]$Current = "",
  [string]$Next = ""
)

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $scriptDir "..")).Path
$globalIndexPath = Join-Path $repoRoot ".memory\\GLOBAL_INDEX.md"
$today = Get-Date -Format 'yyyy-MM-dd'

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
function Write-Utf8NoBomText([string]$path, [string]$text) {
  if ($text -notmatch "`n$") { $text += "`n" }
  [System.IO.File]::WriteAllText($path, $text, $utf8NoBom)
}

if ([string]::IsNullOrWhiteSpace($Current) -or [string]::IsNullOrWhiteSpace($Next)) {
  Write-Error "Current and Next are required"
  exit 2
}

if (!(Test-Path -LiteralPath $globalIndexPath)) {
  Write-Error "GLOBAL_INDEX.md not found: $globalIndexPath"
  exit 2
}

function Set-PhaseState([string]$phaseId, [string]$state) {
  $indexPath = Join-Path (Join-Path $repoRoot ".memory\\PHASES") "$phaseId\\INDEX.md"
  if (!(Test-Path -LiteralPath $indexPath)) {
    Write-Error "Phase INDEX not found: $indexPath"
    exit 2
  }

  $content = Get-Content -Raw -Encoding UTF8 $indexPath
  if ($content -notmatch '(?im)^\s*-\s*State:\s*.+$') {
    Write-Error "Unable to find '- State:' in $indexPath"
    exit 1
  }

  $content = $content -replace '(?im)^\s*-\s*State:\s*.+$', "- State: $state"
  $content = $content -replace '(?im)^updated:\s*\d{4}-\d{2}-\d{2}\s*$', "updated: $today"
  Write-Utf8NoBomText $indexPath $content
}

function Ensure-NextDigest([string]$phaseId) {
  $digestPath = Join-Path (Join-Path $repoRoot ".memory\\PHASES") "$phaseId\\DIGEST.md"
  if (!(Test-Path -LiteralPath $digestPath) -or [string]::IsNullOrWhiteSpace((Get-Content -Raw -Encoding UTF8 $digestPath))) {
    $template = @(
      '---',
      'id: digest',
      "phase: $phaseId",
      "updated: $today",
      '---',
      '# Digest',
      '',
      '- Status: in_progress',
      '- Scope:',
      '- Key outcomes:',
      '- Decisions:',
      '- Assumptions:',
      '- Open questions:',
      '- Dependencies:',
      '- References:'
    ) -join "`n"
    Write-Utf8NoBomText $digestPath $template
    return
  }

  $digest = Get-Content -Raw -Encoding UTF8 $digestPath
  if ($digest -match '(?im)^\s*-\s*Status:\s*.+$') {
    $digest = $digest -replace '(?im)^\s*-\s*Status:\s*.+$', '- Status: in_progress'
  } else {
    $digest += "`n- Status: in_progress"
  }
  $digest = $digest -replace '(?im)^updated:\s*\d{4}-\d{2}-\d{2}\s*$', "updated: $today"
  Write-Utf8NoBomText $digestPath $digest
}

$lines = Get-Content -Encoding UTF8 $globalIndexPath
$updatedLines = New-Object System.Collections.Generic.List[string]

for ($i = 0; $i -lt $lines.Count; $i++) {
  $line = $lines[$i]

  if ($line -match '^\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|?\s*$') {
    $phase = $matches[1].Trim()
    $title = $matches[2].Trim()
    $status = $matches[3].Trim()
    $indexRef = $matches[4].Trim()
    if ($phase -eq $Current) { $status = 'done' }
    if ($phase -eq $Next) { $status = 'in_progress' }
    $line = "| $phase | $title | $status | $indexRef |"
  }

  $line = $line -replace '(?i)^updated:\s*\d{4}-\d{2}-\d{2}\s*$', "updated: $today"
  $updatedLines.Add($line)
}

for ($i = 0; $i -lt $updatedLines.Count; $i++) {
  if ($updatedLines[$i] -match '^##\s+Active Phase\s*$') {
    if ($i + 1 -lt $updatedLines.Count -and $updatedLines[$i + 1] -match '^\s*-\s*ID:\s*') {
      $updatedLines[$i + 1] = "- ID: $Next"
    }
    if ($i + 2 -lt $updatedLines.Count -and $updatedLines[$i + 2] -match '^\s*-\s*Path:\s*') {
      $updatedLines[$i + 2] = "- Path: .memory/PHASES/$Next"
    }
    break
  }
}

Write-Utf8NoBomText $globalIndexPath (($updatedLines -join "`n"))
Set-PhaseState -phaseId $Current -state 'done'
Set-PhaseState -phaseId $Next -state 'in_progress'
Ensure-NextDigest -phaseId $Next

Write-Host "OK: Active Phase switched $Current -> $Next"
exit 0
