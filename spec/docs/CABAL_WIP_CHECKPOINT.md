# CABAL WIP Checkpoint

- checkpoint_id: `WIP-2026-02-25-002`
- status: `ready_for_testing (implementation complete)`
- scope: `Cabal MCP Runtime + TaskClassifier/BudgetController/PatchGate + release-gate orchestration`

## Что зафиксировано на чекпоинте
- Runtime/transport/tests проходят локально.
- Unified release gates реализованы и выдают machine-readable summary.
- Реализованы schema-gates для IDE E2E report и release summary.
- Реализован strict режим real IDE report (`RequireRealIdeReport` + freshness check).
- Реализованы скрипты branch protection apply/verify.
- Добавлены MCP tools:
  - `cabal.classify_task`
  - `cabal.get_budget_policy`
  - `cabal.set_budget_policy`
  - `cabal.plan_task_execution`
  - `cabal.get_patch_gate_policy`
  - `cabal.set_patch_gate_policy`
  - `cabal.evaluate_patch_gate`
- `route_consult` расширен `task_profile` и пишет его в audit.

## Что ещё НЕ закрыто (блокеры финала)
1. Live rollout в целевом GitHub-репозитории:
  - применить branch protection на `main`;
  - подтвердить required checks на живом API (не snapshot).
2. Пользовательский real IDE E2E на целевых MCP-клиентах (VS Code/JetBrains) с загрузкой реальных логов.

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
