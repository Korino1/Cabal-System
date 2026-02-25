# CABAL WIP Checkpoint

- checkpoint_id: `WIP-2026-02-25-001`
- status: `in_progress (not final)`
- scope: `Cabal MCP Runtime hardening + release-gate formalization`

## Что зафиксировано на чекпоинте
- Runtime/transport/tests проходят локально.
- Unified release gates реализованы и выдают machine-readable summary.
- Реализованы schema-gates для IDE E2E report и release summary.
- Реализован strict режим real IDE report (`RequireRealIdeReport` + freshness check).
- Реализованы скрипты branch protection apply/verify.

## Что ещё НЕ закрыто (блокеры финала)
1. Реальный IDE E2E прогон на целевых MCP-клиентах:
  - VS Code
  - JetBrains
2. Применение branch protection на GitHub и подтверждение required checks:
  - `stress-sla-gate`
  - `ide-contract-gate`
  - `ide-e2e-report-schema-gate`
  - `release-summary-schema-gate`
  - `release-gate`

## Критерий выхода из чекпоинта
- Получен валидный real IDE E2E report.
- Branch protection применён и подтверждён в целевом репозитории.
- Повторный запуск release gate в strict режиме (`RequireRealIdeReport`) завершён `PASS`.

## Команды закрытия чекпоинта
- Применить и проверить branch protection:
  - `cabal-mcp-runtime/scripts/apply-and-verify-branch-protection.ps1`
- Проверить real IDE артефакты:
  - `cabal-mcp-runtime/scripts/validate-real-ide-e2e-artifacts.ps1`
- Финальная оркестрация:
  - `cabal-mcp-runtime/scripts/check-final-readiness.ps1`
