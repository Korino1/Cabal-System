param(
  [Parameter(Mandatory = $true)]
  [string]$SpecPath,
  [switch]$AllowMissingHash
)

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $scriptDir "..")).Path
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)

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

function Get-FileLines([string]$fullPath) {
  if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) {
    throw "File not found: $fullPath"
  }
  return [string[]](Get-Content -Path $fullPath -Encoding UTF8)
}

function Write-FileLines([string]$fullPath, [string[]]$lines) {
  if ($lines.Count -eq 0) {
    [System.IO.File]::WriteAllText($fullPath, "", $utf8NoBom)
    return
  }

  $text = [string]::Join("`n", $lines) + "`n"
  [System.IO.File]::WriteAllText($fullPath, $text, $utf8NoBom)
}

function Get-RangeText([string[]]$lines, [int]$startLine, [int]$endLine) {
  if ($lines.Count -eq 0) {
    return ""
  }

  $selected = New-Object System.Collections.Generic.List[string]
  for ($i = $startLine - 1; $i -le $endLine - 1; $i++) {
    $selected.Add([string]$lines[$i])
  }

  $text = [string]::Join("`n", $selected)
  if ($selected.Count -gt 0) {
    $text += "`n"
  }
  return $text
}

function Validate-Range([string[]]$lines, [int]$startLine, [int]$endLine, [string]$label) {
  if ($lines.Count -eq 0) {
    throw "$label is invalid for empty file"
  }
  if ($startLine -lt 1) {
    throw "$label start_line must be >= 1"
  }
  if ($endLine -lt $startLine) {
    throw "$label end_line must be >= start_line"
  }
  if ($endLine -gt $lines.Count) {
    throw "$label end_line is out of range ($endLine > $($lines.Count))"
  }
}

function Normalize-NewLines($operation) {
  if ($null -ne $operation.new_lines) {
    return [string[]]$operation.new_lines
  }

  if ($null -eq $operation.new_text) {
    throw "Operation '$($operation.op)' requires new_text or new_lines"
  }

  $text = [string]$operation.new_text
  $split = $text -split "`r?`n", -1
  if ($split.Count -gt 0 -and $split[$split.Count - 1] -eq "") {
    $split = $split[0..($split.Count - 2)]
  }
  return [string[]]$split
}

function Require-ExpectedHash($operation) {
  if ($AllowMissingHash.IsPresent) {
    return
  }
  if ([string]::IsNullOrWhiteSpace([string]$operation.expected_hash)) {
    throw "Operation '$($operation.op)' requires expected_hash"
  }
}

function Assert-Hash([string]$actualHash, [string]$expectedHash, [string]$context) {
  if ([string]::IsNullOrWhiteSpace($expectedHash)) {
    return
  }

  if (-not $actualHash.Equals($expectedHash.ToLowerInvariant(), [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Hash mismatch at $context. expected=$expectedHash actual=$actualHash"
  }
}

try {
  $specFullPath = Resolve-RepoFile $SpecPath
  if (-not (Test-Path -LiteralPath $specFullPath -PathType Leaf)) {
    throw "Spec file not found: $SpecPath"
  }

  $specRaw = Get-Content -Path $specFullPath -Raw -Encoding UTF8
  $spec = $specRaw | ConvertFrom-Json
  if ($null -eq $spec.operations -or $spec.operations.Count -eq 0) {
    throw "Spec must contain non-empty operations array"
  }

  $opIndex = 0
  foreach ($operation in $spec.operations) {
    $opIndex++
    if ([string]::IsNullOrWhiteSpace([string]$operation.op)) {
      throw "Operation #$opIndex has no op"
    }

    $op = [string]$operation.op
    $fullPath = Resolve-RepoFile ([string]$operation.path)
    $relativePath = Get-RepoRelativePath $fullPath
    $lines = [System.Collections.Generic.List[string]]::new()
    foreach ($line in (Get-FileLines $fullPath)) {
      $lines.Add($line)
    }

    Require-ExpectedHash $operation

    switch ($op) {
      "replace_range" {
        $startLine = [int]$operation.start_line
        $endLine = [int]$operation.end_line
        Validate-Range -lines $lines.ToArray() -startLine $startLine -endLine $endLine -label "replace_range"

        $rangeText = Get-RangeText -lines $lines.ToArray() -startLine $startLine -endLine $endLine
        $actualHash = Compute-Sha256Hex $rangeText
        Assert-Hash -actualHash $actualHash -expectedHash ([string]$operation.expected_hash) -context ("{0}:{1}-{2}" -f $relativePath, $startLine, $endLine)

        $newLines = Normalize-NewLines $operation
        $removeCount = $endLine - $startLine + 1
        $lines.RemoveRange($startLine - 1, $removeCount)
        $lines.InsertRange($startLine - 1, [string[]]$newLines)

        Write-FileLines -fullPath $fullPath -lines $lines.ToArray()
        Write-Host ("[{0}] replace_range {1}:{2}-{3} OK" -f $opIndex, $relativePath, $startLine, $endLine)
      }
      "insert_after" {
        $afterLine = [int]$operation.after_line
        if ($afterLine -lt 0 -or $afterLine -gt $lines.Count) {
          throw "insert_after after_line is out of range ($afterLine, file lines: $($lines.Count))"
        }

        $anchorText = ""
        if ($afterLine -gt 0) {
          $anchorText = [string]$lines[$afterLine - 1] + "`n"
        }
        $actualHash = Compute-Sha256Hex $anchorText
        Assert-Hash -actualHash $actualHash -expectedHash ([string]$operation.expected_hash) -context ("{0}:after_line={1}" -f $relativePath, $afterLine)

        $newLines = Normalize-NewLines $operation
        $lines.InsertRange($afterLine, [string[]]$newLines)

        Write-FileLines -fullPath $fullPath -lines $lines.ToArray()
        Write-Host ("[{0}] insert_after {1}:after {2} OK" -f $opIndex, $relativePath, $afterLine)
      }
      "delete_range" {
        $startLine = [int]$operation.start_line
        $endLine = [int]$operation.end_line
        Validate-Range -lines $lines.ToArray() -startLine $startLine -endLine $endLine -label "delete_range"

        $rangeText = Get-RangeText -lines $lines.ToArray() -startLine $startLine -endLine $endLine
        $actualHash = Compute-Sha256Hex $rangeText
        Assert-Hash -actualHash $actualHash -expectedHash ([string]$operation.expected_hash) -context ("{0}:{1}-{2}" -f $relativePath, $startLine, $endLine)

        $removeCount = $endLine - $startLine + 1
        $lines.RemoveRange($startLine - 1, $removeCount)

        Write-FileLines -fullPath $fullPath -lines $lines.ToArray()
        Write-Host ("[{0}] delete_range {1}:{2}-{3} OK" -f $opIndex, $relativePath, $startLine, $endLine)
      }
      default {
        throw "Unsupported operation '$op' at index $opIndex"
      }
    }
  }

  Write-Host ("OK: applied {0} operations" -f $spec.operations.Count)
  exit 0
} catch {
  Write-Error $_
  exit 1
}
