# CABAL Role-Scoped Tool Access Plan

Updated: 2026-02-25  
Status: in_progress_implemented_core

## 1. Цель
Снизить когнитивную перегрузку модели и риск неверного вызова MCP-инструмента за счёт выдачи ограниченного набора инструментов по активной роли агента, сохранив безопасный и трассируемый механизм перехода между ролями.

## 2. Что было прочитано перед планом (роли агентов)
Роли и обязанности взяты из:
- `agent/orchestrator.md`
- `agent/global-architect.md`
- `agent/architect.md`
- `agent/conceptualizer.md`
- `agent/mathematician.md`
- `agent/integrator-runtime.md`
- `agent/rust-engineer.md`
- `agent/simd-specialist.md`
- `agent/debuger.md`
- `agent/fixer.md`
- `agent/qa-agent.md`
- `agent/tester.md`

Примечание: папки `Roocode`/`Kilocode` исключены из этого плана (по решению пользователя).

## 3. Текущее состояние (as-is)
Сейчас всем ролям выдан практически одинаковый рабочий набор (MCP-only контракт в agent-файлах):
- `cabalget_state`
- `cabalget_gate_policy`
- `cabalget_cross_rules_status`
- `cabalack_cross_rules`
- `cabalgate_check`
- `cabaltransition_phase_strict`
- `cabalplan_task_execution`
- `cabalroute_consult`
- `cabalevaluate_patch_gate`
- `cabalproxy_execute`
- `cabalregister_evidence`
- `cabalrecord_event`

Проблема: одинаковый набор для всех ролей не отражает реальные границы обязанностей и увеличивает вероятность неверного выбора инструмента.

## 4. Целевая карта доступа по ролям (to-be)
Ниже — целевой профиль выдачи инструментов в `tools/list` для активной роли.

### 4.1 Базовый набор для всех ролей (минимум)
- `cabalget_state`
- `cabalget_cross_rules_status`
- `cabalack_cross_rules`
- `cabalgate_check`
- `cabaltransition_phase_strict`
- `cabalroute_consult`
- `cabalregister_evidence`
- `cabalrecord_event`

### 4.2 Ролевые надстройки
`orchestrator`
- `cabalset_consult_mode`
- `cabalset_consult_guard_policy`
- `cabalset_consult_routing_rule`
- `cabalset_consult_priority_timeout`
- `cabalset_consult_retry_limit`
- `cabalset_consult_escalation_target`
- `cabalset_consult_allowed_roles`
- `cabalset_adaptive_router`
- `cabalset_adaptive_exploration_policy`
- `cabalapply_policy_bundle` (только если включён policy-security workflow)

`global_architect`, `architect`
- `cabalclassify_task`
- `cabalget_budget_policy`
- `cabalplan_task_execution`
- `cabalevaluate_patch_gate`

`conceptualizer`
- `cabalclassify_task`
- `cabalget_budget_policy`
- `cabalget_patch_gate_policy`

`mathematician`
- `cabalclassify_task`
- `cabalget_budget_policy`
- `cabalget_patch_gate_policy`

`integrator_runtime`
- `cabalclassify_task`
- `cabalplan_task_execution`
- `cabalget_proxy_operation_policy`
- `cabalproxy_request`
- `cabalproxy_execute`

`rust_engineer`, `simd_specialist`
- `cabalget_cpu_policy`
- `cabalclassify_task`
- `cabalplan_task_execution`
- `cabalevaluate_patch_gate`
- `cabalproxy_execute`

`debuger`
- `cabalclassify_task`
- `cabalplan_task_execution`
- `cabalget_proxy_log`
- `cabalquery_audit_log`

`fixer`
- `cabalclassify_task`
- `cabalplan_task_execution`
- `cabalevaluate_patch_gate`
- `cabalproxy_execute`

`qa_agent`, `tester`
- `cabalquery_audit_log`
- `cabalexport_audit_log`
- `cabalreplay_audit_state`
- `cabalaudit_health_check`
- `cabalevaluate_patch_gate`

## 5. Инструменты вне ролей: контур перехода/переключения роли
Это отдельный “control-plane”, не относящийся к предметной задаче конкретной роли.

### 5.1 Обязательные инструменты перехода
Новые (добавить в runtime):
- `cabalget_role_profile` — текущая роль и источник назначения.
- `caballist_role_profiles` — допустимые роли и их ограничения.
- `cabalrequest_role_switch` — запрос на смену роли (в очередь/на согласование).
- `cabalapprove_role_switch` — подтверждение смены (только orchestrator/user policy).
- `cabalset_role_profile` — фактическая смена роли (guarded, audit-only).

### 5.2 Guard-условия для смены роли
- Успешный `cabalgate_check(kind=entry/exit)` для текущей фазы.
- Подтверждённые cross-rules (`cabalget_cross_rules_status` + при необходимости `cabalack_cross_rules`).
- Отсутствие незакрытого deny по patch-gate для текущего change-set.
- Обязательная запись в audit:
  - `kind=role.switch.requested|approved|applied|rejected`
  - `from_role`, `to_role`, `reason`, `actor`, `policy_revision`.

## 6. План реализации
1. Ввести модель role-profile в runtime state.
2. Реализовать фильтрацию `tools/list` по `active_role`.
3. Добавить инструменты control-plane для role-switch (раздел 5.1).
4. Реализовать guard-policy для role-switch (раздел 5.2).
5. Синхронизировать агентские контракты в `agent/*.md` с role-scoped моделью.
6. Добавить тесты:
   - role-aware `tools/list`,
   - запрет вызова tool вне роли (`POLICY_DENY`),
   - успешный/неуспешный role-switch,
   - корректный audit trail.
7. Прогнать E2E в Roo (формат имён tools: Roo-compatible) и зафиксировать отчёт.

## 7. Критерии готовности
- Для каждой роли выдаётся только её whitelist + базовый набор.
- Попытка вызвать неразрешённый для роли инструмент завершается policy-ошибкой.
- Смена роли работает только через control-plane и пишется в audit.
- Количество видимых инструментов для обычной роли уменьшено минимум на 40% относительно текущего полного списка.

## 8. Риски и меры
- Риск: слишком узкие профили блокируют работу.
  - Мера: emergency-fallback через `cabalrequest_role_switch` + `cabalapprove_role_switch`.
- Риск: рассинхрон runtime и agent-инструкций.
  - Мера: единственный source-of-truth в runtime + автопроверка doc parity.
- Риск: регрессия в IDE-клиентах из-за формата имён tool.
  - Мера: сохранять Roo-совместимый формат имён в `tools/list` и покрыть transport E2E.

## 9. Статус реализации (2026-02-25)
Сделано:
- В runtime добавлена модель role-profile: `active_role_profile`, `role_tool_access_profiles`, `pending_role_switch`.
- Реализованы control-plane инструменты:
  - `cabal.get_role_profile`
  - `cabal.list_role_profiles`
  - `cabal.request_role_switch`
  - `cabal.approve_role_switch`
  - `cabal.set_role_profile`
- Включена проверка доступа на вызов инструмента по активной роли (`policy deny` для неразрешённых tool).
- `tools/list` теперь role-aware и возвращает ограниченный набор по активной роли.
- Для роли `orchestrator` оставлен расширенный набор (операционный суперпрофиль), чтобы не ломать runtime/e2e-потоки.
- Добавлены unit-тесты role-switch и role-filtering.
- Прогон тестов `cabal-mcp-runtime`: PASS.

Осталось:
- Синхронизировать `agent/*.md` с role-scoped моделью как source-of-truth runtime.
- Подготовить отдельный e2e-отчёт по Roo-сценарию с переключением ролей в реальной IDE-сессии.
