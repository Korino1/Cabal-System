param(
    [switch]$WithIntegration
)

$ErrorActionPreference = "Stop"

Write-Host "[cabal] running IDE contract gate..." -ForegroundColor Cyan
$projectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $projectRoot

try {
    $stdioTests = @(
        "mcp_stdio_initialize_tracks_vscode_ide_profile",
        "mcp_stdio_initialize_profile_enforcement_allows_jetbrains_client",
        "mcp_stdio_gate_policy_strict_artifacts_affects_gate_report",
        "mcp_stdio_route_consult_audit_contract_fields_present_for_jetbrains_profile",
        "mcp_stdio_ack_cross_rules_updates_status_and_unblocks_consult",
        "mcp_stdio_route_consult_role_mismatch_fallback_and_escalation",
        "mcp_stdio_route_consult_adaptive_exploration_selects_undertrained_executor"
    )

    foreach ($testName in $stdioTests) {
        Write-Host "[cabal] cargo test --test mcp_stdio_e2e $testName -- --nocapture" -ForegroundColor DarkCyan
        & cargo test --test mcp_stdio_e2e $testName -- --nocapture
        if ($LASTEXITCODE -ne 0) {
            throw "[cabal] IDE contract gate failed at mcp_stdio_e2e::$testName (exit code $LASTEXITCODE)"
        }
    }

    if ($WithIntegration) {
        $integrationTests = @(
            "integration_route_consult_guard_requires_cross_rules_ack_evidence",
            "integration_ack_cross_rules_sets_status_and_unblocks_consult",
            "integration_route_consult_uses_policy_driven_matrix",
            "integration_route_consult_adaptive_exploration_uses_undertrained_executor"
        )
        foreach ($testName in $integrationTests) {
            Write-Host "[cabal] cargo test --test runtime_api $testName -- --nocapture" -ForegroundColor DarkCyan
            & cargo test --test runtime_api $testName -- --nocapture
            if ($LASTEXITCODE -ne 0) {
                throw "[cabal] IDE contract gate failed at runtime_api::$testName (exit code $LASTEXITCODE)"
            }
        }
    }
}
finally {
    Pop-Location
}

Write-Host "[cabal] IDE contract gate passed." -ForegroundColor Green
