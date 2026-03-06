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
$ownerPattern = '\[Owner:\s*[^\]]+\]'
$taskPattern = '^\s*-\s*\[[ x~]\]\s+.*$'
$bad = @()

foreach ($file in $taskFiles) {
  $lines = Get-Content -Encoding UTF8 $file
  foreach ($line in $lines) {
    if ($line -match $taskPattern) {
      if ($line -notmatch $ownerPattern) {
        $bad += "$file :: $line"
      }
    }
  }
}

if ($bad.Count -gt 0) {
  Write-Host 'Missing Owner in tasks:'
  $bad | ForEach-Object { Write-Host $_ }
  exit 1
}

Write-Host 'OK: all tasks contain Owner.'
exit 0

