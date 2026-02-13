param(
  [Parameter(Mandatory = $true)]
  [string]$Path,
  [int]$StartLine = 1,
  [int]$EndLine = 0,
  [switch]$AsJson
)

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $scriptDir "..")).Path

function Resolve-RepoFile([string]$inputPath) {
  if ([string]::IsNullOrWhiteSpace($inputPath)) {
    throw "Path is required"
  }

  if ([System.IO.Path]::IsPathRooted($inputPath)) {
    $fullPath = [System.IO.Path]::GetFullPath($inputPath)
  } else {
    $fullPath = [System.IO.Path]::GetFullPath((Join-Path $repoRoot $inputPath))
  }

  $rootWithSep = $repoRoot
  if (-not $rootWithSep.EndsWith([System.IO.Path]::DirectorySeparatorChar)) {
    $rootWithSep += [System.IO.Path]::DirectorySeparatorChar
  }

  $cmp = [System.StringComparison]::OrdinalIgnoreCase
  if (-not ($fullPath.Equals($repoRoot, $cmp) -or $fullPath.StartsWith($rootWithSep, $cmp))) {
    throw "Path is outside repository root: $inputPath"
  }

  return $fullPath
}

function Get-RepoRelativePath([string]$fullPath) {
  $rootUri = New-Object System.Uri(($repoRoot.TrimEnd('\') + '\'))
  $fileUri = New-Object System.Uri($fullPath)
  $relativeUri = $rootUri.MakeRelativeUri($fileUri)
  return ([System.Uri]::UnescapeDataString($relativeUri.ToString())).Replace('\', '/')
}
function Compute-Sha256Hex([string]$text) {
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($text)
    $hash = $sha.ComputeHash($bytes)
    return ([System.BitConverter]::ToString($hash)).Replace("-", "").ToLowerInvariant()
  } finally {
    $sha.Dispose()
  }
}

function Get-RangeText([string[]]$lines, [int]$start, [int]$end) {
  if ($lines.Count -eq 0) {
    return ""
  }

  $selected = New-Object System.Collections.Generic.List[string]
  for ($i = $start - 1; $i -le $end - 1; $i++) {
    $selected.Add([string]$lines[$i])
  }

  $text = [string]::Join("`n", $selected)
  if ($selected.Count -gt 0) {
    $text += "`n"
  }

  return $text
}

try {
  $fullPath = Resolve-RepoFile $Path
  if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) {
    throw "File not found: $Path"
  }

  $lines = Get-Content -Path $fullPath -Encoding UTF8
  $totalLines = $lines.Count

  if ($totalLines -eq 0) {
    if ($StartLine -ne 1) {
      throw "StartLine must be 1 for empty file"
    }
    if ($EndLine -ne 0) {
      throw "EndLine must be 0 for empty file"
    }
    $effectiveStart = 1
    $effectiveEnd = 0
  } else {
    if ($StartLine -lt 1) {
      throw "StartLine must be >= 1"
    }

    $effectiveStart = $StartLine
    $effectiveEnd = if ($EndLine -le 0) { $totalLines } else { $EndLine }

    if ($effectiveStart -gt $totalLines) {
      throw "StartLine is out of range ($effectiveStart > $totalLines)"
    }
    if ($effectiveEnd -lt $effectiveStart) {
      throw "EndLine must be >= StartLine"
    }
    if ($effectiveEnd -gt $totalLines) {
      throw "EndLine is out of range ($effectiveEnd > $totalLines)"
    }
  }

  $rangeText = Get-RangeText -lines $lines -start $effectiveStart -end $effectiveEnd
  $rangeHash = Compute-Sha256Hex $rangeText
  $relativePath = Get-RepoRelativePath $fullPath

  if ($AsJson.IsPresent) {
    $rangeLines = @()
    if ($totalLines -gt 0 -and $effectiveEnd -ge $effectiveStart) {
      for ($i = $effectiveStart - 1; $i -le $effectiveEnd - 1; $i++) {
        $rangeLines += [string]$lines[$i]
      }
    }

    $payload = [ordered]@{
      path = $relativePath
      start_line = $effectiveStart
      end_line = $effectiveEnd
      total_lines = $totalLines
      range_hash = $rangeHash
      range_lines = $rangeLines
    }
    $payload | ConvertTo-Json -Depth 10
    exit 0
  }

  Write-Host ("Path: {0}" -f $relativePath)
  Write-Host ("Range: {0}-{1} of {2}" -f $effectiveStart, $effectiveEnd, $totalLines)
  Write-Host ("SHA256: {0}" -f $rangeHash)

  if ($totalLines -eq 0 -or $effectiveEnd -lt $effectiveStart) {
    Write-Host "<empty range>"
    exit 0
  }

  $width = [Math]::Max(4, $effectiveEnd.ToString().Length)
  for ($lineNo = $effectiveStart; $lineNo -le $effectiveEnd; $lineNo++) {
    $line = [string]$lines[$lineNo - 1]
    Write-Host (("{0}| {1}" -f $lineNo.ToString().PadLeft($width, '0'), $line))
  }

  exit 0
} catch {
  Write-Error $_
  exit 1
}
