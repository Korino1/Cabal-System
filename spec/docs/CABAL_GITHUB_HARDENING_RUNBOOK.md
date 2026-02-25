# CABAL GitHub Hardening Runbook

Цель: закрепить release-гейты Cabal как обязательные проверки ветки.

## 1) Локальный preflight
```powershell
powershell -ExecutionPolicy Bypass -File .\cabal-mcp-runtime\scripts\check-release-gates.ps1 -WithIntegration
powershell -ExecutionPolicy Bypass -File .\cabal-mcp-runtime\scripts\validate-release-gate-summary.ps1 -SummaryPath .\cabal-mcp-runtime\.cabal_runtime\release_gate_summary.json
```

## 2) Применить required status checks (GitHub branch protection)
```powershell
powershell -ExecutionPolicy Bypass -File .\cabal-mcp-runtime\scripts\set-required-stress-gate.ps1 -RepoOwner "<owner>" -RepoName "<repo>" -Branch "main" -UseCabalRecommendedChecks
```
Требования доступа:
- либо установлен `gh` CLI,
- либо задан `GITHUB_TOKEN` (или `GH_TOKEN`) с правами на branch protection.

Альтернатива: apply+verify одной командой:
```powershell
powershell -ExecutionPolicy Bypass -File .\cabal-mcp-runtime\scripts\apply-and-verify-branch-protection.ps1 -RepoOwner "<owner>" -RepoName "<repo>" -Branch "main" -UseCabalRecommendedChecks
```

Это включает:
- `stress-sla-gate`
- `ide-contract-gate`
- `ide-e2e-report-schema-gate`
- `release-summary-schema-gate`
- `release-gate`

## 3) Верифицировать, что checks реально применились
```powershell
powershell -ExecutionPolicy Bypass -File .\cabal-mcp-runtime\scripts\verify-required-status-checks.ps1 -RepoOwner "<owner>" -RepoName "<repo>" -Branch "main" -UseCabalRecommendedChecks
```

## 4) Manual release-gate run (реальный IDE E2E отчёт)
В GitHub Actions workflow `cabal-mcp-runtime-release-gate` указать:
- `ide_e2e_report_path`
- `require_real_ide_report=true`
- `real_ide_report_max_age_hours` (например, `24`)

## 5) Артефакты для аудита
- `cabal-release-gate-summary` artifact из workflow `cabal-mcp-runtime-release-gate`
- локальный `cabal-mcp-runtime/.cabal_runtime/release_gate_summary.json`

## 6) Единая команда финальной готовности
```powershell
powershell -ExecutionPolicy Bypass -File .\cabal-mcp-runtime\scripts\check-final-readiness.ps1 -IdeE2EReportPath .\spec\docs\ide_e2e_report.json -RepoOwner "<owner>" -RepoName "<repo>" -Branch "main"
```

С проверкой реальных IDE логов:
```powershell
powershell -ExecutionPolicy Bypass -File .\cabal-mcp-runtime\scripts\check-final-readiness.ps1 -IdeE2EReportPath .\spec\docs\ide_e2e_report.json -VsCodeLogPath .\spec\docs\ide_logs\vscode.log -JetBrainsLogPath .\spec\docs\ide_logs\jetbrains.log -RepoOwner "<owner>" -RepoName "<repo>" -Branch "main"
```

## 7) Валидация итогового final readiness summary
```powershell
powershell -ExecutionPolicy Bypass -File .\cabal-mcp-runtime\scripts\validate-final-readiness-summary.ps1 -SummaryPath .\cabal-mcp-runtime\.cabal_runtime\final_readiness_result.json
```
