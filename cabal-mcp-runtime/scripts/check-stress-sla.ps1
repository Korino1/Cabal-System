$ErrorActionPreference = "Stop"

Write-Host "[cabal] running stress SLA gate..." -ForegroundColor Cyan
$cmd = "cargo test --test runtime_stress -- --ignored --nocapture"
Write-Host "[cabal] $cmd" -ForegroundColor DarkCyan

$projectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $projectRoot
try {
    & cargo test --test runtime_stress -- --ignored --nocapture
    $code = $LASTEXITCODE
    if ($code -ne 0) {
        throw "[cabal] stress SLA gate failed with exit code $code"
    }
}
finally {
    Pop-Location
}

Write-Host "[cabal] stress SLA gate passed." -ForegroundColor Green
