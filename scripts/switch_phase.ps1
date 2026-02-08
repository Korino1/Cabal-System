param(
  [string]$Current = "",
  [string]$Next = ""
)

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $scriptDir "..")).Path
$globalIndexPath = Join-Path $repoRoot ".memory\\GLOBAL_INDEX.md"

if ([string]::IsNullOrWhiteSpace($Current) -or [string]::IsNullOrWhiteSpace($Next)) {
  Write-Error "Current and Next are required"
  exit 2
}

if (!(Test-Path -LiteralPath $globalIndexPath)) {
  Write-Error "GLOBAL_INDEX.md not found: $globalIndexPath"
  exit 2
}

$text = Get-Content -Raw -Encoding UTF8 $globalIndexPath
$text = $text -replace '\| ' + [regex]::Escape($Current) + ' \|([^\n]+)\|', ('| ' + $Current + ' |$1|')
$text = $text -replace "\| $Current \|([^|]+)\|[^|]+\|", "| $Current |`$1| done |"
$text = $text -replace "\| $Next \|([^|]+)\|[^|]+\|", "| $Next |`$1| in_progress |"
$text = $text -replace 'updated: \d{4}-\d{2}-\d{2}', "updated: $(Get-Date -Format 'yyyy-MM-dd')"
$text = $text -replace '## Active Phase\s*- ID: .*\s*- Path: .*', "## Active Phase`r`n- ID: $Next`r`n- Path: .memory/PHASES/$Next"
Set-Content -Encoding UTF8 -Path $globalIndexPath -Value $text

Write-Host "OK: Active Phase switched $Current -> $Next"
exit 0
