param(
  [string]$TasksPath = ".memory\\TASKS.md"
)

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $scriptDir "..")).Path
if (![System.IO.Path]::IsPathRooted($TasksPath)) {
  $TasksPath = Join-Path $repoRoot $TasksPath
}

$taskFiles = @()
if (Test-Path -LiteralPath $TasksPath) {
  $taskFiles += $TasksPath
}

$phaseRoot = Join-Path $repoRoot ".memory\\PHASES"
if (Test-Path -LiteralPath $phaseRoot) {
  $taskFiles += Get-ChildItem -Path $phaseRoot -Filter "TASKS.md" -Recurse -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName }
}

if ($taskFiles.Count -eq 0) {
  Write-Error "TASKS.md not found: $TasksPath"
  exit 2
}
$defectPattern = 'DEF-\d{8}-\d{3}'
# Keep script ASCII-only for Windows PowerShell 5.1 compatibility (UTF-8 without BOM).
$fixPattern = '(?i)(fix|bug|defect|\u0438\u0441\u043f\u0440\u0430\u0432|\u0434\u0435\u0444\u0435\u043a\u0442)'
$bad = @()

foreach ($file in $taskFiles) {
  $lines = Get-Content -Encoding UTF8 $file
  foreach ($line in $lines) {
    if ($line -match 'Template' -or $line -match 'BASE') {
      continue
    }
    if ($line -match '^\s*-\s*\[[ x~]\]\s+' -and $line -match $fixPattern) {
      if ($line -notmatch $defectPattern) {
        $bad += "$file :: $line"
      }
    }
  }
}

if ($bad.Count -gt 0) {
  Write-Host 'Missing DEFECT ID in fix tasks:'
  $bad | ForEach-Object { Write-Host $_ }
  exit 1
}

Write-Host 'OK: all fix tasks contain DEFECT ID.'
exit 0

