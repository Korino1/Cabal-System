# Cabal MCP Runtime (Rust, Nightly, Zen4-first)

`Cabal MCP Runtime` — программный control plane для Cabal в модели `MCP-only`.

## Требования
- Rust nightly (`edition = 2024`)
- CPU с поддержкой `AVX2` (минимум)
- Быстрый путь: `AVX-512F`, `AVX-512VL`, `FMA`, `BMI2`, `SHA` (Zen4-профиль)

## Политика CPU
- Ниже AVX2 запуск запрещён.
- Если CPU совпадает с Zen4-профилем, используется AVX-512 путь.
- Для остальных CPU с AVX2 используется generic AVX2 путь.

## Сборка
```powershell
cargo build --release
```

## Запуск
```powershell
cargo run --release
```

## Тесты
```powershell
cargo test
```

Опциональный stress-профиль audit (большие журналы):
```powershell
cargo test --test runtime_stress -- --ignored --nocapture
```
Профиль включает single-run и multi-run (`p95/p99`) сценарии для `query/export/replay`.
SLA пороги зафиксированы в `../spec/docs/CABAL_STRESS_SLA.md`.

Release-gate проверка stress SLA:
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check-stress-sla.ps1
```
CI workflow: `.github/workflows/cabal-mcp-runtime-stress-gate.yml`.

IDE transport/profile contract gate:
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check-ide-contract-gate.ps1 -WithIntegration
```
CI workflow: `.github/workflows/cabal-mcp-runtime-ide-contract-gate.yml`.

Unified release gates (stress + IDE contract):
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check-release-gates.ps1 -WithIntegration
```
Скрипт дополнительно проверяет schema-contract для `IDE_E2E_REPORT`.
По умолчанию используется fixture `../spec/contracts/ide_e2e_report.pass.json`; для реального отчёта передайте `-IdeE2EReportPath`.
Machine-readable summary сохраняется в `.cabal_runtime/release_gate_summary.json` (можно переопределить `-SummaryPath`).

Пример c кастомным summary-path:
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check-release-gates.ps1 -WithIntegration -SummaryPath .\tmp\release_gate_summary.json
```

Пример для реального IDE E2E отчёта (без fallback на fixture):
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check-release-gates.ps1 -WithIntegration -IdeE2EReportPath .\spec\docs\ide_e2e_report.json -RequireRealIdeReport
```
Создание шаблона реального IDE E2E отчёта:
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\new-ide-e2e-report.ps1 -ReportPath .\..\spec\docs\ide_e2e_report.json
```
Шаблон создаётся с `IDE-P1..IDE-P5=false`; после реальной проверки нужно отметить все пройденные пункты как `true`.
В strict режиме `-RequireRealIdeReport` отчёт проверяется на свежесть (по умолчанию `72` часа).
Порог можно переопределить:
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check-release-gates.ps1 -WithIntegration -IdeE2EReportPath .\spec\docs\ide_e2e_report.json -RequireRealIdeReport -RealIdeReportMaxAgeHours 24
```

Валидация release summary:
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\validate-release-gate-summary.ps1 -SummaryPath .\.cabal_runtime\release_gate_summary.json
```

Manual CI workflow: `.github/workflows/cabal-mcp-runtime-release-gate.yml`.
Workflow inputs:
- `ide_e2e_report_path` (default: `spec/contracts/ide_e2e_report.pass.json`)
- `require_real_ide_report` (`true` включает строгий режим без fallback на fixture)
- `real_ide_report_max_age_hours` (default: `72`, используется только при `require_real_ide_report=true`)

GitHub hardening runbook: `../spec/docs/CABAL_GITHUB_HARDENING_RUNBOOK.md`.

Закрепление workflow как required status check (GitHub branch protection):
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\set-required-stress-gate.ps1 -RepoOwner "<owner>" -RepoName "<repo>" -Branch "main" -StatusCheck "stress-sla-gate" -AdditionalStatusChecks "ide-contract-gate","ide-e2e-report-schema-gate","release-summary-schema-gate","release-gate"
```

Упрощённый режим с рекомендованным набором Cabal-checks:
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\set-required-stress-gate.ps1 -RepoOwner "<owner>" -RepoName "<repo>" -Branch "main" -UseCabalRecommendedChecks
```

Проверка, что required status checks реально применились в branch protection:
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-required-status-checks.ps1 -RepoOwner "<owner>" -RepoName "<repo>" -Branch "main" -UseCabalRecommendedChecks
```

Финальная проверка готовности (strict release gate + summary validation + branch protection verification):
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check-final-readiness.ps1 -IdeE2EReportPath .\..\spec\docs\ide_e2e_report.json -RepoOwner "<owner>" -RepoName "<repo>" -Branch "main"
```
Опционально можно включить проверку реальных IDE логов:
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check-final-readiness.ps1 -IdeE2EReportPath .\..\spec\docs\ide_e2e_report.json -VsCodeLogPath .\..\spec\docs\ide_logs\vscode.log -JetBrainsLogPath .\..\spec\docs\ide_logs\jetbrains.log -RepoOwner "<owner>" -RepoName "<repo>" -Branch "main"
```

Шаблоны IDE-адаптеров MCP (VS Code/JetBrains):
- `../spec/examples/ide/README.md`
- `../spec/examples/ide/vscode.mcp.jsonc`
- `../spec/examples/ide/jetbrains.mcp.jsonc`
- `../spec/docs/CABAL_IDE_E2E_CHECKLIST.md`
- `../spec/contracts/IDE_E2E_REPORT.schema.json`
- `../spec/contracts/ide_e2e_report.pass.json`
- `../spec/contracts/RELEASE_GATE_SUMMARY.schema.json`
- `../spec/contracts/release_gate_summary.pass.json`
- `scripts/validate-ide-e2e-report.ps1`
- `scripts/new-ide-e2e-report.ps1`
- `scripts/validate-release-gate-summary.ps1`
- `scripts/verify-required-status-checks.ps1`
- `scripts/check-final-readiness.ps1`
- `scripts/apply-and-verify-branch-protection.ps1`
- `scripts/validate-real-ide-e2e-artifacts.ps1`

Schema smoke gate workflow: `.github/workflows/cabal-mcp-runtime-ide-e2e-report-schema.yml`.
Schema smoke gate workflow (release summary): `.github/workflows/cabal-mcp-runtime-release-summary-schema.yml`.

Включает transport-level MCP smoke:
- `tests/mcp_stdio_e2e.rs`:
  - NDJSON path (adaptive routing + proxy/gate/error contract),
  - Content-Length framed input path (`initialize/tools/list/tools/call`),
  - mixed framed/NDJSON session path,
  - IDE profile handshake/enforcement path (`initialize` + policy-driven allowlist + consult route context chain),
  - CPU policy path (`get/set_cpu_policy`, startup validation for `require_zen4_fast_path` + feature flags `require_avx512f/avx512vl/fma/bmi2/sha`),
  - audit hardening path (`get/set policy` + auto-rotation + `rotate` + `verify` + `prune` + `audit_health_check` + empty-log guard),
  - consult routing contract path (role-mismatch fallback/escalation, critical SLA/escalation, adaptive confidence-floor fallback, IDE profile context in route/audit for `vscode` + `jetbrains`),
  - adaptive exploration path (`set_adaptive_exploration_policy` + `route_consult` strategy `adaptive_explore`),
  - proxy shell hardening path (dangerous command fragments blocked with `PROXY_DENY` even in `allow_by_default` mode),
  - proxy operation policy path (`set_proxy_operation_policy` allow/deny operations with result-level deny semantics),
  - proxy network hardening path (unsafe localhost/private/link-local/metadata targets blocked with `PROXY_DENY`),
  - transport error-code contract checks (`PARSE_ERROR`, `TRANSPORT_ERROR`, `UNKNOWN_TOOL`, `UNSUPPORTED_METHOD`, `GATE_FAIL`, `PROXY_DENY`, `INVALID_REQUEST`, `REVISION_MISMATCH`, `SIGNATURE_INVALID`, `IO_FAILURE`, `STORAGE_FAILURE`),
  - policy signing key lifecycle error paths on transport (`expired`/`revoked` key -> `SIGNATURE_INVALID`),
  - consult/audit contract checks (`consult.routed` with `request_id/actor/policy_revision`),
  - parity check tool path (`cabal.validate_error_codes_parity`).

Core-модули (после `P9` рефакторинга):
- `src/core/router.rs` — CONSULT routing/adaptive scoring/resolvers.
- `src/core/policy.rs` — policy signing/nonce/key selection.
- `src/core/gate.rs` + `src/core/gate_engine.rs` — gate DTO + checks.
- `src/core/audit.rs` — audit append/read/query/replay/rotate/verify.
- `src/core/ide.rs` — IDE profile detect/normalize/allowlist checks.
- `src/core/proxy.rs` + `src/core/proxy_exec.rs` — proxy decision/trace + fs/shell/network execution.
- `src/core/fsm.rs` — phase transition decision/strict validation.
- `src/core/events.rs` — event canonicalization/summary helpers.

Сервер работает по stdio JSON-RPC (MCP-совместимый метод-слой):
- `initialize`
- `tools/list`
- `tools/call`

## Ключевые tools
- `cabal.get_capabilities`
- `cabal.get_error_codes`
- `cabal.validate_error_codes_parity`
- `cabal.get_state`
- `cabal.get_cpu_policy`
- `cabal.set_cpu_policy`
- `cabal.get_gate_policy`
- `cabal.set_gate_policy`
- `cabal.get_ide_profile_policy`
- `cabal.set_ide_profile_policy`
- `cabal.get_consult_routing`
- `cabal.get_cross_rules_status`
- `cabal.get_consult_guard_policy`
- `cabal.get_adaptive_router`
- `cabal.set_consult_mode`
- `cabal.set_consult_guard_policy`
- `cabal.ack_cross_rules`
- `cabal.set_adaptive_router`
- `cabal.set_adaptive_exploration_policy`
- `cabal.set_consult_routing_rule`
- `cabal.set_consult_priority_timeout`
- `cabal.set_consult_retry_limit`
- `cabal.set_consult_escalation_target`
- `cabal.set_consult_allowed_roles`
- `cabal.record_consult_feedback`
- `cabal.apply_policy_bundle` (revision-locked apply)
- `cabal.set_policy_security` (require signed policy on/off)
- `cabal.list_policy_signing_keys`
- `cabal.upsert_policy_signing_key`
- `cabal.set_active_policy_signing_key`
- `cabal.revoke_policy_signing_key`
- `cabal.guard_action`
- `cabal.get_proxy_operation_policy`
- `cabal.set_proxy_operation_policy`
- `cabal.set_proxy_policy`
- `cabal.get_proxy_log`
- `cabal.get_audit_log`
- `cabal.query_audit_log`
- `cabal.export_audit_log`
- `cabal.replay_audit_state`
- `cabal.get_audit_rotation_policy`
- `cabal.set_audit_rotation_policy`
- `cabal.rotate_audit_log`
- `cabal.verify_audit_archive`
- `cabal.prune_audit_archives`
- `cabal.audit_health_check`
- `cabal.proxy_request` (deny-by-default mediation)
- `cabal.proxy_execute` (enforce + trace)
- `cabal.transition_phase`
- `cabal.transition_phase_strict` (exit+entry gate checks)
- `cabal.gate_check`
- `cabal.route_consult`
- `cabal.register_evidence`
- `cabal.record_event`

Shell safety policy:
- `cabal.proxy_execute(category=shell, operation=run)` блокирует опасные фрагменты команд (`rm -rf`, `git reset --hard`, `format`, `mkfs`, `shutdown` и др.) независимо от `deny_by_default`.
- Дополнительно введён лимит длины shell target (`1024` символа): overlong command блокируется до исполнения.
- Результат shell execution ограничен bounded stdout/stderr (`4000` bytes) и содержит метки `stdout_truncated`/`stderr_truncated`.
- Для `shell/run` включён runtime timeout (`15s`), при превышении возвращается отказ `shell command timed out` (`EXECUTOR_FAILURE`).
- Для тестовых/операционных профилей timeout можно переопределить env-параметром `CABAL_PROXY_SHELL_TIMEOUT_MS`.
- `cabal.set_proxy_operation_policy` позволяет задать category-level operation allowlist/denylist (denylist имеет приоритет).
- `proxy_log` ограничен bounded retention (`5000` последних записей), старые trace записи автоматически отбрасываются.
- `cabal.get_proxy_log(limit)` валидирует `limit>0` и применяет server-side cap (`max_limit=1000`).

FS safety policy:
- `cabal.proxy_execute(category=fs, operation=read_text)` использует bounded-read guardrail (`131072` bytes max) и возвращает поля `truncated`/`read_bytes`.
- `cabal.proxy_execute(category=fs, operation=write_text)` ограничен max payload (`1048576` bytes) с отказом до записи при превышении.
- `cabal.proxy_execute(category=fs, operation=list_dir)` ограничивает ответ первыми `1000` entries и возвращает `truncated`/`total_entries`.

Network safety policy:
- `cabal.proxy_execute(category=network, operation=http_get)` блокирует unsafe targets (localhost/private/link-local/metadata hosts), даже в `allow_by_default` режиме.
- Некорректный URL target (`invalid network target url`) также классифицируется как `PROXY_DENY` для единообразного proxy-contract.
- Для `http_get` включены runtime guardrails: connect/read/write timeout и лимит тела ответа (`body` усечён до 8192 байт, флаг `truncated` в результате).

## Error Contract
Ошибки возвращаются в JSON-RPC `error.data` с машинными полями:
- `cabal_code` (например: `REVISION_MISMATCH`, `GATE_FAIL`, `PROXY_DENY`)
- `retryable` (`true|false`)
- `method`
- `tool`

Ключевые классы отказов:
- `PARSE_ERROR`, `TRANSPORT_ERROR`
- `REVISION_MISMATCH`, `SIGNATURE_INVALID`, `NONCE_REPLAY`
- `GATE_FAIL`, `POLICY_DENY`, `PROXY_DENY`
- `EXECUTOR_FAILURE`, `STORAGE_FAILURE`, `IO_FAILURE`, `STATE_CORRUPT`

Каноническая SDK-спека кодов:
- `../spec/docs/CABAL_ERROR_CODES.md`

## Policy Signature
Если включён `require_signed_policy=true`, для `cabal.apply_policy_bundle` обязательны:
- `signature` (hex HMAC-SHA256)
- `key_id` (опционально; если не задан, используется active key)
- `nonce` (уникальный anti-replay токен)

Runtime поддерживает key registry (rotation/revoke/expiry):
- key-id и env-var секрета;
- окно валидности (`not_before_unix`/`not_after_unix`);
- отзыв ключа (`revoke`).

Ключ по умолчанию:
- `key_id=default`
- `key_env=CABAL_POLICY_HMAC_KEY`

## Runtime state
Состояние хранится в:
- `.cabal_runtime/state.json`
- `.cabal_runtime/audit.jsonl` (append-only audit log)

Это программный реестр фазы, policy bundle, evidence и audit-событий.

## CPU Policy
`cabal.get_cpu_policy`/`cabal.set_cpu_policy` управляют runtime-политикой CPU:
- `require_zen4_fast_path=false` (по умолчанию);
- `require_zen4_fast_path=true` разрешается только при активном пути `zen4_avx512`, иначе возвращается `POLICY_DENY`.
- Дополнительные feature-flags: `require_avx512f`, `require_avx512vl`, `require_fma`, `require_bmi2`, `require_sha`.
- При включении любого feature-flag runtime валидирует наличие feature на текущем CPU и при несоответствии возвращает `POLICY_DENY`.
- При несовместимой persisted CPU policy runtime блокирует startup до MCP loop (boot-time enforcement).

## IDE Profile Policy
Runtime фиксирует профиль IDE-клиента на `initialize` (например `vscode`, `jetbrains`, `cursor`) и сохраняет его в runtime-state.

Policy-управление:
- `cabal.get_ide_profile_policy` — текущий active profile + allowlist/enforcement;
- `cabal.set_ide_profile_policy` — настройка `enforce_ide_profile`, `require_client_info` и `allowed_profiles`.

Если enforcement включен, `initialize` блокируется для неразрешённых профилей (`POLICY_DENY`).
Если включен `require_client_info`, `initialize` без `clientInfo.name` также блокируется (`POLICY_DENY`).

## Gate Entry Rules
Для входа в фазу через `cabal.transition_phase_strict` требуется evidence-подтверждение ознакомления со сквозными правилами:
- `cross_rules_agent_ack`
- `cross_rules_subagent_ack`

Оба ключа регистрируются через `cabal.register_evidence` до вызова strict transition.
Быстрый атомарный путь: `cabal.ack_cross_rules` (регистрирует оба evidence ключа и может сразу включить consult guard policy).
Текущее состояние по cross-rules проверяется через `cabal.get_cross_rules_status`.

`cabal.get_gate_policy`/`cabal.set_gate_policy` управляют флагом `strict_artifacts`:
- `false` (по умолчанию): локальные sandbox-прогоны допускают skip части file-based checks;
- `true`: отсутствие обязательных phase/global/canon файлов фиксируется как `gate fail`.

## Audit v2
Добавлены операции уровня расследований:
- фильтрация аудита по `kind/phase/policy_revision/request_id/time`;
- экспорт среза аудита в `jsonl` (repo-relative путь);
- replay snapshot состояния до `event_id` или `ts_unix`;
- auto-rotation policy (`size/time/compress/retention`) через `cabal.get_audit_rotation_policy`/`cabal.set_audit_rotation_policy`;
- ротация активного `audit.jsonl` в архив (`jsonl`/`jsonl.gz`) с `sha256` sidecar;
- проверка целостности audit-архива через `cabal.verify_audit_archive`;
- retention cleanup архивов через `cabal.prune_audit_archives` (`keep_last`).
- health-check отчёт по active log + архивам через `cabal.audit_health_check` (verify последних `N` архивов, статус `pass/warn/fail`).
- для `get/query/export` audit применяется server-side limit cap (`2000`) и валидация `limit>0`.
- `cabal.query_audit_log` возвращает `max_limit` для прозрачности applied server-side cap.
- `cabal.export_audit_log` возвращает `requested_limit`/`applied_limit`/`max_limit` для аудита bounded-export поведения.

## Consult Router v2
`cabal.route_consult` поддерживает:
- `consult_type`
- `priority` (`low|normal|high|critical`)
- `preferred_role`
- `request_id`

Ответ маршрутизации всегда включает:
- `route`
- `reason`
- `actor`
- `policy_revision`
- `ide_profile`
- `ide_client_name`

Поведение:
- `USER_TRACKING` -> маршрут к пользователю;
- `YOLO` -> маршрут к orchestrator с выбором исполнителя по типу CONSULT.
- Если `preferred_role` не входит в allowlist для `consult_type`, runtime выполняет эскалацию и выбирает только разрешённую fallback-роль (без dispatch в запрещённого исполнителя).
- При включённой guard policy (`require_cross_rules_ack=true`) в режиме `YOLO` вызов `cabal.route_consult` блокируется с `POLICY_DENY`, пока не зарегистрированы все `required_evidence_ids`.

Policy-driven настройка:
- `cabal.set_consult_routing_rule` для `consult_type -> executor`;
- `cabal.set_consult_priority_timeout` для SLA timeout по приоритетам;
- `cabal.set_consult_retry_limit` для retry policy по приоритетам;
- `cabal.set_consult_escalation_target` для target эскалации;
- `cabal.set_consult_allowed_roles` для role allowlist по типам CONSULT;
- `cabal.get_consult_guard_policy`/`cabal.set_consult_guard_policy` для enforcement набора evidence перед CONSULT routing;
- `cabal.get_consult_routing` для чтения активной матрицы.

Adaptive Router (эмерджентный слой):
- `cabal.set_adaptive_router` включает/настраивает адаптивный выбор исполнителя;
- `cabal.set_adaptive_exploration_policy` настраивает exploration (`exploration_rate`, `exploration_min_samples`);
- `cabal.record_consult_feedback` пишет outcome/latency телеметрию по роли исполнителя;
- `cabal.get_adaptive_router` показывает текущие настройки и накопленные метрики;
- `cabal.route_consult` возвращает `routing_decision` (`strategy`, `score`, `confidence`, `confidence_floor`, `exploration_rate`, `exploration_min_samples`);
- при активном exploration возможна стратегия `adaptive_explore` с выбором недообученного исполнителя (по `min_samples`) для controlled exploration.
