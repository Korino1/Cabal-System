---
id: cabal_mcp_runtime_implementation_plan
updated: 2026-02-25
owner: orchestrator
status: ready_for_testing
---
# План Полной Реализации Cabal MCP Runtime

## 1) Цель и рамки
Реализовать `Cabal MCP Runtime` как единый `control plane` для любой IDE, где:
- любой рабочий шаг модели идёт только через MCP-вызов;
- политика, фазы и гейты применяются программно, а не из markdown-инструкций;
- логика решения воспроизводима по audit trail;
- обход контроля фиксируется как дефект и блокируется.

## 2) Целевая архитектура (Cabal Core)
### 2.1) Слой A — Deterministic Kernel (обязательный)
- `TaskClassifier`
- `PolicyEngine`
- `PhaseFSM`
- `GateValidator`
- `PatchGate`
- `AuditStore` (append-only + replay)

### 2.2) Слой B — Adaptive Regulator (управляемая эмерджентность)
- `ExecutorRouter`
- `RiskScorer`
- `BudgetController`
- `ConfidenceController`

### 2.3) Слой C — Executors
- LLM-исполнители
- компилятор/линтер/тест-раннер
- статический анализ
- локальные инструменты (только через proxy)

## 3) Инварианты системы (неизменяемые правила)
- `MCP-only`: вне MCP действий нет.
- `Policy-first`: policy и gate всегда до исполнения.
- `No direct logic files`: модели не исполняют инструкции из файлов схемы.
- `No silent bypass`: любой отказ или обход логируется.
- `Deterministic state`: phase/policy/evidence/events версионируются и проверяются.

## 4) Контракты runtime (канон реализации)
### 4.1) FSM контур
Состояния:
- `received`
- `planned`
- `executing`
- `verifying`
- `finalizing`
- `done`
- `blocked`
- `escalated`
- `rolled_back`

Переходы:
- только через `PolicyEngine + GateValidator`;
- переход фазы (`C-0 -> GA-1 -> ...`) только через strict gate.

### 4.2) Event contract (audit)
Каждое событие обязано содержать:
- `event_id`
- `ts_unix`
- `kind`
- `phase`
- `policy_revision`
- `request_id`
- `actor`
- `payload`
- `digest`

Append-only хранилище:
- `.cabal_runtime/audit.jsonl`

### 4.3) Command envelope
```json
{
  "request_id": "uuid",
  "phase": "GA-1",
  "task_type": "refactor|debug|codegen|analysis",
  "risk": "low|medium|high",
  "constraints": {},
  "executor_hint": "optional",
  "policy_revision": 42
}
```

### 4.4) Error taxonomy
Минимум классов:
- `POLICY_DENY`
- `GATE_FAIL`
- `PROXY_DENY`
- `REVISION_MISMATCH`
- `SIGNATURE_INVALID`
- `NONCE_REPLAY`
- `EXECUTOR_FAILURE`
- `VERIFY_FAIL`

## 5) Структура модулей Rust (план целевого кода)
- `src/core/fsm.rs`
- `src/core/policy.rs`
- `src/core/gate.rs`
- `src/core/events.rs`
- `src/core/router.rs`
- `src/core/patch_gate.rs`
- `src/core/errors.rs`
- `src/runtime.rs` (агрегатор состояния и orchestration glue)
- `src/main.rs` (MCP transport + tools registry)

Текущее состояние:
- часть функционала уже реализована в `src/runtime.rs` и `src/main.rs`;
- модульный рефакторинг в отдельный `src/core/*` запланирован отдельными шагами.

## 6) Дорожная карта реализации (end-to-end)
Статусы: `pending | in_progress | done | blocked`

| ID | Этап | Ключевой результат | Статус | % |
|---|---|---|---|---|
| P0 | Architecture Freeze | Зафиксированы слои A/B/C и инварианты | done | 100 |
| P1 | Runtime Base | MCP сервер, state, базовые tools | done | 100 |
| P2 | CPU/SIMD Enforcement | Hard fail < AVX2, Zen4 fast-path | done | 100 |
| P3 | Tool Proxy | deny-by-default, allowlist, exec mediation | done | 100 |
| P4 | Policy Engine v2 | revision lock, signature, key registry | done | 100 |
| P5 | Gate Engine v2 | strict entry/exit по PHASE_GATE | done | 100 |
| P6 | Consult Router v2 | USER_TRACKING/YOLO full routing | done | 100 |
| P7 | Audit Store v2 | append-only + query/filter/export/replay | done | 100 |
| P8 | Error Codes | строгая машинная таксономия отказов | done | 100 |
| P9 | Core Refactor | вынос в `src/core/*` без регрессий | done | 100 |
| P10 | IDE Integration | VS Code/JetBrains transport profile + policy enforcement | done | 100 |
| P11 | E2E Anti-Bypass | сценарии обхода и блокировок | done | 100 |
| P12 | Perf/Hardening | нагрузка, стабильность, memory limits | done | 100 |
| P13 | RC | freeze API/policy + acceptance audit | done | 100 |
| P14 | Production Rollout | staged rollout + ops playbook | in_progress | 85 |

## 7) Детализация этапов (что делаем и критерии done)
### P4 — Policy Engine v2
Сделать:
- key-id registry (rotation/revoke/expiry);
- обязательная подпись при включённом secure-mode;
- миграции policy без потери совместимости.
Done когда:
- подмена/устаревание policy блокируется;
- все policy-изменения трассируются и воспроизводимы.

### P5 — Gate Engine v2
Сделать:
- программные checks по `PHASE_GATE.md` для entry/exit;
- строгие отказы переходов при невыполнении критериев;
- machine-readable отчёты.
Done когда:
- ни один фазовый переход не проходит без валидного gate report.

### P6 — Consult Router v2
Сделать:
- `USER_TRACKING`: маршрут к пользователю;
- `YOLO`: каждый consult к orchestrator + выбор исполнителя;
- SLA по эскалациям и таймаутам.
Done когда:
- у каждого consult есть `route + reason + actor + policy_revision`.

### P7 — Audit Store v2
Сделать:
- фильтры по `kind/phase/revision/request_id`;
- экспорт расследований (`jsonl`/`json`);
- replay трассы решения.
Done когда:
- любую развилку решения можно воспроизвести и объяснить.

### P8 — Error Codes
Сделать:
- единая enum-таксономия ошибок;
- стабильные коды в MCP ответах;
- таблица совместимости для IDE клиентов.
Done когда:
- клиенты могут детерминированно обрабатывать отказ без парсинга текста ошибок.

### P9 — Core Refactor
Сделать:
- вынести kernel-модули из монолитного `runtime.rs`;
- сохранить API совместимость tools;
- покрыть регрессионными тестами.
Done когда:
- код разделён по ответственности, тесты зелёные, поведение неизменно.

## 8) Текущий срез выполнения (на 2026-02-25)
Уже реализовано:
- MCP stdio layer (`initialize`, `tools/list`, `tools/call`);
- proxy deny-by-default + `proxy_execute` для `fs/shell/network`;
- CPU policy runtime-контур: `cabal.get_cpu_policy`/`cabal.set_cpu_policy` + startup validation (`require_zen4_fast_path`);
- CPU feature policy runtime-контур: `cabal.set_cpu_policy` поддерживает `require_avx512f/require_avx512vl/require_fma/require_bmi2/require_sha` с `policy deny` при несовместимости.
- `policy revision lock`;
- signed policy (`HMAC-SHA256` + nonce anti-replay);
- key registry (`key_id`, active key, revoke, expiry);
- transport-level error contract для signing key lifecycle (`expired`/`revoked` -> `SIGNATURE_INVALID`);
- `gate_check` и `transition_phase_strict`;
- append-only audit (`.cabal_runtime/audit.jsonl`);
- машинные `cabal_code` в JSON-RPC ошибках + каталог кодов;
- audit v2: `query/filter/export/replay` инструменты;
- consult router v2 (priority/type/role + YOLO dispatch на orchestrator);
- consult router v2 policy: настраиваемый `type -> executor` и SLA timeout matrix;
- consult router v2 contract: `route + reason + actor + policy_revision` в ответе и audit-записи;
- consult router v2 IDE context contract: `ide_profile`/`ide_client_name` включены в route response и `consult.routed` audit payload;
- adaptive consult router: эмерджентный выбор исполнителя по feedback/latency телеметрии с `confidence_floor` и policy fallback;
- добавлены `TaskClassifier + BudgetController + PatchGate` на MCP-уровне:
  - tools `cabal.classify_task`, `cabal.get_budget_policy`, `cabal.set_budget_policy`, `cabal.plan_task_execution`, `cabal.get_patch_gate_policy`, `cabal.set_patch_gate_policy`, `cabal.evaluate_patch_gate`,
  - `route_consult` включает `task_profile` (type/risk/confidence/keywords/budget) в response и audit.
- начат `P9`: выделен `src/core/router.rs` и перенесены scorer/selector функции adaptive routing из `runtime.rs`;
- в `P9` продолжен вынос CONSULT-канона: defaults/normalization/executor selection перенесены в `core/router.rs`;
- добавлен transport-level MCP stdio E2E smoke-test (initialize/tools/call/adaptive route) как база для IDE-клиентских профилей;
- в `P9` вынесен audit core: `query/replay` фильтрация и snapshot логика перенесены в `src/core/audit.rs`;
- в `P9` вынесен audit I/O core: append/read записей перенесён в `src/core/audit.rs`, `runtime.rs` использует thin wrappers;
- в `P9` вынесен phase core: transition/evidence канон (`is_valid_transition`, `phase_order_index`, `required_exit_evidence`) перенесён в `src/core/phase.rs`;
- в `P9` вынесен gate core: `GateReport/GateCheckItem/gate_item` перенесены в `src/core/gate.rs`;
- в `P9` вынесен proxy core: deny/allow решение (`evaluate_proxy_request`) перенесено в `src/core/proxy.rs`;
- в `P9` вынесен gate engine core: `build_gate_report` и markdown/file checks перенесены в `src/core/gate_engine.rs`, `runtime.rs` переключён на thin adapter без дублирования логики;
- в `P9` вынесен policy core: `PolicySigningKey`, policy signature verify/message/nonce логика перенесены в `src/core/policy.rs`, `runtime.rs` использует thin wrappers;
- в `P9` вынесен events core: `EventRecord`, `summarize_payload`, `truncate_text` перенесены в `src/core/events.rs`;
- в `P9` продолжен consult core: timeout/retry/escalation/executor/role-fallback/adaptive resolver логика перенесена в `src/core/router.rs`, `runtime.rs::route_consult` использует core-helpers;
- в `P9` вынесен proxy-exec core: `fs/shell/network` execution и safe path resolution перенесены в `src/core/proxy_exec.rs`, runtime использует core-helpers;
- в `P9` вынесен fsm core: phase transition decision и strict gate validation перенесены в `src/core/fsm.rs`, runtime использует core-helpers;
- в `P9` вынесен proxy-trace core: `ProxyTraceRecord`, hash-input и record builder перенесены в `src/core/proxy.rs`, runtime использует core-helpers;
- в `P9` устранён лишний runtime-wrapper слой для policy signing: `apply_policy` вызывает `core/policy` напрямую, дублирующие методы удалены;
- в `P9` вынесен event wiring core: hash-material и event record builder перенесены в `src/core/events.rs`, runtime использует core-helpers;
- для `P11` добавлены anti-bypass integration tests: FS path traversal/absolute path блокируются даже в `allow_by_default` режиме proxy;
- для `P3/P11` добавлен proxy shell hardening контур:
  - `proxy_execute(shell/run)` блокирует опасные command fragments (`rm -rf`, `git reset --hard`, `format`, `mkfs`, `shutdown` и т.д.) даже в `allow_by_default` режиме,
  - добавлены unit/integration/stdIO E2E проверки с machine-readable классификацией `PROXY_DENY`.
- для `P3/P10/P11` добавлен operation-level proxy policy контур:
  - tools `cabal.get_proxy_operation_policy`/`cabal.set_proxy_operation_policy`,
  - runtime учитывает category operation allowlist/denylist в `proxy_request` до исполнения executor,
  - transport-level контракт подтверждён: deny по operation policy возвращается result-путём (`allow=false`, `executed=false`), без JSON-RPC error.
- для `P3/P11` добавлен network target hardening контур:
  - `proxy_execute(network/http_get)` блокирует localhost/private/link-local/metadata endpoints до сетевого вызова,
  - добавлены unit/integration/stdIO E2E тесты на policy-block с классификацией `PROXY_DENY`.
- для `P10/P11` добавлен stdio E2E anti-bypass test: `tools/call(cabal.proxy_execute)` с path traversal возвращает JSON-RPC error с `cabal_code=INVALID_REQUEST`;
- для `P10/P11` добавлен IDE profile policy runtime-контур:
  - `initialize` фиксирует active IDE profile (`vscode/jetbrains/cursor/windsurf/zed/generic`) и пишет audit `ide.client_initialized`,
  - `cabal.get_ide_profile_policy`/`cabal.set_ide_profile_policy` управляют enforcement и allowlist профилей,
  - поддержан anti-bypass флаг `require_client_info` (блок `initialize` без `clientInfo.name`),
  - при включённом enforcement недопустимый IDE profile блокируется на `initialize` с `POLICY_DENY`;
- для `P10` добавлены transport-level stdio E2E сценарии IDE profile handshake/enforcement;
- для `P10` добавлен цепочный IDE transport сценарий: profile enforcement (`deny/allow`) + `route_consult` в одной сессии с проверкой IDE context в ответе.
- для `P5/P10/P11` добавлен жёсткий entry-gate контракт по сквозным правилам:
  - на entry любой фазы обязательны evidence `cross_rules_agent_ack` и `cross_rules_subagent_ack`,
  - добавлены integration/stdIO E2E проверки на deny-path без ack и pass-path после регистрации evidence;
- для `P5/P10/P11` добавлен policy-контур strict gate artifacts:
  - tools `cabal.get_gate_policy`/`cabal.set_gate_policy` управляют флагом `strict_artifacts`,
  - в strict mode отсутствие обязательных phase/global/canon файлов приводит к явному `gate fail`,
  - добавлены integration/stdIO E2E тесты на toggling strict-mode и проверку `entry_required_files_present`.
- для `P7` добавлен audit hardening runtime-контур:
  - `cabal.rotate_audit_log` ротирует активный `audit.jsonl` в архив (`jsonl`/`jsonl.gz`) и создаёт `sha256` sidecar,
  - `cabal.verify_audit_archive` проверяет целостность архива по sidecar (включая `.gz` decode path),
  - добавлены contract tests на успешный rotate/verify и tamper detection.
- для `P7` добавлен retention cleanup:
  - `cabal.prune_audit_archives` удаляет старые архивы и sidecar по policy `keep_last`,
  - добавлены unit/integration/stdio E2E тесты для prune-контракта.
- для `P7` добавлен auto-rotation trigger контур:
  - `cabal.get_audit_rotation_policy`/`cabal.set_audit_rotation_policy` управляют `enabled/max_bytes/max_age_sec/compress/keep_last/archive_dir`,
  - append-audit path теперь автоматически запускает rotate+prune по size/time threshold,
  - события auto/manual ротаций фиксируются в audit (`audit.rotated` / `audit.archives_pruned` / `audit.rotation_policy_changed`).
- для `P7` добавлен агрегированный health-check контур:
  - tool `cabal.audit_health_check` возвращает состояние active audit + архивов в одной проверке,
  - проверяется подпись последних `N` архивов (`verify_last`) и рассчитывается итоговый статус `pass/warn/fail`,
  - добавлены pass/fail (tamper) сценарии на runtime API и stdio transport.
- для `P3/P8/P10` усилен error-contract proxy network URL:
  - `invalid network target url` для `cabal.proxy_execute` теперь классифицируется как `PROXY_DENY`,
  - добавлены unit/stdio E2E проверки на invalid network target url path.
- для `P6/P10` добавлен controlled adaptive exploration контур:
  - runtime-state расширен policy-параметрами `adaptive_exploration_rate` и `adaptive_exploration_min_samples`,
  - добавлен tool `cabal.set_adaptive_exploration_policy`,
  - `route_consult` поддерживает стратегию `adaptive_explore` (детерминированный seed-based exploration по недообученным исполнителям),
  - добавлены runtime/integration/stdio E2E тесты на pass-path и invalid policy path.
- для `P3/P12` добавлен network response hardening контур:
  - `proxy_execute(network/http_get)` теперь использует фиксированные connect/read/write timeout guardrails,
  - добавлен лимит network response body (8192 bytes) с контрактными полями `truncated` и `body_bytes` в результате,
  - добавлены unit tests для bounded body reader (truncate/no-truncate paths).
- для `P3/P12` добавлен shell input hardening контур:
  - введён лимит длины shell target command (`1024`) до execution path,
  - overlong shell requests классифицируются как `INVALID_REQUEST`,
  - добавлены unit/stdio E2E тесты на overlong shell-command path.
- для `P3/P12` добавлен bounded FS read контур:
  - `proxy_execute(fs/read_text)` теперь ограничивает объём чтения (`131072` bytes),
  - результат `read_text` дополнен полями `truncated` и `read_bytes`,
  - добавлен unit test на bounded-read поведение для large file path.
- для `P3/P12` добавлен bounded shell stdio контур:
  - `proxy_execute(shell/run)` возвращает ограниченные `stdout/stderr` (`4000` bytes),
  - в response contract добавлены поля `stdout_truncated`/`stderr_truncated` и `stdout_bytes`/`stderr_bytes`,
  - добавлен unit test на bounded text helper path.
- для `P3/P12` добавлен bounded FS operations контур:
  - `proxy_execute(fs/write_text)` теперь ограничен max payload (`1048576` bytes) до disk write path,
  - `proxy_execute(fs/list_dir)` ограничивает выдачу (`1000` entries) и возвращает `truncated`/`total_entries`,
  - добавлены unit/stdio E2E тесты на oversized write и bounded list path.
- для `P3/P12` добавлен bounded proxy trace retention контур:
  - runtime теперь удерживает только последние `5000` записей `proxy_log`,
  - при overflow старые trace entries автоматически отбрасываются (tail-retention),
  - добавлен unit test на bounded retention ordering.
- для `P3/P8/P12` добавлен limit-validation/limit-cap контур:
  - `cabal.get_proxy_log(limit)` и audit query/export paths валидируют `limit>0`,
  - применён server-side cap (`proxy_log=1000`, `audit=2000`) для стабилизации payload-size,
  - добавлены runtime/errors/stdio E2E тесты на `limit=0` invalid-request path.
- для `P3/P12` добавлен shell timeout/circuit-breaker контур:
  - `proxy_execute(shell/run)` теперь работает с runtime timeout guardrail (`15s`) и kill path при exceed,
  - timeout path возвращает детерминированный отказ `shell command timed out`,
  - добавлены unit tests в `core/proxy_exec` + `errors` классификация в `EXECUTOR_FAILURE`.
- для `P3/P10/P12` добавлен transport-level shell timeout контур:
  - добавлен env-override `CABAL_PROXY_SHELL_TIMEOUT_MS` для controlled timeout profiles,
  - stdio E2E сценарий подтверждает timeout-path `cabal.proxy_execute(shell/run)` -> `EXECUTOR_FAILURE`.
- для `P12` добавлен release-gate скрипт stress SLA:
  - `scripts/check-stress-sla.ps1` выполняет `cargo test --test runtime_stress -- --ignored --nocapture`,
  - скрипт стабилизирован для запуска из любой текущей директории (`Push-Location` к корню `cabal-mcp-runtime`),
  - актуальный прогон подтверждён: single-run ingest=`964ms`, query=`108ms`, export=`135ms`, replay=`74ms`; multi-run query `p95/p99=61/61ms`, export `84/84ms`, replay `38/38ms`.
- для `P12` подключён CI release-gate:
  - добавлен workflow `.github/workflows/cabal-mcp-runtime-stress-gate.yml`,
  - на `push/pull_request` по путям `cabal-mcp-runtime/**` запускается `scripts/check-stress-sla.ps1`,
  - CI теперь фиксирует regression latency до ручного релиза.
- для `P10` добавлены IDE adapter templates:
  - `spec/examples/ide/vscode.mcp.jsonc` и `spec/examples/ide/jetbrains.mcp.jsonc`,
  - `spec/examples/ide/README.md` фиксирует единый onboarding для stdio MCP подключения,
  - `cabal-mcp-runtime/README.md` связан с шаблонами для быстрого запуска в IDE клиентах.
- для `P6/P10/P11` добавлен CONSULT guard policy контур:
  - `RuntimeState` расширен `consult_require_cross_rules_ack` и `consult_required_evidence_ids`,
  - добавлены tools `cabal.get_consult_guard_policy`/`cabal.set_consult_guard_policy`,
  - `route_consult` в `YOLO` режиме теперь может блокироваться с `POLICY_DENY` до регистрации обязательных evidence IDs,
  - deny-path фиксируется в audit событии `consult.blocked_missing_evidence`.
- для `P6/P10/P11` добавлен cross-rules orchestration контур:
  - добавлен tool `cabal.get_cross_rules_status` (entry-gate и consult-guard статус по evidence),
  - добавлен tool `cabal.ack_cross_rules` (атомарная регистрация `cross_rules_agent_ack` + `cross_rules_subagent_ack`),
  - `ack_cross_rules` поддерживает авто-включение consult-guard и логирует `cross_rules.acknowledged`.
- для `P12` добавлен branch-protection automation script:
  - `scripts/set-required-stress-gate.ps1` применяет GitHub branch protection с required status check `stress-sla-gate`,
  - доступен `-DryRun` режим для безопасной валидации payload без вызова GitHub API.
- для `P10/P12` добавлен IDE contract gate контур:
  - `scripts/check-ide-contract-gate.ps1` запускает целевые stdio E2E + integration тесты для IDE profile/gate/consult contract,
  - подключён CI workflow `.github/workflows/cabal-mcp-runtime-ide-contract-gate.yml`,
  - README дополен отдельной командой локального IDE contract gate.
- для `P12` добавлен unified release-gate контур:
  - `scripts/check-release-gates.ps1` последовательно исполняет `check-stress-sla` и `check-ide-contract-gate`,
  - подключён manual CI workflow `.github/workflows/cabal-mcp-runtime-release-gate.yml` как единая pre-RC проверка.
- для `P10` добавлен канонический checklist реальной IDE E2E:
  - `spec/docs/CABAL_IDE_E2E_CHECKLIST.md` фиксирует matrix `IDE-P1..IDE-P5` и критерий PASS для VS Code/JetBrains.
- для `P10` добавлен формальный контракт отчёта ручной IDE E2E:
  - `spec/contracts/IDE_E2E_REPORT.schema.json`,
  - `cabal-mcp-runtime/scripts/validate-ide-e2e-report.ps1` для machine-validation итогового отчёта.
- расширенная таксономия ошибок: storage/io/state/signature/proxy/gate/policy/revision;
- runtime parity-check `cabal.validate_error_codes_parity` + тест синхронизации с `CABAL_ERROR_CODES.md`;
- unit/integration tests на policy/gate/proxy/audit.

Осталось до полноценного контура:
- выполнить IDE E2E parity-проверку SDK-кодов (runtime tool vs docs) на реальных MCP-клиентах;
- audit v2 hardening: validate auto-rotation/retention в реальном IDE MCP-пайплайне + ops thresholds tuning;
- consult router v2: провести IDE E2E-валидацию маршрутизации (fallback/escalation/role-policy/adaptive scoring) на целевых MCP-клиентах;
- IDE E2E anti-bypass в реальном MCP-клиентском окружении;
- закрепить workflow `cabal-mcp-runtime-stress-gate` как required status check на protected branches.

## 9) Трекинг прогресса (обязательный регламент)
После каждой сессии обновлять:
1. Таблицу этапов (статус + %).
2. Блок `Журнал прогресса`.
3. `Next-3`.
4. Блокеры и риски.

## 10) Журнал прогресса
### 2026-02-24 (update-1)
- Создан runtime skeleton на Rust nightly.
- Поднят MCP transport и базовые tools.

### 2026-02-24 (update-2)
- Реализован proxy mediation + deny-by-default.
- Введён `policy revision lock`.
- Добавлены integration tests на guard/phase/proxy.

### 2026-02-24 (update-3)
- Реализован `proxy_execute` (fs/shell/network) с trace.
- Добавлены tools управления proxy-политикой.

### 2026-02-24 (update-4)
- Добавлена signed policy верификация (`HMAC-SHA256 + nonce`).
- Добавлены `gate_check` и `transition_phase_strict`.

### 2026-02-24 (update-5)
- Добавлен policy key registry (`key_id`, active, revoke, expiry).
- Добавлен append-only `audit.jsonl` и чтение хвоста.
- Расширены gate checks по `PHASE_GATE.md`.
- `cargo test`: PASS.

### 2026-02-24 (update-6)
- План обновлён в полный implementation blueprint:
  - зафиксированы архитектура слоёв A/B/C,
  - контракты FSM/events/errors,
  - roadmap до production rollout,
  - обязательный регламент трекинга.

### 2026-02-24 (update-7)
- Добавлен модуль `src/errors.rs` с таксономией и классификатором ошибок.
- В JSON-RPC ответы внедрён `error.data.cabal_code` + `retryable` + `method/tool`.
- Добавлен tool `cabal.get_error_codes` для машинного чтения каталога отказов IDE-клиентами.
- Добавлены unit-тесты классификатора ошибок (`revision`, `nonce replay`, `gate fail`).
- `cargo test`: PASS.

### 2026-02-24 (update-8)
- Реализован Audit v2: `cabal.query_audit_log`, `cabal.export_audit_log`, `cabal.replay_audit_state`.
- В audit-записи добавлены `event_id` и `digest_sha256`.
- Добавлены integration-тесты: фильтрация/реплей и экспорт audit-среза.
- Обновлён runtime README по новым audit-возможностям.
- `cargo test`: PASS.

### 2026-02-24 (update-9)
- Расширен `cabal.route_consult` до v2: `consult_type`, `priority`, `preferred_role`, `request_id`.
- В режиме `YOLO` реализован обязательный route к `orchestrator` с выбором исполнителя по типу CONSULT.
- Добавлен аудит маршрутизации (`consult.routed`) с данными route/dispatch.
- Добавлены unit/integration tests для consult routing v2.
- `cargo test`: PASS.

### 2026-02-24 (update-10)
- Расширен классификатор ошибок до production-набора: `STORAGE_FAILURE`, `IO_FAILURE`, `STATE_CORRUPT`.
- Уточнены правила `SIGNATURE_INVALID` (включая revoke/expiry/key-env/algorithm).
- Добавлены unit-тесты для новых error-классов.
- `P8` доведён до 82% (основной mapping runtime покрыт).
- `cargo test`: PASS.

### 2026-02-24 (update-11)
- Закрыты transport/protocol edge-cases в `P8`: добавлены `PARSE_ERROR` и `TRANSPORT_ERROR`.
- Основной цикл сервера больше не падает на malformed MCP frames: возвращает JSON-RPC error с `cabal_code`.
- Добавлены unit-тесты для классификации parse/transport ошибок и протокольные тесты `src/protocol.rs`.
- `P8` доведён до 90% (runtime + transport mapping покрыт).
- `cargo test`: PASS.

### 2026-02-24 (update-12)
- Добавлена версионированная SDK-спека кодов ошибок: `spec/docs/CABAL_ERROR_CODES.md`.
- Зафиксирован контракт формата `error.data` и таблица `cabal_code -> rpc_code`.
- README runtime синхронизирован ссылкой на SDK-спеку.
- `P8` доведён до 94% (осталась межклиентская parity-проверка).

### 2026-02-24 (update-13)
- Добавлен tool `cabal.validate_error_codes_parity` (проверка runtime codes vs `CABAL_ERROR_CODES.md`).
- Добавлены unit-тесты parity в `src/errors.rs` и integration-тест parity-инструмента.
- Зафиксирована автоматическая проверка паритета в CI-прохождении `cargo test`.
- `P8` доведён до 97% (осталась только IDE E2E parity-проверка по клиентам).
- `cargo test`: PASS.

### 2026-02-24 (update-14)
- Реализована policy-driven конфигурация CONSULT:
  - `cabal.get_consult_routing`
  - `cabal.set_consult_routing_rule`
  - `cabal.set_consult_priority_timeout`
- `cabal.route_consult` использует runtime-матрицу маршрутизации и SLA timeout policy.
- Добавлены unit-тесты на custom routing/timeout + integration-тест policy-driven routing matrix.
- `P6` доведён до 68%, `P8` до 98%.
- `cargo test`: PASS.

### 2026-02-24 (update-15)
- Расширен policy-driven CONSULT-контур:
  - `cabal.set_consult_retry_limit`
  - `cabal.set_consult_escalation_target`
  - `cabal.set_consult_allowed_roles`
- `cabal.route_consult` теперь строго применяет role allowlist: при mismatch выбирается только разрешённый fallback-исполнитель, без silent-dispatch в запрещённую роль.
- Добавлен unit-тест отказа при некорректной role-конфигурации (`empty allowed_roles`) и обновлён integration-тест policy matrix.
- `P6` доведён до 78%.
- `cargo test`: PASS.

### 2026-02-24 (update-16)
- `cabal.route_consult` расширен обязательными полями контракта: `actor` и `policy_revision`.
- Эти же поля добавлены в audit-событие `consult.routed` для replay/forensics согласованности.
- Обновлены unit/integration tests на новый contract и на strict role fallback.
- `P6` доведён до 86%.
- `cargo test`: PASS.

### 2026-02-24 (update-17)
- Реализован адаптивный (эмерджентный) слой `Consult Router`:
  - `cabal.get_adaptive_router`
  - `cabal.set_adaptive_router`
  - `cabal.record_consult_feedback`
- Добавлена telemetry-модель исполнителей (`success/fail/latency`) и адаптивный выбор executor в `cabal.route_consult`.
- Добавлен `routing_decision` (`strategy`, `score`, `confidence`, `confidence_floor`) в ответ маршрутизации и audit `consult.routed`.
- Добавлены unit/integration tests на adaptive routing и confidence-floor fallback.
- `P1` доведён до 82%, `P6` до 92%.
- `cargo test`: PASS.

### 2026-02-24 (update-18)
- Начат `P9` модульный рефакторинг без изменения MCP API:
  - добавлен `src/core/mod.rs`,
  - добавлен `src/core/router.rs`,
  - в core вынесены telemetry structs и adaptive scoring/selection функции.
- `runtime.rs` переключён на `core::router` (убран дублирующий scorer/selector код в runtime).
- Добавлены unit-тесты core-роутера.
- `P9` доведён до 15% (breaking changes нет, все текущие tests зелёные).
- `cargo test`: PASS.

### 2026-02-24 (update-19)
- `P9` продолжен: в `src/core/router.rs` вынесены:
  - CONSULT defaults (`routing/timeout/retry/escalation/allowed_roles`),
  - normalization (`priority/escalation_target`),
  - канонический `select_executor_for_consult`.
- `runtime.rs` переведён на core-реализацию через thin wrappers без изменения MCP API.
- `P9` доведён до 24%.
- `cargo test`: PASS.

### 2026-02-24 (update-20)
- Добавлены unit-тесты `core/router` на normalization/default-role invariants.
- Рефакторинг `P9` зафиксирован дополнительным test coverage без изменения внешнего MCP API.
- `P9` доведён до 26%.
- `cargo test`: PASS.

### 2026-02-24 (update-21)
- Добавлен интеграционный `tests/mcp_stdio_e2e.rs`:
  - поднимает runtime бинарь как subprocess,
  - проверяет `initialize -> tools/call` цепочку,
  - валидирует adaptive routing через реальный MCP transport (`stdio` + framed response).
- `P10` переведён в `in_progress` и доведён до 8% (сформирован transport-level smoke baseline).
- `cargo test`: PASS.

### 2026-02-24 (update-22)
- `P9` продолжен: добавлен `src/core/audit.rs` с выносом:
  - query-фильтрации audit (`kind/phase/revision/request_id/time/limit`),
  - replay snapshot логики (`phase/policy_revision/consult_mode/evidence/events`).
- `runtime.rs` переведён на thin-wrapper вызовы core-audit (`query_audit_log` / `replay_audit_state`), поведение MCP API сохранено.
- Добавлены unit-тесты `core/audit` + подтверждена совместимость через integration и stdio E2E тесты.
- `P9` доведён до 33%.
- `cargo test`: PASS.

### 2026-02-24 (update-23)
- `P9` продолжен: в `core/audit` дополнительно вынесены audit I/O операции:
  - `append_audit_record`,
  - `read_audit_items`.
- `runtime.rs` переключён на core-audit для `append/read/query/replay`; публичный MCP контракт сохранён без изменений.
- Добавлен unit-тест `core/audit::append_and_read_roundtrip`.
- `P9` доведён до 40%.
- `cargo test`: PASS.

### 2026-02-24 (update-24)
- `P9` продолжен: добавлен `src/core/phase.rs`:
  - `is_valid_transition`,
  - `phase_order_index`,
  - `required_exit_evidence`.
- `runtime.rs` переключён на core-phase wrappers без изменения внешнего MCP API/поведения.
- Добавлены unit-тесты `core/phase`.
- `P9` доведён до 45%.
- `cargo test`: PASS.

### 2026-02-24 (update-25)
- `P9` продолжен: добавлен `src/core/gate.rs` с переносом `GateCheckItem`, `GateReport`, `gate_item`.
- `runtime.rs` использует core-gate типы/конструктор, локальные дубли удалены.
- Поведение gate-валидации не изменено, покрытие тестами сохранено.
- `P9` доведён до 50%.
- `cargo test`: PASS.

### 2026-02-24 (update-26)
- `P9` продолжен: добавлен `src/core/proxy.rs` и вынесена proxy decision логика (`evaluate_proxy_request`).
- `runtime.rs::proxy_request` переведён на core-proxy без изменения формата ответа.
- Добавлены unit-тесты `core/proxy`.
- `P9` доведён до 56%.
- `cargo test`: PASS.

### 2026-02-24 (update-27)
- `P9` продолжен: gate engine логика полностью вынесена в `src/core/gate_engine.rs` (`build_gate_report` + markdown/file checks).
- `runtime.rs` переключён на `core::gate_engine` через thin adapter (`GateEvalContext`), удалены дублирующие gate helper-функции из runtime.
- Удалены неиспользуемые phase wrappers после миграции (`dead_code` cleanup), поведение MCP API сохранено.
- `P9` доведён до 63%.
- `cargo test`: PASS.

### 2026-02-24 (update-28)
- `P9` продолжен: добавлен `src/core/policy.rs`, вынесены policy-signing сущности/функции (`PolicySigningKey`, verify signature, nonce replay guard, signing message).
- `runtime.rs` переключён на core-policy через thin wrappers, дублирующий cryptographic/policy код удалён из runtime.
- Добавлен `src/core/events.rs`, вынесены `EventRecord`, `summarize_payload`, `truncate_text`; runtime использует core-events без изменения MCP контракта.
- Добавлены unit-тесты `core/policy` и `core/events`.
- `P9` доведён до 72%.
- `cargo test`: PASS.

### 2026-02-24 (update-29)
- `P9` продолжен: в `src/core/router.rs` вынесены consult resolver-функции:
  - timeout/retries/escalation resolution;
  - executor resolution + allowed-role fallback;
  - role allow-check + adaptive resolver.
- `runtime.rs::route_consult` переведён на `core/router` helpers, локальные дублирующие методы удалены.
- Добавлены unit-тесты `core/router` для override/fallback/allowlist поведения.
- `P9` доведён до 79%.
- `cargo test`: PASS.

### 2026-02-24 (update-30)
- `P9` продолжен: добавлен `src/core/proxy_exec.rs` с переносом:
  - `resolve_safe_repo_path`,
  - `exec_fs`,
  - `exec_shell`,
  - `exec_network`.
- `runtime.rs` переведён на core proxy-exec helpers (`validate_error_codes_parity`, `export_audit_log`, `proxy_execute`), локальные дублирующие методы удалены.
- Добавлены unit-тесты `core/proxy_exec` (path traversal guard + fs read/write roundtrip).
- `P9` доведён до 85%.
- `cargo test`: PASS.

### 2026-02-24 (update-31)
- `P9` продолжен: добавлен `src/core/fsm.rs` с переносом:
  - phase transition decision (`transition_phase`),
  - strict gate validation (`validate_strict_phase_transition`).
- `runtime.rs::transition_phase` и `runtime.rs::transition_phase_strict` переведены на core-fsm helpers без изменения MCP контракта.
- Добавлены unit-тесты `core/fsm` (invalid transition / strict gate fail / strict pass).
- `P9` доведён до 90%.
- `cargo test`: PASS.

### 2026-02-24 (update-32)
- `P9` продолжен: в `src/core/proxy.rs` вынесены proxy-trace сущности/хелперы:
  - `ProxyTraceRecord`,
  - `proxy_trace_hash_input`,
  - `build_proxy_trace_record`.
- `runtime.rs::append_proxy_trace` переведён на core-proxy helpers; локальная дублирующая структура и сборка записи удалены.
- Добавлены unit-тесты `core/proxy` на формат hash-input и builder.
- `P9` доведён до 94%.
- `cargo test`: PASS.

### 2026-02-24 (update-33)
- `P9` продолжен: в `runtime.rs::apply_policy` удалён промежуточный wrapper-слой `verify_policy_signature/register_policy_nonce`.
- Policy signing в runtime теперь вызывает `core/policy` напрямую (verify + nonce register), поведение/контракт без изменений.
- `P9` доведён до 96%.
- `cargo test`: PASS.

### 2026-02-24 (update-34)
- `P9` продолжен: в `src/core/events.rs` добавлены:
  - `event_hash_material`,
  - `build_event_record`.
- `runtime.rs::record_event` переведён на core-events helpers (hash material + record build), локальная дублирующая логика удалена.
- Добавлены unit-тесты `core/events` на event hash material и build_event_record.
- `P9` доведён до 98%.
- `cargo test`: PASS.

### 2026-02-24 (update-35)
- `P9` завершён: в `runtime.rs` удалены финальные consult normalize wrappers, вызовы переведены на `core/router` напрямую.
- Проведён финальный sweep core-refactor без изменения MCP API/контрактов.
- `P9` переведён в `done` (100%).
- `cargo test`: PASS.

### 2026-02-24 (update-36)
- `P11` продолжен: в `tests/runtime_api.rs` добавлены anti-bypass integration tests:
  - блок path traversal (`../...`) через `proxy_execute` для `fs`,
  - блок absolute path в `allow_by_default` режиме.
- Зафиксировано, что FS safety guard работает независимо от policy-mode proxy.
- `P11` доведён до 62%.
- `cargo test`: PASS.

### 2026-02-24 (update-37)
- `P10/P11` продолжены: в `tests/mcp_stdio_e2e.rs` добавлен transport-level сценарий:
  - `initialize`,
  - `tools/call(cabal.set_proxy_policy)` -> `allow_by_default`,
  - `tools/call(cabal.proxy_execute)` с `target=../secret.txt`,
  - проверка JSON-RPC error `error.data.cabal_code == INVALID_REQUEST`.
- Подтверждён anti-bypass path guard на реальном MCP stdio transport.
- `P10` доведён до 12%, `P11` до 66%.
- `cargo test`: PASS.

### 2026-02-24 (update-38)
- `P10/P11` расширены: в stdio E2E anti-bypass тест добавлена проверка absolute path запрета через `cabal.proxy_execute`.
- Подтверждено, что для traversal и absolute-path нарушений на MCP transport возвращается `error.data.cabal_code=INVALID_REQUEST`.
- `P10` доведён до 13%, `P11` до 68%.
- `cargo test`: PASS.

### 2026-02-24 (update-40)
- `P10/P11` расширены transport-level error-contract сценариями в `tests/mcp_stdio_e2e.rs`:
  - `cabal.transition_phase_strict` без evidence -> JSON-RPC error с `cabal_code=GATE_FAIL`;
  - `cabal.proxy_execute` с allowlist-match на unsupported category -> JSON-RPC error с `cabal_code=PROXY_DENY`.
- Подтверждена корректная классификация ошибок на реальном stdio MCP transport.
- `P10` доведён до 16%, `P11` до 72%.
- `cargo test`: PASS.

### 2026-02-24 (update-41)
- `P10` продолжен: в stdio E2E добавлены базовые transport contract проверки классификации ошибок:
  - unknown tool -> `cabal_code=UNKNOWN_TOOL`,
  - unsupported method -> `cabal_code=UNSUPPORTED_METHOD`.
- Подтверждён корректный JSON-RPC error contract для клиентского method/tool layer.
- `P10` доведён до 20%.
- `cargo test`: PASS.

### 2026-02-24 (update-42)
- `P10` продолжен transport-level protocol fault сценариями в `tests/mcp_stdio_e2e.rs`:
  - malformed NDJSON -> `cabal_code=PARSE_ERROR`,
  - framed request без `Content-Length` -> `cabal_code=TRANSPORT_ERROR`.
- Проверено, что в обоих случаях runtime возвращает JSON-RPC error с `id=null` и корректной machine-readable классификацией.
- `P10` доведён до 26%.
- `cargo test`: PASS.

### 2026-02-24 (update-43)
- `P10` продолжен: в stdio E2E добавлен отдельный профиль входящих `Content-Length` framed-запросов:
  - `initialize` (framed),
  - `tools/list` (framed),
  - `tools/call(cabal.set_consult_mode)` (framed).
- Подтверждена корректная работа runtime с framed-входом (не только NDJSON), что критично для IDE MCP-клиентов.
- `P10` доведён до 33%.
- `cargo test`: PASS.

### 2026-02-24 (update-44)
- README runtime обновлён: тестовый транспортный контур явно описывает оба профиля входа (`NDJSON` и `Content-Length framed`).
- Документация синхронизирована с актуальным покрытием `tests/mcp_stdio_e2e.rs`.
- `cargo test`: PASS.

### 2026-02-24 (update-45)
- `P10` продолжен: добавлен mixed-mode stdio E2E сценарий (framed `initialize` + NDJSON `tools/call` + framed `tools/list` в одной сессии).
- Подтверждена устойчивость runtime-парсера к переключению транспортного формата в рамках одного MCP-подключения.
- `P10` доведён до 38%.
- `cargo test`: PASS.

### 2026-02-24 (update-46)
- `P10/P11` расширены: добавлен stdio E2E сценарий policy-deny semantics для `cabal.proxy_execute`:
  - deny path (`network/http_get` при default proxy policy) возвращает `result` (`allow=false, executed=false`), а не JSON-RPC error.
- Зафиксирован контракт client-side обработки deny-path для MCP IDE клиентов.
- `P10` доведён до 41%, `P11` до 74%.
- `cargo test`: PASS.

### 2026-02-24 (update-47)
- `P10/P11` значительно расширены на transport-level (`tests/mcp_stdio_e2e.rs`):
  - revision mismatch -> `REVISION_MISMATCH`,
  - signed policy without signature (with nonce) -> `SIGNATURE_INVALID`,
  - invalid consult priority -> `INVALID_REQUEST`,
  - unsupported gate kind -> `GATE_FAIL`,
  - mixed framed/ndjson session compatibility.
- Уточнён тестовый кейс signed-policy: добавлен `nonce` для проверки именно signature class.
- `P10` доведён до 52%, `P11` до 77%.
- `cargo test`: PASS.

### 2026-02-24 (update-48)
- README runtime дополнен явным перечнем transport-level error-contract coverage в stdio E2E suite.
- Документация синхронизирована с фактическим тестовым покрытием (`15` stdio E2E tests).
- `cargo test`: PASS.

### 2026-02-24 (update-49)
- `P10/P11` расширены дополнительными stdio E2E error-contract сценариями:
  - `IO_FAILURE` (чтение отсутствующего файла через allowlisted fs-path),
  - `STORAGE_FAILURE` (экспорт audit в directory path),
  - `REVISION_MISMATCH`, `SIGNATURE_INVALID`, `INVALID_REQUEST`, `GATE_FAIL` через `tools/call` на transport уровне.
- В `src/errors.rs` добавлен unit-тест классификации `STATE_CORRUPT`.
- README transport error-contract список синхронизирован с новым покрытием.
- Фактическое покрытие stdio E2E: `17` тестов.
- `P10` доведён до 58%, `P11` до 80%.
- `cargo test`: PASS.

### 2026-02-24 (update-50)
- `P10` расширен transport-level функциональными E2E сценариями:
  - `cabal.validate_error_codes_parity` в stdio runtime-сессии (parity pass),
  - `cabal.route_consult` + `cabal.query_audit_log` проверка `consult.routed` audit contract (`request_id/actor/policy_revision`).
- Частично усилен `P6` за счёт дополнительной stdio E2E-валидации consult routing contract.
- Фактическое покрытие stdio E2E: `19` тестов.
- `P6` доведён до 94%, `P10` до 64%, `P11` до 82%.
- `cargo test`: PASS.

### 2026-02-24 (update-51)
- README runtime дополнен transport-level coverage пунктами для:
  - consult/audit contract checks,
  - parity-tool path в stdio E2E.
- Документация синхронизирована с текущим stdio E2E покрытием (`19` тестов).
- `cargo test`: PASS.

### 2026-02-24 (update-52)
- `P10/P11` расширены IDE profile policy контуром:
  - добавлен core-модуль `src/core/ide.rs` (detector/normalizer/allow-check),
  - `initialize` теперь регистрирует IDE client session и active profile (`ide.client_initialized` audit),
  - добавлены tools `cabal.get_ide_profile_policy` и `cabal.set_ide_profile_policy`,
  - при `enforce_ide_profile=true` disallowed profile блокируется на `initialize` с `POLICY_DENY`.
- Расширено transport-level покрытие `tests/mcp_stdio_e2e.rs`:
  - `initialize` tracking для VS Code профиля,
  - profile enforcement deny-case для disallowed IDE profile,
  - profile enforcement allow-case для JetBrains profile.
- Расширено покрытие unit/integration:
  - runtime tests на IDE profile normalization/enforcement,
  - integration tests на register+deny поведение,
  - errors classifier test на `POLICY_DENY` для IDE profile block.
- Фактическое покрытие stdio E2E: `22` теста.
- `P10` доведён до 74%, `P11` до 85%.
- `cargo test`: PASS.

### 2026-02-24 (update-53)
- `P7` существенно продвинут: реализованы audit rotation/compression/signature в runtime и core:
  - добавлены tools `cabal.rotate_audit_log` и `cabal.verify_audit_archive`,
  - rotate пишет архив (`jsonl`/`jsonl.gz`) и `sha256` sidecar,
  - verify проверяет digest parity и возвращает machine-readable `pass`.
- Расширены тесты:
  - `core/audit` unit tests (rotate+verify roundtrip + tamper detect),
  - `runtime_api` integration tests (rotate/verify + tamper path),
  - `mcp_stdio_e2e` transport tests (rotate/verify + empty-log `INVALID_REQUEST`).
- README синхронизирован с новым audit hardening API.
- Фактическое покрытие stdio E2E: `24` теста.
- `P7` доведён до 89%, `P10` до 76%, `P11` до 87%.
- `cargo test`: PASS.

### 2026-02-24 (update-54)
- `P7` расширен retention-слоем:
  - добавлен core API `prune_audit_archives` (архивы + sidecar, policy `keep_last`),
  - добавлен runtime/tool `cabal.prune_audit_archives`,
  - расширен error contract для `keep_last must be > 0`.
- Добавлены тесты:
  - `core/audit` unit tests на prune (keep latest + invalid keep_last),
  - `runtime_api` integration test на prune flow после нескольких ротаций,
  - `mcp_stdio_e2e` transport test на prune через MCP tools.
- README синхронизирован с `rotate/verify/prune` audit hardening API.
- Фактическое покрытие: `75` unit, `25` stdio E2E, `21` integration.
- `P7` доведён до 93%, `P10` до 78%, `P11` до 88%.
- `cargo test`: PASS.

### 2026-02-24 (update-55)
- `P7` расширен auto-rotation trigger слоем:
  - добавлены tools `cabal.get_audit_rotation_policy` и `cabal.set_audit_rotation_policy`,
  - в runtime-state добавлены threshold-поля (`enabled/max_bytes/max_age_sec/compress/keep_last/archive_dir/last_rotation_unix`),
  - append-audit path теперь выполняет rotate+prune автоматически по size/time triggers.
- Manual audit tools синхронизированы с policy:
  - `cabal.rotate_audit_log` использует policy defaults и возвращает `archive + prune`,
  - `cabal.prune_audit_archives` использует default `keep_last` policy.
- Расширены тесты:
  - runtime unit tests на set/get audit rotation policy,
  - integration tests на auto-rotate by size и by age,
  - stdio E2E tests на `get/set_audit_rotation_policy`, auto-rotate audit event и invalid prune request.
- README синхронизирован с новым auto-rotation policy API.
- Фактическое покрытие: `78` unit, `28` stdio E2E, `24` integration.
- `P7` доведён до 96%, `P10` до 80%, `P11` до 90%.
- `cargo test`: PASS.

### 2026-02-24 (update-56)
- `P10/P11` усилены anti-bypass контуром IDE profile policy:
  - в `RuntimeState` добавлен флаг `require_ide_client_info`,
  - `cabal.set_ide_profile_policy` поддерживает `require_client_info`,
  - при `enforce_ide_profile=true` + `require_client_info=true` runtime блокирует `initialize` без `clientInfo.name` (`POLICY_DENY`).
- Обновлены контракты tools/docs:
  - `cabal.get_ide_profile_policy` возвращает `require_client_info`,
  - schema/description для `cabal.set_ide_profile_policy` синхронизированы.
- Расширены тесты:
  - runtime unit test на deny-path для missing `client_info.name`,
  - integration test на deny-path при required client info,
  - stdio E2E test на `initialize` без `clientInfo.name` -> `POLICY_DENY`.
- Фактическое покрытие: `79` unit, `29` stdio E2E, `25` integration.
- `P10` доведён до 82%, `P11` до 92%.
- `cargo test`: PASS.

### 2026-02-24 (update-57)
- Для IDE anti-bypass `require_client_info` добавлен симметричный allow-path:
  - integration test: enforcement + required client info + allowed profile -> `PASS`,
  - stdio E2E test: `initialize` с `clientInfo.name` при `require_client_info=true` -> `PASS`.
- Контракт `require_client_info` теперь проверен на transport-level для обоих путей (`deny` и `allow`).
- Фактическое покрытие: `79` unit, `30` stdio E2E, `26` integration.
- `P10` доведён до 83%, `P11` до 93%.
- `cargo test`: PASS.

### 2026-02-24 (update-58)
- `P5/P10/P11` усилены entry-gate проверкой ознакомления со сквозными правилами:
  - `src/core/gate_engine.rs`: на вход любой фазы обязательны evidence `cross_rules_agent_ack` и `cross_rules_subagent_ack`,
  - `tests/runtime_api.rs`: добавлены проверки deny/pass для entry gate по ack evidence и обновлён strict transition pass-path,
  - `tests/mcp_stdio_e2e.rs`: добавлен transport-level сценарий `transition_phase_strict` (deny без ack, pass после ack).
- README runtime синхронизирован: добавлен раздел `Gate Entry Rules` с обязательными evidence ключами.
- Фактическое покрытие: `79` unit, `34` stdio E2E, `27` integration.
- `P5` доведён до 81%, `P10` до 84%, `P11` до 94%.
- `cargo test`: PASS.

### 2026-02-24 (update-59)
- `P5/P10/P11` расширены policy-контуром strict gate artifacts:
  - `RuntimeState` получил флаг `strict_gate_artifacts`,
  - добавлены tools `cabal.get_gate_policy` и `cabal.set_gate_policy`,
  - `build_gate_report` учитывает strict-mode и валидирует required phase/global/canon files через checks `entry_required_files_present`/`exit_required_files_present`.
- Добавлены тесты:
  - runtime unit: `set_gate_policy_updates_values`,
  - integration: toggling strict-mode и проверки required-files checks,
  - stdio E2E: `set/get_gate_policy` + gate report contract в strict-mode.
- README синхронизирован с новым Gate Policy API и `strict_artifacts` semantics.
- Фактическое покрытие: `80` unit, `35` stdio E2E, `28` integration.
- `P5` доведён до 85%, `P10` до 86%, `P11` до 95%.
- `cargo test`: PASS.

### 2026-02-24 (update-60)
- `P6/P10` расширены IDE-aware consult contract:
  - `route_consult` теперь возвращает `ide_profile` и `ide_client_name`,
  - `consult.routed` audit payload теперь также содержит `ide_profile` и `ide_client_name`.
- Тесты:
  - integration: проверка consult-route с активным IDE профилем (`vscode`) и audit payload contract,
  - stdio E2E: `mcp_stdio_route_consult_audit_contract_fields_present` расширен проверкой IDE context полей.
- README синхронизирован: consult routing contract и test coverage теперь явно включают IDE context.
- Фактическое покрытие: `80` unit, `35` stdio E2E, `29` integration.
- `P6` доведён до 95%, `P10` до 87%, `P11` 95%.
- `cargo test`: PASS.

### 2026-02-24 (update-61)
- `P4/P10/P11` усилены transport-level policy-signing lifecycle сценариями:
  - stdio E2E: `apply_policy_bundle` с `expired` key_id -> `SIGNATURE_INVALID`,
  - stdio E2E: `apply_policy_bundle` с `revoked` key_id -> `SIGNATURE_INVALID`.
- README синхронизирован: transport coverage явно включает expired/revoked signing-key paths.
- Фактическое покрытие: `80` unit, `37` stdio E2E, `29` integration.
- `P4` доведён до 86%, `P10` до 88%, `P11` до 96%.
- `cargo test`: PASS.

### 2026-02-24 (update-62)
- `P3/P11` усилены shell anti-bypass guardrails в Tool Proxy:
  - `src/core/proxy_exec.rs`: добавлен denylist опасных shell command fragments и ранний блок `shell command blocked by policy`,
  - `src/errors.rs`: классификация blocked-shell path как `PROXY_DENY` для `cabal.proxy_execute`,
  - `tests/runtime_api.rs`: integration test для blocked shell command в `allow_by_default`,
  - `tests/mcp_stdio_e2e.rs`: transport-level test (`cabal.proxy_execute`) с `PROXY_DENY` на опасной shell команде.
- README синхронизирован: добавлено описание shell safety policy и transport coverage.
- Фактическое покрытие: `80` unit, `38` stdio E2E, `30` integration.
- `P3` доведён до 76%, `P11` до 97%.
- `cargo test`: PASS.

### 2026-02-24 (update-63)
- `P2/P10` расширены CPU policy-контуром:
  - добавлены tools `cabal.get_cpu_policy` и `cabal.set_cpu_policy`,
  - `RuntimeState` хранит `require_zen4_fast_path`,
  - startup теперь валидирует CPU policy (`validate_cpu_policy`) до начала MCP цикла.
- Контракт CPU policy:
  - `require_zen4_fast_path=true` разрешается только при `zen4_avx512` execution path, иначе `policy deny`.
- Добавлены тесты:
  - unit: `set_cpu_policy_validates_zen4_requirement`,
  - integration: roundtrip `set/get` + `get_state` snapshot для `cpu_policy`,
  - stdio E2E: `cabal.set_cpu_policy` + `cabal.get_cpu_policy` contract path.
- README синхронизирован: добавлен раздел `CPU Policy` и tools list обновлён.
- Фактическое покрытие: `84` unit, `39` stdio E2E, `31` integration.
- `P2` доведён до 72%, `P3` до 78%, `P10` до 90%, `P11` до 98%.
- `cargo test`: PASS.

### 2026-02-24 (update-64)
- `P3/P10/P11` расширены operation-level proxy policy:
  - `RuntimeState` получил `proxy_allowed_operations` и `proxy_denied_operations`,
  - `proxy_request` теперь проверяет operation allowlist/denylist до target allowlist и исполнения,
  - добавлены tools `cabal.get_proxy_operation_policy` и `cabal.set_proxy_operation_policy`.
- Добавлены тесты:
  - core unit tests на operation allowlist/denylist resolver в `src/core/proxy.rs`,
  - runtime unit/integration на policy update и deny before execution,
  - stdio E2E на `set_proxy_operation_policy` + deny result contract (`allow=false`, `executed=false`).
- README синхронизирован: tools list и transport coverage дополнены operation policy path.
- Фактическое покрытие: `87` unit, `40` stdio E2E, `32` integration.
- `P3` доведён до 82%, `P10` до 91%, `P11` до 99%.
- `cargo test`: PASS.

### 2026-02-24 (update-65)
- `P6/P10` расширены IDE profile matrix для consult routing contract:
  - добавлен stdio E2E сценарий `consult.routed` для `jetbrains` профиля (`IntelliJ IDEA`),
  - проверены `ide_profile`/`ide_client_name` как в route response, так и в audit payload.
- README синхронизирован: явно зафиксирован transport coverage для `vscode + jetbrains`.
- Фактическое покрытие: `87` unit, `41` stdio E2E, `32` integration.
- `P6` доведён до 96%, `P10` до 92%, `P11` 99%.
- `cargo test`: PASS.

### 2026-02-24 (update-66)
- `P3/P11` расширены network safety guardrails в Tool Proxy:
  - `src/core/proxy_exec.rs`: добавлен preflight validator `ensure_network_target_allowed` (scheme + localhost/private/link-local/metadata host blocking),
  - `src/errors.rs`: network policy block для proxy_execute классифицируется как `PROXY_DENY`.
- Добавлены тесты:
  - core unit: network target allow/block cases,
  - integration: `proxy_execute(network/http_get)` блокирует local target даже в `allow_by_default`,
  - stdio E2E: transport-level `PROXY_DENY` для local network target.
- README синхронизирован: добавлен раздел `Network safety policy`.
- Фактическое покрытие: `91` unit, `42` stdio E2E, `33` integration.
- `P3` доведён до 86%, `P10` до 93%, `P11` закрыт на 100%.
- `cargo test`: PASS.

### 2026-02-24 (update-67)
- `P2/P10` расширены CPU feature-policy контрактом:
  - `cabal.set_cpu_policy` теперь поддерживает feature flags: `require_avx512f`, `require_avx512vl`, `require_fma`, `require_bmi2`, `require_sha`,
  - `validate_cpu_policy` применяет те же feature-требования на startup/runtime.
- Добавлены тесты:
  - runtime unit: feature requirement pass/fail checks,
  - stdio E2E: dynamic unavailable-requirement path (`POLICY_DENY`) через `cabal.get_capabilities` + `cabal.set_cpu_policy`.
- README синхронизирован: CPU policy раздел дополнен feature flags semantics.
- Фактическое покрытие: `92` unit, `43` stdio E2E, `33` integration.
- `P2` доведён до 78%, `P10` до 94%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-24 (update-68)
- `P10` расширен сквозным IDE transport chain контрактом:
  - stdio E2E: `set_ide_profile_policy` (enforce + require_client_info) -> `initialize` disallowed IDE (`POLICY_DENY`) -> `initialize` allowed IDE (`jetbrains`) -> `route_consult`,
  - подтверждён контекст `ide_profile`/`ide_client_name` в consult routing response после успешного enforce chain.
- README синхронизирован: IDE profile handshake/enforcement coverage уточнён как chain path.
- Фактическое покрытие: `92` unit, `44` stdio E2E, `33` integration.
- `P2` доведён до 80%, `P10` до 95%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-24 (update-69)
- `P2` усилен startup validation fail-path тестом:
  - integration test форсирует несовместимую CPU policy в `.cabal_runtime/state.json` и подтверждает fail startup процесса runtime,
  - проверен stderr contract (`policy deny`) для boot-time policy enforcement.
- Этот сценарий покрывает реальный boot-контур (`main -> load_or_create -> validate_cpu_policy`) и снижает риск silent-start с невалидной persisted CPU policy.
- Фактическое покрытие: `92` unit, `44` stdio E2E, `34` integration.
- `P2` доведён до 81%, `P10` 95%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-24 (update-70)
- `P7` расширен unified audit health API:
  - добавлен runtime/tool `cabal.audit_health_check` с параметрами `archive_dir` и `verify_last`,
  - отчёт включает `active_log` (path/bytes/line_count/last-event), `rotation_policy`, и срез архивов (`total/checked/passed/failed/missing_signature/items`),
  - при верификации архивов используется `core_verify_audit_archive`, итоговый статус выставляется как `pass/warn/fail`.
- Усилен error-contract для нового API:
  - `verify_last must be > 0` классифицируется как `INVALID_REQUEST`.
- Добавлены тесты:
  - runtime unit: `audit_health_check_rejects_zero_verify_last`,
  - integration: `integration_audit_health_check_pass_and_fail_paths` (pass до tamper и fail после tamper),
  - stdio E2E: `mcp_stdio_audit_health_check_pass_and_fail_paths` + invalid-path `verify_last=0`.
- README runtime синхронизирован с новым tool и audit v2 semantics.
- Фактическое покрытие: `93` unit, `46` stdio E2E, `35` integration.
- `P7` доведён до 98%, `P10` 95%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-24 (update-71)
- `P3/P8/P10` синхронизирован proxy network error-contract:
  - `src/errors.rs`: `invalid network target url` для `cabal.proxy_request/cabal.proxy_execute` теперь маппится в `PROXY_DENY` (`-32031`),
  - `IO_FAILURE` path для прочих файловых операций не изменён.
- Добавлены тесты:
  - error unit: `classify_proxy_deny_for_invalid_network_target_url`,
  - stdio E2E: `mcp_stdio_proxy_execute_network_invalid_url_is_proxy_deny`.
- README синхронизирован: network safety policy дополнен invalid-url semantic.
- Фактическое покрытие: `94` unit, `47` stdio E2E, `35` integration.
- `P3` доведён до 88%, `P7` 98%, `P10` 95%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-24 (update-72)
- `P6/P10` расширены adaptive exploration policy:
  - `RuntimeState` получил `adaptive_exploration_rate` и `adaptive_exploration_min_samples` (serde defaults для миграции старого state),
  - `cabal.get_adaptive_router`/`cabal.get_consult_routing` теперь возвращают exploration settings,
  - добавлен tool `cabal.set_adaptive_exploration_policy` с валидацией (`rate in [0,1]`, `min_samples > 0`).
- Расширен consult routing contract:
  - `route_consult` в YOLO + adaptive mode теперь поддерживает strategy `adaptive_explore`,
  - exploration selection детерминирован по seed (`request_id`/`question`) и выбирает недообученного исполнителя (`outcomes < min_samples`),
  - fallback behavior (`adaptive`/`policy_confidence_floor`) сохранён без регрессий.
- Добавлены тесты:
  - `core/router` unit tests на exploration rate bounds + undertrained selector,
  - runtime unit/integration на `set_adaptive_exploration_policy` и `adaptive_explore` route path,
  - stdio E2E на `set_adaptive_exploration_policy` invalid-request и `route_consult` exploration dispatch contract.
- README синхронизирован с новым tool и стратегией `adaptive_explore`.
- Фактическое покрытие: `99` unit, `49` stdio E2E, `36` integration.
- `P6` доведён до 98%, `P10` до 96%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-25 (update-73)
- `P3/P12` усилены network runtime guardrails:
  - `src/core/proxy_exec.rs`: для `http_get` добавлены connect/read/write таймауты через `ureq::AgentBuilder`,
  - добавлен bounded body reader c лимитом `8192` байт и машиночитаемыми полями `truncated`/`body_bytes`,
  - сохранён текущий fail-contract (`http_get failed`/`failed to read response body`) без изменения cabal_code mapping.
- Добавлены тесты:
  - `core/proxy_exec` unit: `read_limited_utf8_body_truncates_large_payload`,
  - `core/proxy_exec` unit: `read_limited_utf8_body_keeps_small_payload`.
- README синхронизирован с network timeout/body-limit semantics.
- Фактическое покрытие: `101` unit, `49` stdio E2E, `36` integration.
- `P3` доведён до 90%, `P12` переведён в `in_progress` и доведён до 8%, `P10` 96%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-25 (update-74)
- `P3/P12` усилены shell input guardrails:
  - `src/core/proxy_exec.rs`: `ensure_shell_command_allowed` теперь блокирует overlong command (`>1024`) до запуска shell,
  - для overlong path сохранена строгая отказная семантика без side-effects исполнения.
- Error-contract:
  - `shell target command is too long` маппится в `INVALID_REQUEST`.
- Добавлены тесты:
  - `core/proxy_exec` unit: `ensure_shell_command_allowed_blocks_overlong_command`,
  - `errors` unit: `classify_invalid_request_for_shell_command_too_long`,
  - stdio E2E: `mcp_stdio_proxy_execute_shell_overlong_command_is_invalid_request`.
- README синхронизирован с shell length-limit semantics.
- Фактическое покрытие: `103` unit, `50` stdio E2E, `36` integration.
- `P3` доведён до 92%, `P12` до 12%, `P10` 96%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-25 (update-75)
- `P3/P12` расширены bounded FS read guardrail:
  - `src/core/proxy_exec.rs`: `fs/read_text` переведён на bounded reader (`131072` bytes) вместо полного чтения файла,
  - в contract ответа добавлены `truncated` и `read_bytes` для machine-observable контроля усечения.
- Добавлены тесты:
  - `core/proxy_exec` unit: `exec_fs_read_text_is_bounded`.
- README синхронизирован с FS bounded-read semantics.
- Фактическое покрытие: `104` unit, `50` stdio E2E, `36` integration.
- `P3` доведён до 93%, `P12` до 16%, `P10` 96%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-25 (update-76)
- `P3/P12` расширены bounded shell output guardrails:
  - `src/core/proxy_exec.rs`: shell execution response теперь содержит bounded `stdout/stderr` с лимитом `4000` bytes,
  - добавлены machine-readable поля `stdout_truncated`, `stderr_truncated`, `stdout_bytes`, `stderr_bytes`.
- Добавлены тесты:
  - `core/proxy_exec` unit: `bounded_text_output_marks_truncation`.
- README синхронизирован с shell bounded-output semantics.
- Фактическое покрытие: `105` unit, `50` stdio E2E, `36` integration.
- `P3` доведён до 94%, `P12` до 20%, `P10` 96%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-25 (update-77)
- `P3/P12` расширены bounded FS write/list guardrails:
  - `src/core/proxy_exec.rs`: `write_text` теперь блокирует oversized `payload.text` (`>1048576`) до записи,
  - `list_dir` возвращает bounded список (до `1000` entries) с machine-readable полями `truncated` и `total_entries`.
- Error-contract:
  - `payload.text is too large` классифицируется как `INVALID_REQUEST`.
- Добавлены тесты:
  - `core/proxy_exec` unit: `exec_fs_write_text_rejects_oversized_payload`,
  - `core/proxy_exec` unit: `exec_fs_list_dir_is_bounded`,
  - `errors` unit: `classify_invalid_request_for_oversized_write_text`,
  - stdio E2E: `mcp_stdio_proxy_execute_fs_write_oversized_payload_is_invalid_request`.
- README синхронизирован с FS write/list limits semantics.
- Фактическое покрытие: `108` unit, `51` stdio E2E, `36` integration.
- `P3` доведён до 96%, `P12` до 26%, `P10` 96%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-25 (update-78)
- `P3/P12` усилены bounded proxy-trace retention:
  - `src/runtime.rs`: добавлен `PROXY_LOG_MAX_ENTRIES=5000` и автоматический trim oldest entries при append,
  - `get_state` теперь возвращает `proxy_log_max_entries` для observability.
- Добавлены тесты:
  - runtime unit: `proxy_log_is_bounded_to_max_entries`.
- README синхронизирован с proxy-log bounded retention semantics.
- Фактическое покрытие: `109` unit, `51` stdio E2E, `36` integration.
- `P3` доведён до 97%, `P12` до 31%, `P10` 96%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-25 (update-79)
- `P3/P8/P12` усилены limit validation и payload caps:
  - `src/runtime.rs`: `cabal.get_proxy_log` переведён на Result-контракт с валидацией `limit>0`,
  - добавлен server-side cap для выдачи: `proxy_log max_limit=1000`, `audit query/export cap=2000`,
  - `query_audit_log` теперь стабилизирует bounded limit до core-layer.
- Error-contract:
  - `limit must be > 0` классифицируется как `INVALID_REQUEST`.
- Добавлены тесты:
  - runtime unit: `normalize_limit_rejects_zero`, `normalize_limit_caps_to_max`, `get_proxy_log_rejects_zero_limit`,
  - errors unit: `classify_invalid_request_for_zero_limit`,
  - stdio E2E: `mcp_stdio_get_proxy_log_zero_limit_is_invalid_request`.
- README синхронизирован с `get_proxy_log`/audit limit semantics.
- Фактическое покрытие: `113` unit, `52` stdio E2E, `36` integration.
- `P3` доведён до 98%, `P8` до 99%, `P12` до 36%, `P10` 96%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-25 (update-80)
- `P3/P12` усилены shell timeout/circuit-breaker:
  - `src/core/proxy_exec.rs`: `shell/run` переведён на execution path с timeout-wait и kill при exceed (`15s`),
  - bounded stdout/stderr contract сохранён, timeout path теперь явный и воспроизводимый (`shell command timed out`).
- Error-contract:
  - `shell command timed out` классифицируется как `EXECUTOR_FAILURE`.
- Добавлены тесты:
  - `core/proxy_exec` unit: `exec_shell_times_out`,
  - `errors` unit: `classify_executor_failure_for_shell_timeout`.
- README синхронизирован с shell timeout semantics.
- Фактическое покрытие: `115` unit, `52` stdio E2E, `36` integration.
- `P3` доведён до 99%, `P8` закрыт на 100%, `P12` до 43%, `P10` 96%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-25 (update-81)
- `P3/P10/P12` расширены transport-level shell timeout contract:
  - `src/core/proxy_exec.rs`: добавлен `shell_exec_timeout()` с env-override `CABAL_PROXY_SHELL_TIMEOUT_MS`,
  - `exec_shell` теперь использует timeout policy через runtime/env profile.
- Добавлены тесты:
  - `core/proxy_exec` unit: `shell_exec_timeout_uses_env_override_when_valid`,
  - stdio E2E: `mcp_stdio_proxy_execute_shell_timeout_is_executor_failure`.
- README синхронизирован с env-based timeout override semantics.
- Фактическое покрытие: `116` unit, `53` stdio E2E, `36` integration.
- `P3` закрыт на 100%, `P8` 100%, `P10` до 97%, `P12` до 50%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-25 (update-82)
- `P8/P10/P12` усилены audit/proxy limit contracts:
  - `src/runtime.rs`: `query_audit_log` теперь всегда возвращает `max_limit` и применяет централизованный bounded limit через `normalize_limit`,
  - `cabal.get_proxy_log` и `cabal.query_audit_log` блокируют `limit=0` независимо от input schema.
- Добавлены тесты:
  - runtime integration: `integration_query_audit_log_and_replay` проверяет `max_limit`,
  - stdio E2E: `mcp_stdio_query_audit_log_zero_limit_is_invalid_request`,
  - stdio E2E: `mcp_stdio_get_proxy_log_zero_limit_is_invalid_request`.
- README синхронизирован с `query_audit_log.max_limit` semantics.
- Фактическое покрытие: `116` unit, `54` stdio E2E, `36` integration.
- `P3` 100%, `P8` 100%, `P10` до 98%, `P12` до 55%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-25 (update-83)
- `P10/P12` расширены stress-контрактами для больших audit логов:
  - `tests/runtime_api.rs`: добавлен integration stress-сценарий `integration_audit_query_export_replay_large_log_are_capped`,
  - подтверждено bounded поведение: `query(limit=50000)`/`export(limit=50000)` ограничиваются `max_limit=2000`.
- Уточнены API контракты audit:
  - `export_audit_log` теперь возвращает `requested_limit`, `applied_limit`, `max_limit`,
  - `query_audit_log` в transport-path подтверждён тестом `mcp_stdio_query_audit_log_exposes_max_limit`.
- Добавлены/обновлены тесты:
  - runtime integration: `integration_query_audit_log_and_replay` проверяет `max_limit`,
  - stdio E2E: `mcp_stdio_query_audit_log_exposes_max_limit`.
- README синхронизирован с bounded-export limit semantics.
- Фактическое покрытие: `116` unit, `55` stdio E2E, `37` integration.
- `P3` 100%, `P8` 100%, `P10` до 99%, `P12` до 62%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-25 (update-84)
- `P12` расширен отдельным stress test profile:
  - добавлен `tests/runtime_stress.rs` с ignored-тестом `stress_audit_query_export_replay_profile`,
  - профиль прогоняет ingest/query/export/replay на `10_000` audit-событиях и валидирует bounded caps (`max_limit=2000`).
- Зафиксированы фактические тайминги stress-run:
  - ingest=`1030ms`, query=`108ms`, export=`130ms`, replay=`73ms`.
- README дополнен отдельной командой запуска stress-профиля:
  - `cargo test --test runtime_stress -- --ignored --nocapture`.
- Фактическое покрытие: `116` unit, `55` stdio E2E, `37` integration (+ `1` ignored stress).
- `P3` 100%, `P8` 100%, `P10` 99%, `P12` 68%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-25 (update-85)
- `P12` расширен multi-run latency harness:
  - `tests/runtime_stress.rs`: добавлен `stress_audit_query_export_replay_multirun_p95_p99`,
  - реализован p95/p99 расчёт для `query/export/replay` (5 прогонов по 5000 событий).
- Зафиксированы фактические multi-run метрики:
  - query `p95/p99=59/59ms`,
  - export `p95/p99=82/82ms`,
  - replay `p95/p99=36/36ms`.
- README синхронизирован: stress-профиль теперь явно включает single-run + multi-run сценарии.
- Фактическое покрытие: `116` unit, `55` stdio E2E, `37` integration (+ `2` ignored stress).
- `P3` 100%, `P8` 100%, `P10` 99%, `P12` 74%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-25 (update-86)
- `P12` формализован SLA-документом:
  - добавлен `spec/docs/CABAL_STRESS_SLA.md` с scope, dataset profile, thresholds и latest observed values,
  - stress-tests синхронизированы с именованными SLA-константами (`STRESS_SLA_*`) вместо magic numbers.
- Подтверждены свежие метрики после синхронизации:
  - single-run: ingest=`982ms`, query=`112ms`, export=`138ms`, replay=`75ms`,
  - multi-run: query `p95/p99=60/60ms`, export `86/86ms`, replay `37/37ms`.
- README дополнен ссылкой на SLA-документ.
- Фактическое покрытие: `116` unit, `55` stdio E2E, `37` integration (+ `2` ignored stress).
- `P3` 100%, `P8` 100%, `P10` 99%, `P12` 80%, `P11` 100%.
- `cargo test`: PASS.

### 2026-02-25 (update-87)
- `P12` усилен release-gate automation для stress SLA:
  - `scripts/check-stress-sla.ps1` переведён на стабильный execution path без проблемного stderr-capture,
  - добавлен `Push-Location` к корню `cabal-mcp-runtime`, чтобы скрипт корректно работал из любого `cwd`.
- Выполнена проверка gate из корня репозитория (вне подпроекта `cabal-mcp-runtime`):
  - команда: `powershell -ExecutionPolicy Bypass -File .\cabal-mcp-runtime\scripts\check-stress-sla.ps1`,
  - single-run: ingest=`964ms`, query=`108ms`, export=`135ms`, replay=`74ms`,
  - multi-run: query `p95/p99=61/61ms`, export `84/84ms`, replay `38/38ms`.
- README синхронизирован с отдельной release-gate командой запуска.
- Фактическое покрытие: `116` unit, `55` stdio E2E, `37` integration (+ `2` ignored stress).
- `P3` 100%, `P8` 100%, `P10` 99%, `P12` 84%, `P11` 100%.
- `cargo test --test runtime_stress -- --ignored --nocapture`: PASS.

### 2026-02-25 (update-88)
- `P12` усилен CI gate для stress SLA:
  - добавлен workflow `.github/workflows/cabal-mcp-runtime-stress-gate.yml`,
  - workflow запускается на `push/pull_request` для `cabal-mcp-runtime/**` и `spec/docs/CABAL_STRESS_SLA.md`,
  - release-gate шаг вызывает `powershell -ExecutionPolicy Bypass -File .\cabal-mcp-runtime\scripts\check-stress-sla.ps1`.
- `Next-3` обновлён: дальнейший шаг по `P12` смещён на закрепление workflow как required status check.
- Фактическое покрытие локального кода неизменно: `116` unit, `55` stdio E2E, `37` integration (+ `2` ignored stress).
- `P3` 100%, `P8` 100%, `P10` 99%, `P12` 88%, `P11` 100%.

### 2026-02-25 (update-89)
- `P10` закрыт по пункту IDE adapters/examples:
  - добавлены шаблоны `spec/examples/ide/vscode.mcp.jsonc` и `spec/examples/ide/jetbrains.mcp.jsonc`,
  - добавлен `spec/examples/ide/README.md` с единым onboarding для MCP stdio подключения,
  - `cabal-mcp-runtime/README.md` дополнен ссылками на эти шаблоны.
- Остаточные задачи `P10` остаются в real IDE E2E (handshake/enforcement/routing parity).
- `P3` 100%, `P8` 100%, `P10` 99%, `P12` 88%, `P11` 100%.

### 2026-02-25 (update-90)
- Проведён повторный контрольный прогон `scripts/check-stress-sla.ps1` после документационных и CI-правок.
- Актуальные значения stress-run:
  - single-run: ingest=`1012ms`, query=`109ms`, export=`133ms`, replay=`76ms`,
  - multi-run: query `p95/p99=59/59ms`, export `82/82ms`, replay `38/38ms`.
- `cabal` release-gate status: PASS.

### 2026-02-25 (update-91)
- `P6/P10/P11` усилены CONSULT guard-policy:
  - добавлены runtime методы `get/set_consult_guard_policy`,
  - добавлен enforce path в `route_consult`: при `YOLO + require_cross_rules_ack=true` отсутствующие `required_evidence_ids` блокируют routing с `POLICY_DENY`,
  - deny-path логируется событием `consult.blocked_missing_evidence` для расследований.
- MCP tools и docs синхронизированы:
  - `src/main.rs`: зарегистрированы `cabal.get_consult_guard_policy` и `cabal.set_consult_guard_policy`,
  - `cabal-mcp-runtime/README.md`: описана guard policy в разделе Consult Router.
- Добавлены тесты:
  - runtime unit + runtime_api integration + stdio E2E сценарии guard deny/pass path.
- Фактическое покрытие: `118` unit, `56` stdio E2E, `38` integration (+ `2` ignored stress).
- `cargo test -q`: PASS.

### 2026-02-25 (update-92)
- `P12` расширен automation для GitHub required status checks:
  - добавлен `scripts/set-required-stress-gate.ps1`,
  - script формирует/применяет branch protection с required check `stress-sla-gate`,
  - подтверждён `-DryRun` path (валидный payload без внешнего вызова).
- README синхронизирован с командой настройки branch protection.
- Подтверждён регрессионный статус runtime после добавления automation:
  - `cargo test -q`: PASS (`118` unit, `56` stdio E2E, `38` integration, `2` ignored stress).
- `P3` 100%, `P8` 100%, `P10` 99%, `P12` 90%, `P11` 100%.

### 2026-02-25 (update-93)
- `P6/P10/P11` расширены cross-rules MCP orchestration tools:
  - добавлены `cabal.get_cross_rules_status` и `cabal.ack_cross_rules` в runtime и tools catalog,
  - `ack_cross_rules` выполняет атомарный ack для agent/subagent evidence и может сразу включать consult-guard,
  - README синхронизирован с новым fast-path для cross-rules ack/status.
- Добавлены тесты:
  - runtime unit для `get_cross_rules_status` и `ack_cross_rules`,
  - runtime integration + stdio E2E сценарии `ack_cross_rules`/`get_cross_rules_status` + `route_consult` pass-path.
- Подтверждён регрессионный статус:
  - `cargo test -q`: PASS (`121` unit, `57` stdio E2E, `39` integration, `2` ignored stress),
  - `scripts/check-stress-sla.ps1`: PASS (single-run ingest=`981ms`, query=`109ms`, export=`132ms`, replay=`73ms`; multi-run query `p95/p99=60/60ms`, export `82/82ms`, replay `35/35ms`).
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 90%, `P11` 100%.

### 2026-02-25 (update-94)
- `P10/P12` усилены IDE contract gate automation:
  - добавлен `scripts/check-ide-contract-gate.ps1` с целевыми тестами MCP stdio (`vscode/jetbrains profile`, `strict_artifacts`, `consult audit contract`, `ack_cross_rules`) и integration path,
  - добавлен workflow `.github/workflows/cabal-mcp-runtime-ide-contract-gate.yml` для автоматического запуска на `push/pull_request`.
- Подтверждён локальный проход IDE contract gate:
  - `powershell -ExecutionPolicy Bypass -File .\scripts\check-ide-contract-gate.ps1 -WithIntegration`: PASS.
- Подтверждён общий регрессионный статус:
  - `cargo test -q`: PASS (`121` unit, `57` stdio E2E, `39` integration, `2` ignored stress).
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 91%, `P11` 100%.

### 2026-02-25 (update-95)
- `P12` branch-protection automation расширен мульти-check режимом:
  - `scripts/set-required-stress-gate.ps1` поддерживает `-AdditionalStatusChecks`,
  - скрипт теперь может одним вызовом закреплять `stress-sla-gate` + `ide-contract-gate`.
- Подтверждён dry-run контракт:
  - команда с `-AdditionalStatusChecks ide-contract-gate -DryRun` возвращает ожидаемый payload с двумя required contexts.
- README синхронизирован с новой командой branch protection.
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 92%, `P11` 100%.

### 2026-02-25 (update-96)
- `P10` дополнен рабочим runbook для ручной валидации в реальных IDE MCP-клиентах:
  - добавлен `spec/docs/CABAL_IDE_E2E_CHECKLIST.md` (подготовка, matrix, шаги, критерии PASS),
  - `README` runtime дополнен ссылкой на checklist.
- Это снижает риск неоднозначной ручной проверки перед финальным закрытием `P10`.
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 92%, `P11` 100%.

### 2026-02-25 (update-97)
- Подтверждена portability IDE contract gate скрипта:
  - `check-ide-contract-gate.ps1` успешно запущен из корня репозитория (вне `cabal-mcp-runtime`) за счёт `Push-Location` в скрипте.
- Обновлены фактические stress-метрики после последнего прогона:
  - single-run: ingest=`998ms`, query=`102ms`, export=`126ms`, replay=`68ms`,
  - multi-run: query `p95/p99=55/55ms`, export `75/75ms`, replay `34/34ms`.
- Подтверждён статус локальных gate-проверок:
  - `check-ide-contract-gate.ps1`: PASS,
  - `check-stress-sla.ps1`: PASS.
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 93%, `P11` 100%.

### 2026-02-25 (update-98)
- `P10` усилен формализованной приёмкой manual IDE E2E:
  - добавлен schema `spec/contracts/IDE_E2E_REPORT.schema.json`,
  - добавлен валидатор `scripts/validate-ide-e2e-report.ps1`,
  - `CABAL_IDE_E2E_CHECKLIST.md` дополнен командой валидации отчёта.
- Проведён smoke-run валидатора на синтетическом PASS-отчёте: PASS.
- Подтверждён регрессионный статус кода после изменений документации/скриптов:
  - `cargo test -q`: PASS (`121` unit, `57` stdio E2E, `39` integration, `2` ignored stress).
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 93%, `P11` 100%.

### 2026-02-25 (update-99)
- `P12` усилен unified release-gate automation:
  - добавлен `scripts/check-release-gates.ps1` (`stress SLA` + `IDE contract` в одном запуске),
  - добавлен workflow `.github/workflows/cabal-mcp-runtime-release-gate.yml` (`workflow_dispatch`).
- Подтверждён локальный проход объединённого gate:
  - `powershell -ExecutionPolicy Bypass -File .\scripts\check-release-gates.ps1 -WithIntegration`: PASS.
- Обновлены фактические stress-метрики (из объединённого прогона):
  - single-run: ingest=`938ms`, query=`100ms`, export=`127ms`, replay=`70ms`,
  - multi-run: query `p95/p99=56/56ms`, export `74/74ms`, replay `34/34ms`.
- Подтверждён общий регрессионный статус после изменений:
  - `cargo test -q`: PASS (`121` unit, `57` stdio E2E, `39` integration, `2` ignored stress).
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 94%, `P11` 100%.

### 2026-02-25 (update-100)
- `P12` доработан branch-protection helper для multi-check параметров:
  - `scripts/set-required-stress-gate.ps1` теперь корректно разбирает `-AdditionalStatusChecks` как список, включая comma-separated формат.
- Подтверждён dry-run контракт для трёх required checks:
  - `ide-contract-gate`,
  - `release-gate`,
  - `stress-sla-gate`.
- README команда branch protection оставлена совместимой с comma-separated форматом.
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 95%, `P11` 100%.

### 2026-02-25 (update-101)
- `P10/P11` усилены transport-level invalid-request проверкой для `ack_cross_rules`:
  - добавлен stdio E2E тест `mcp_stdio_ack_cross_rules_empty_path_is_invalid_request`,
  - зафиксирован контракт: пустой `agent_ack_path` возвращает JSON-RPC error с `cabal_code=INVALID_REQUEST`.
- Это закрывает недостающий transport negative-path для `ack_cross_rules` после runtime/error unit coverage.
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 95%, `P11` 100%.

### 2026-02-25 (update-102)
- `P10/P12` расширены schema-gate и release artifacts:
  - добавлен fixture `spec/contracts/ide_e2e_report.pass.json` для стабильного schema smoke,
  - добавлен CI workflow `.github/workflows/cabal-mcp-runtime-ide-e2e-report-schema.yml`,
  - `scripts/check-release-gates.ps1` теперь включает шаг IDE E2E schema validation и пишет machine-readable summary (`.cabal_runtime/release_gate_summary.json`),
- `README` и `CABAL_IDE_E2E_CHECKLIST.md` синхронизированы с новым fixture/gate контуром.
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 96%, `P11` 100%.

### 2026-02-25 (update-103)
- Подтверждён локальный проход нового unified release-gate контура:
  - `powershell -ExecutionPolicy Bypass -File .\cabal-mcp-runtime\scripts\check-release-gates.ps1 -WithIntegration`: PASS,
  - сформирован summary `cabal-mcp-runtime/.cabal_runtime/release_gate_summary.json` со step-level статусами (`stress_sla_gate`, `ide_contract_gate`, `ide_e2e_schema_gate`).
- Зафиксированы актуальные stress-метрики из release-gate прогона:
  - single-run: ingest=`1382ms`, query=`160ms`, export=`197ms`, replay=`90ms`,
  - multi-run: query `p95/p99=83/83ms`, export `118/118ms`, replay `55/55ms`.
- Подтверждён branch-protection payload для четырёх required contexts:
  - `ide-contract-gate`,
  - `ide-e2e-report-schema-gate`,
  - `release-gate`,
  - `stress-sla-gate`.
- Подтверждён регрессионный статус:
- `cargo test -q`: PASS (`121` unit, `58` stdio E2E, `39` integration, `2` ignored stress).
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 96%, `P11` 100%.

### 2026-02-25 (update-104)
- `P12` усилен формальным контрактом release summary:
  - добавлены `spec/contracts/RELEASE_GATE_SUMMARY.schema.json` и fixture `spec/contracts/release_gate_summary.pass.json`,
  - добавлен валидатор `cabal-mcp-runtime/scripts/validate-release-gate-summary.ps1`.
- Unified release-gate расширен:
  - `scripts/check-release-gates.ps1` поддерживает `-IdeE2EReportPath` и `-RequireRealIdeReport`,
  - summary теперь фиксирует `ide_e2e_report_source`/`ide_e2e_report_path`.
- CI и артефакты:
  - `.github/workflows/cabal-mcp-runtime-release-gate.yml` валидирует summary и публикует artifact `cabal-release-gate-summary`,
  - добавлен schema-smoke workflow `.github/workflows/cabal-mcp-runtime-release-summary-schema.yml`.
- `README` синхронизирован с новыми командами (`RequireRealIdeReport`, summary validator, required checks).
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 97%, `P11` 100%.

### 2026-02-25 (update-105)
- Подтверждён локальный проход новых summary-контрактов:
  - `validate-release-gate-summary.ps1` на fixture `spec/contracts/release_gate_summary.pass.json`: PASS,
  - `check-release-gates.ps1 -WithIntegration` (fixture-source): PASS + summary валиден,
  - `check-release-gates.ps1 -WithIntegration -IdeE2EReportPath .\spec\contracts\ide_e2e_report.pass.json -RequireRealIdeReport` (user-source): PASS + summary валиден.
- Зафиксированы актуальные stress-метрики из последних прогонов release-gate:
  - fixture-source run: ingest=`1380ms`, query=`152ms`, export=`196ms`, replay=`109ms`; multi-run query `p95/p99=85/85ms`, export `119/119ms`, replay `53/53ms`,
  - user-source run: ingest=`1376ms`, query=`155ms`, export=`189ms`, replay=`106ms`; multi-run query `p95/p99=88/88ms`, export `124/124ms`, replay `54/54ms`.
- Подтверждён dry-run payload branch protection для 5 required contexts:
  - `ide-contract-gate`,
  - `ide-e2e-report-schema-gate`,
  - `release-summary-schema-gate`,
  - `release-gate`,
  - `stress-sla-gate`.
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 97%, `P11` 100%.

### 2026-02-25 (update-106)
- `P12` усилен для branch protection automation:
  - `scripts/set-required-stress-gate.ps1` поддерживает `-UseCabalRecommendedChecks`,
  - флаг автоматически добавляет рекомендованный набор контекстов (`ide-contract-gate`, `ide-e2e-report-schema-gate`, `release-summary-schema-gate`, `release-gate`) поверх базового `StatusCheck`.
- `README` дополнен сокращённой командой branch protection с `-UseCabalRecommendedChecks`.
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 98%, `P11` 100%.

### 2026-02-25 (update-107)
- `P12` release workflow сделан более пригодным для реального IDE E2E:
  - `.github/workflows/cabal-mcp-runtime-release-gate.yml` получил `workflow_dispatch` inputs:
    - `ide_e2e_report_path`,
    - `require_real_ide_report`,
  - запуск `check-release-gates.ps1` в workflow теперь параметризован этими input и совместим с режимом реального отчёта.
- `README` синхронизирован с input-параметрами release workflow.
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 98%, `P11` 100%.

### 2026-02-25 (update-108)
- `P6/P10/P12` усилены внутри IDE contract gate automation:
  - `scripts/check-ide-contract-gate.ps1` дополнен CONSULT-routing сценариями:
    - stdio: `mcp_stdio_route_consult_role_mismatch_fallback_and_escalation`,
    - stdio: `mcp_stdio_route_consult_adaptive_exploration_selects_undertrained_executor`,
    - integration: `integration_route_consult_uses_policy_driven_matrix`,
    - integration: `integration_route_consult_adaptive_exploration_uses_undertrained_executor`.
- Подтверждён повторный проход unified release-gate после расширения:
  - `check-release-gates.ps1 -WithIntegration`: PASS,
  - `validate-release-gate-summary.ps1` на generated summary: PASS.
- Зафиксированы актуальные stress-метрики из последнего release-gate прогона:
  - single-run: ingest=`1363ms`, query=`160ms`, export=`198ms`, replay=`110ms`,
  - multi-run: query `p95/p99=86/86ms`, export `122/122ms`, replay `50/50ms`.
- Подтверждён общий регрессионный статус:
- `cargo test -q`: PASS (`121` unit, `58` stdio E2E, `39` integration, `2` ignored stress).
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 98%, `P11` 100%.

### 2026-02-25 (update-109)
- `P12` усилен контролем свежести реального IDE E2E отчёта:
  - `validate-ide-e2e-report.ps1` поддерживает `-MaxReportAgeHours`,
  - `check-release-gates.ps1` при `-RequireRealIdeReport` применяет freshness-check с параметром `-RealIdeReportMaxAgeHours` (default `72`),
  - summary расширен полями `require_real_ide_report` и `real_ide_report_max_age_hours`.
- Контракт release summary синхронизирован:
  - `RELEASE_GATE_SUMMARY.schema.json` и `release_gate_summary.pass.json` обновлены под новые поля,
  - `validate-release-gate-summary.ps1` проверяет связку `require_real_ide_report=true` -> `ide_e2e_report_source=user` + positive `real_ide_report_max_age_hours`.
- CI release workflow синхронизирован:
  - `.github/workflows/cabal-mcp-runtime-release-gate.yml` получил input `real_ide_report_max_age_hours` и передаёт его в runtime gate.
- Документация синхронизирована:
  - `README` (команды strict-mode + workflow inputs),
  - `CABAL_IDE_E2E_CHECKLIST.md` (пример валидации свежести отчёта).
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 99%, `P11` 100%.

### 2026-02-25 (update-110)
- Исправлена совместимость freshness-проверки `timestamp_utc` в PowerShell:
  - `validate-ide-e2e-report.ps1` переведён с `TryParse` на стабильный `DateTimeOffset::Parse(...)` path.
- Подтверждён проход новых strict-сценариев:
  - `validate-ide-e2e-report.ps1 -MaxReportAgeHours 72`: PASS,
  - `check-release-gates.ps1 -WithIntegration -IdeE2EReportPath ... -RequireRealIdeReport -RealIdeReportMaxAgeHours 72`: PASS,
  - `validate-release-gate-summary.ps1` на generated `release_gate_summary.real.json`: PASS.
- Подтверждён повторный проход стандартного unified release-gate после фикса:
  - `check-release-gates.ps1 -WithIntegration`: PASS,
  - `validate-release-gate-summary.ps1` на `release_gate_summary.json`: PASS.
- Зафиксированы актуальные stress-метрики из последних прогонов:
  - strict user-source run: ingest=`977ms`, query=`101ms`, export=`121ms`, replay=`72ms`; multi-run query `p95/p99=54/54ms`, export `74/74ms`, replay `34/34ms`,
  - fixture-source run: ingest=`1004ms`, query=`100ms`, export=`124ms`, replay=`69ms`; multi-run query `p95/p99=57/57ms`, export `76/76ms`, replay `34/34ms`.
- Подтверждён общий регрессионный статус:
- `cargo test -q`: PASS (`121` unit, `58` stdio E2E, `39` integration, `2` ignored stress).
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 99%, `P11` 100%.

### 2026-02-25 (update-111)
- Для операционного удобства real IDE E2E добавлен генератор шаблона отчёта:
  - `cabal-mcp-runtime/scripts/new-ide-e2e-report.ps1`.
- Документация синхронизирована:
  - `README` и `CABAL_IDE_E2E_CHECKLIST.md` дополнены командой генерации шаблона,
- явно зафиксировано, что шаблон стартует с `IDE-P1..IDE-P5=false` и должен быть заполнен результатами фактической проверки.
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 99%, `P11` 100%.

### 2026-02-25 (update-112)
- `P12` дополнен post-apply верификацией branch protection:
  - добавлен `cabal-mcp-runtime/scripts/verify-required-status-checks.ps1`,
  - скрипт проверяет, что требуемые status checks действительно присутствуют в branch protection (через GitHub API или локальный JSON snapshot).
- `README` синхронизирован командой верификации после `set-required-stress-gate`.
- Подтверждён офлайн-smoke сценарий:
  - `verify-required-status-checks.ps1 -ProtectionJsonPath ... -UseCabalRecommendedChecks`: PASS.
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 99%, `P11` 100%.

### 2026-02-25 (update-113)
- Добавлен операционный runbook применения hardening на GitHub:
  - `spec/docs/CABAL_GITHUB_HARDENING_RUNBOOK.md`,
  - включает preflight, apply required checks, verification и release-gate запуск с real IDE report.
- `README` дополнен ссылкой на runbook.
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 99%, `P11` 100%.

### 2026-02-25 (update-114)
- Текущая стадия явно обозначена как checkpoint:
  - добавлен `spec/docs/CABAL_WIP_CHECKPOINT.md` (исторический статус на момент update-114: `in_progress (not final)`).
- Добавлен единый финальный orchestrator-скрипт:
  - `cabal-mcp-runtime/scripts/check-final-readiness.ps1`,
  - выполняет strict release gate + validate summary + verify required status checks в одном запуске.
- Подтверждён офлайн-end-to-end smoke:
  - `check-final-readiness.ps1 -IdeE2EReportPath ... -ProtectionJsonPath ...`: PASS.
- `README` и `CABAL_GITHUB_HARDENING_RUNBOOK.md` синхронизированы с командой `check-final-readiness`.
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 99%, `P11` 100%.

### 2026-02-25 (update-115)
- `P12` доведён до orchestrated close-path:
  - добавлен `apply-and-verify-branch-protection.ps1` (apply+verify required checks в одном запуске),
  - добавлен `validate-real-ide-e2e-artifacts.ps1` (report + vscode/jetbrains logs),
  - `check-final-readiness.ps1` расширен опциональным шагом реальных IDE-логов (`VsCodeLogPath` + `JetBrainsLogPath`).
- Подтверждён полный офлайн smoke `4-step` финального readiness-контура:
  - `check-final-readiness.ps1 -IdeE2EReportPath ... -VsCodeLogPath ... -JetBrainsLogPath ... -ProtectionJsonPath ...`: PASS.
- Документация синхронизирована:
  - `README`, `CABAL_GITHUB_HARDENING_RUNBOOK.md`, `CABAL_WIP_CHECKPOINT.md`.
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 99%, `P11` 100%.

### 2026-02-25 (update-116)
- Устранён операционный блокер `gh` CLI:
  - `set-required-stress-gate.ps1` и `verify-required-status-checks.ps1` поддерживают fallback через GitHub REST API по `GITHUB_TOKEN`/`GH_TOKEN`.
- `README` и `CABAL_GITHUB_HARDENING_RUNBOOK.md` синхронизированы с требованиями доступа (gh CLI или token env).
- Кодовая реализация и checkpoint-push опубликованы в `origin/main`:
  - commit `fc1afb3` — основной WIP checkpoint (runtime + CI + contracts + docs),
  - commit `64c71c6` — token-based fallback для branch-protection automation.
- Текущий внешний блокер финализации не кодовый: для применения branch protection нужен `gh` или `GITHUB_TOKEN`/`GH_TOKEN` в окружении.
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 99%, `P11` 100%.

### 2026-02-25 (update-117)
- Финализирован machine-readable контракт итоговой готовности:
  - `check-final-readiness.ps1` теперь всегда пишет `final_readiness_result.json` со step-level статусами и метаданными режима проверки,
  - добавлены `spec/contracts/FINAL_READINESS_SUMMARY.schema.json` и fixture `spec/contracts/final_readiness_summary.pass.json`,
  - добавлен валидатор `cabal-mcp-runtime/scripts/validate-final-readiness-summary.ps1`,
  - добавлен schema-smoke workflow `.github/workflows/cabal-mcp-runtime-final-readiness-summary-schema.yml`.
- `README` и `CABAL_GITHUB_HARDENING_RUNBOOK.md` синхронизированы с командами валидации final readiness summary.
- `P3` 100%, `P6` 99%, `P8` 100%, `P10` 99%, `P12` 99%, `P11` 100%.

### 2026-02-24 (update-39)
- README runtime синхронизирован с текущим состоянием после `P9`:
  - добавлен список core-модулей,
  - уточнено покрытие stdio E2E anti-bypass сценария.
- Поведение runtime не изменено, документация приведена в соответствие с кодом.
- `cargo test`: PASS.

### 2026-02-25 (update-118)
- Добавлены недостающие программные слои deterministic/adaptive orchestration:
  - `TaskClassifier`: `cabal.classify_task`,
  - `BudgetController`: `cabal.get_budget_policy`, `cabal.set_budget_policy`, `cabal.plan_task_execution`,
  - `PatchGate`: `cabal.get_patch_gate_policy`, `cabal.set_patch_gate_policy`, `cabal.evaluate_patch_gate`.
- `route_consult` расширен `task_profile` (`task_type/risk/confidence/keywords/budget`) в response и `consult.routed` audit payload.
- Добавлены unit/E2E тесты для новых контуров (task planning + patch gate), регрессионный прогон:
  - `cargo test -q`: PASS (`126` unit, `59` stdio E2E, `39` integration, `2` ignored stress).
- Подтверждён финальный локальный readiness-контур:
  - `check-release-gates.ps1 -WithIntegration`: PASS,
  - `check-final-readiness.ps1 -IdeE2EReportPath spec/contracts/ide_e2e_report.pass.json -ProtectionJsonPath ...`: PASS,
  - `validate-final-readiness-summary.ps1`: PASS.
- Статус этапов после update-118:
  - `P1..P13`: `done (100%)`,
  - `P14`: `in_progress (85%)`, остался только live rollout в целевой GitHub ветке.

## 11) Next-3 (актуальные)
1. Провести пользовательский real IDE E2E в VS Code/JetBrains по `spec/examples/ide/*` и сохранить отчёт `spec/docs/ide_e2e_report.json` + логи.
2. Выполнить `check-final-readiness.ps1` в `github`-режиме (без snapshot) на целевом репозитории с `GITHUB_TOKEN`/`GH_TOKEN`.
3. Закрыть `P14`: применить/подтвердить branch protection на живой ветке `main` и зафиксировать rollout-результат отдельным релизным отчётом.

## 12) Блокеры и риски
- Кодовых блокеров реализации нет.
- Оставшиеся риски относятся к live rollout:
  - различия MCP transport-поведения между IDE клиентами;
  - риск внешнего обхода до фактического применения branch protection в GitHub;
  - риск деградации latency на целевом CI окружении.

Митигации:
- контрактные integration tests на каждом transport-профиле;
- versioned policy schema + migration tests;
- deny-by-default в IDE-конфиге + anti-bypass E2E.

## 13) Definition of Done (полная готовность)
Система считается полностью реализованной, когда:
- любая модель/агент в IDE работает только через Cabal MCP runtime;
- фазы и переходы строго контролируются Gate Engine;
- policy/consult/audit полностью воспроизводимы;
- попытки bypass блокируются и фиксируются с кодом отказа;
- подключение новой IDE сводится к MCP endpoint без переноса логики в prompt-файлы.

Текущий статус по этому критерию:
- программная реализация runtime: `DONE`;
- production rollout на целевом GitHub репозитории: `IN_PROGRESS` (операционный шаг, не кодовый).
