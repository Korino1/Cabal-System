param(
    [Parameter(Mandatory = $true)]
    [string]$ReportPath,

    [string]$RunId = "",
    [string]$RuntimeVersion = "cabal-mcp-runtime"
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($RunId)) {
    $RunId = "ide-e2e-" + [guid]::NewGuid().ToString("N")
}

$report = [ordered]@{
    schema_version = "1.0.0"
    run_id = $RunId
    timestamp_utc = (Get-Date).ToUniversalTime().ToString("o")
    runtime_version = $RuntimeVersion
    profiles = @(
        [ordered]@{
            profile = "vscode"
            checks = [ordered]@{
                "IDE-P1" = $false
                "IDE-P2" = $false
                "IDE-P3" = $false
                "IDE-P4" = $false
                "IDE-P5" = $false
            }
            notes = "fill with VS Code execution details"
        },
        [ordered]@{
            profile = "jetbrains"
            checks = [ordered]@{
                "IDE-P1" = $false
                "IDE-P2" = $false
                "IDE-P3" = $false
                "IDE-P4" = $false
                "IDE-P5" = $false
            }
            notes = "fill with JetBrains execution details"
        }
    )
}

$dir = Split-Path -Parent $ReportPath
if (-not [string]::IsNullOrWhiteSpace($dir)) {
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
}

$report | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 -Path $ReportPath
Write-Host "[cabal] IDE E2E report template written: $ReportPath" -ForegroundColor Green
