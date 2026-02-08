param(
  [string]$StatePath = ".memory\\STATE.md",
  [string]$StateHistoryPath = "",
  [string]$ProgressPath = ".memory\\PROGRESS.md",
  [string]$ActiveId = "",
  [string]$Phase = "",
  [string]$PhaseId = "",
  [string]$Status = "[~]",
  [string]$DefectId = "",
  [string]$NextSteps = "",
  [string]$Blockers = "",
  [string]$FilesTouched = "",
  [string]$Tests = "",
  [string]$Commands = "",
  [string]$Summary = "",
  [switch]$Checkpoint,
  [ValidateSet('PRE','POST')]
  [string]$PhaseLabel = 'POST'
)

# Validations before checkpoint (required)
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $scriptDir "..")).Path
$validatorDefect = Join-Path $scriptDir "validate_defect_ids.ps1"
$validatorOwner = Join-Path $scriptDir "validate_owner.ps1"

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
function Write-Utf8NoBomLines([string]$path, $lines) {
  $text = ($lines | ForEach-Object { $_.ToString() }) -join "`n"
  if ($text -notmatch "`n$") { $text += "`n" }
  [System.IO.File]::WriteAllText($path, $text, $utf8NoBom)
}
function Append-Utf8NoBomLines([string]$path, $lines) {
  $text = ($lines | ForEach-Object { $_.ToString() }) -join "`n"
  if ($text -notmatch "`n$") { $text += "`n" }
  [System.IO.File]::AppendAllText($path, $text, $utf8NoBom)
}

function Resolve-RepoPath([string]$path) {
  if ([string]::IsNullOrWhiteSpace($path)) { return $path }
  if ([System.IO.Path]::IsPathRooted($path)) { return $path }
  return (Join-Path $repoRoot $path)
}

function Invoke-Validator([string]$validatorPath, [string]$name) {
  if (!(Test-Path -LiteralPath $validatorPath)) {
    Write-Error "Missing validator: $name ($validatorPath)"
    exit 1
  }
  & $validatorPath | Out-Host
  if ($LASTEXITCODE -ne 0) {
    Write-Error "Validation failed: $name"
    exit 1
  }
}

Invoke-Validator $validatorDefect "validate_defect_ids.ps1"
Invoke-Validator $validatorOwner "validate_owner.ps1"

$PhaseId = $PhaseId.Trim()
if ([string]::IsNullOrWhiteSpace($PhaseId)) {
  Write-Error "PhaseId is required (PHASES/<Phase>/WORKLOG.md is the only worklog)."
  exit 1
}

$StatePath = Resolve-RepoPath $StatePath
$StateHistoryPath = ""
$ProgressPath = Resolve-RepoPath $ProgressPath
$WorklogPath = ""

$phaseDir = ""
if (![string]::IsNullOrWhiteSpace($PhaseId)) {
  $phaseDir = Join-Path (Join-Path $repoRoot ".memory\\PHASES") $PhaseId
  if (!(Test-Path -LiteralPath $phaseDir)) {
    New-Item -ItemType Directory -Force -Path $phaseDir | Out-Null
  }
  $WorklogPath = Join-Path $phaseDir "WORKLOG.md"
  $StateHistoryPath = Join-Path $phaseDir "STATE_HISTORY.md"
}

$nowDate = Get-Date -Format "yyyy-MM-dd"
$nowTime = Get-Date -Format "yyyy-MM-dd HH:mm"
$snapId = "SNAP-" + (Get-Date -Format "yyyyMMdd-HHmm") + "-" + $PhaseLabel

function Split-List([string]$value) {
  if ([string]::IsNullOrWhiteSpace($value)) { return @() }
  return $value -split ';' | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne "" }
}

$nextList = Split-List $NextSteps
$blockersList = Split-List $Blockers
$filesList = Split-List $FilesTouched
$testsList = Split-List $Tests
$commandsList = Split-List $Commands

$stateLines = New-Object System.Collections.Generic.List[string]
$stateLines.Add('---')
$stateLines.Add('id: state')
$stateLines.Add("updated: $nowDate")
$stateLines.Add('owner: techlead')
$stateLines.Add('---')
$stateLines.Add('# State (resume point)')
$stateLines.Add('')
$stateLines.Add('## Snapshot')
$stateLines.Add("- ID: $snapId")
$stateLines.Add("- Timestamp: $nowTime")
$stateLines.Add("- Active ID: $ActiveId")
$stateLines.Add("- Phase: $Phase")
if (![string]::IsNullOrWhiteSpace($PhaseId)) {
  $stateLines.Add("- Active Phase: $PhaseId")
}
$stateLines.Add("- Status: $Status")
$stateLines.Add("- Defect ID (if any): $DefectId")
$stateLines.Add("- PhaseLabel: $PhaseLabel")
$stateLines.Add('')
$stateLines.Add('## Next Steps')

if ($nextList.Count -eq 0) {
  $stateLines += '1)'
  $stateLines += '2)'
  $stateLines += '3)'
} else {
  $i = 1
  foreach ($item in $nextList) {
    $stateLines += ("$i) $item")
    $i++
  }
}

$stateLines += ''
$stateLines += '## Blockers / Questions'
if ($blockersList.Count -eq 0) { $stateLines += '- ' } else { $blockersList | ForEach-Object { $stateLines += "- $_" } }
$stateLines += ''
$stateLines += '## Files Touched'
if ($filesList.Count -eq 0) { $stateLines += '- ' } else { $filesList | ForEach-Object { $stateLines += "- $_" } }
$stateLines += ''
$stateLines += '## Commands Run'
if ($commandsList.Count -eq 0) { $stateLines += '- ' } else { $commandsList | ForEach-Object { $stateLines += "- $_" } }
$stateLines += ''
$stateLines += '## Tests'
if ($testsList.Count -eq 0) { $stateLines += '- ' } else { $testsList | ForEach-Object { $stateLines += "- $_" } }
$stateLines += ''
$stateLines += '## Dependency Matrix (short)'
$stateLines += '| ID | DependsOn | Status | Notes |'
$stateLines += '| -- | --------- | ------ | ----- |'
$stateLines += ''
$stateLines += '## History'
$stateLines += "- Append-only log: .memory/PHASES/$PhaseId/STATE_HISTORY.md"
$stateLines += ''
$stateLines += '## Notes'
$stateLines += '- '

Write-Utf8NoBomLines $StatePath $stateLines

if (!(Test-Path -LiteralPath $StateHistoryPath)) {
  $historyHeader = @(
    '---',
    'id: state_history',
    "updated: $nowDate",
    'owner: techlead'
  )
  if (![string]::IsNullOrWhiteSpace($PhaseId)) {
    $historyHeader += "phase: $PhaseId"
  }
  $historyHeader += @(
    '---',
    '# State History (append-only)',
    ''
  )
  Write-Utf8NoBomLines $StateHistoryPath $historyHeader
}

$historyLines = New-Object System.Collections.Generic.List[string]
$historyLines.Add('')
$historyLines.Add('SNAPSHOT:')
$historyLines.Add("- ID: $snapId")
$historyLines.Add("- Timestamp: $nowTime")
$historyLines.Add("- Active ID: $ActiveId")
$historyLines.Add("- Phase: $Phase")
if (![string]::IsNullOrWhiteSpace($PhaseId)) {
  $historyLines.Add("- Active Phase: $PhaseId")
}
$historyLines.Add("- Status: $Status")
$historyLines.Add("- Defect ID: $DefectId")
$historyLines.Add("- PhaseLabel: $PhaseLabel")
$historyLines.Add("- Next: $($nextList -join ' | ')")
$historyLines.Add("- Blockers: $($blockersList -join ' | ')")
$historyLines.Add("- Files: $($filesList -join ' | ')")
$historyLines.Add("- Tests: $($testsList -join ' | ')")
$historyLines.Add("- Commands: $($commandsList -join ' | ')")
$historyLines.Add("- Notes: $Summary")
Append-Utf8NoBomLines $StateHistoryPath $historyLines

if (!(Test-Path -LiteralPath $WorklogPath)) {
  $worklogHeader = @('---','id: worklog',"updated: $nowDate")
  if (![string]::IsNullOrWhiteSpace($PhaseId)) {
    $worklogHeader += "phase: $PhaseId"
  }
  $worklogHeader += '---'
  Write-Utf8NoBomLines $WorklogPath $worklogHeader
}

if ([string]::IsNullOrWhiteSpace($Summary)) {
  $Summary = "Checkpoint snapshot recorded ($snapId)"
}

Append-Utf8NoBomLines $WorklogPath @("- $nowTime [$PhaseLabel]: $Summary")

if ($Checkpoint.IsPresent) {
  if (!(Test-Path -LiteralPath $ProgressPath)) {
    Write-Utf8NoBomLines $ProgressPath @('---','id: progress',"updated: $nowDate",'---','# Progress (changelog — одна строка на событие)')
  }
  Append-Utf8NoBomLines $ProgressPath @("${nowDate}: $Summary")
}

Write-Host "OK: $snapId ($PhaseLabel)"


