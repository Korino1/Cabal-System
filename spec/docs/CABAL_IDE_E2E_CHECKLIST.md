# CABAL IDE E2E Checklist (VS Code / JetBrains)

Цель: подтвердить реальное поведение `cabal-mcp-runtime` в IDE MCP-клиентах, а не только в тестах stdio.

## 1) Подготовка
1. Сборка runtime:
```powershell
cd .\cabal-mcp-runtime
cargo build --release
```
2. Использовать шаблоны:
- `spec/examples/ide/vscode.mcp.jsonc`
- `spec/examples/ide/jetbrains.mcp.jsonc`
3. Проверить, что `command` указывает на существующий `cabal-mcp-runtime.exe`.

## 2) Матрица проверок
- `IDE-P1`: `initialize` корректно определяет профиль IDE (`vscode`/`jetbrains`).
- `IDE-P2`: policy enforcement блокирует неразрешённый профиль (`POLICY_DENY`).
- `IDE-P3`: при `strict_artifacts=true` gate-check отражает строгий режим.
- `IDE-P4`: `route_consult` возвращает обязательный routing contract (`route/actor/policy_revision/ide_profile/ide_client_name`).
- `IDE-P5`: `ack_cross_rules` + `get_cross_rules_status` + `route_consult` работают как связка (deny до ack, pass после ack при включённом guard).

## 3) Шаги проверки
1. `initialize`:
  - открыть MCP-сессию из IDE;
  - вызвать `cabal.get_ide_profile_policy`;
  - ожидаемо: `active_profile` соответствует IDE.
2. Enforcement:
  - включить `cabal.set_ide_profile_policy` с `enforce_ide_profile=true`;
  - проверить deny/allow по профилям.
3. Gate strict mode:
  - `cabal.set_gate_policy {"strict_artifacts": true}`;
  - выполнить `cabal.gate_check {"kind":"entry","phase":"GA-1"}`.
4. Consult contract:
  - `cabal.set_consult_mode {"mode":"YOLO"}`;
  - вызвать `cabal.route_consult`;
  - проверить обязательные поля контракта.
5. Cross-rules:
  - включить guard: `cabal.set_consult_guard_policy {"require_cross_rules_ack":true}`;
  - проверить deny `route_consult` до ack;
  - выполнить `cabal.ack_cross_rules`;
  - повторить `route_consult` и получить pass.

## 4) Артефакты запуска
- Сохранить raw MCP logs/trace из IDE с timestamp.
- Приложить JSON-ответы по `IDE-P1..IDE-P5`.
- Зафиксировать версию клиента IDE и версию runtime.

Рекомендуемый JSON-формат отчёта:
- schema: `spec/contracts/IDE_E2E_REPORT.schema.json`
- fixture (CI smoke): `spec/contracts/ide_e2e_report.pass.json`
- валидатор: `cabal-mcp-runtime/scripts/validate-ide-e2e-report.ps1`

Команда валидации:
```powershell
powershell -ExecutionPolicy Bypass -File .\cabal-mcp-runtime\scripts\validate-ide-e2e-report.ps1 -ReportPath .\spec\docs\ide_e2e_report.json
```

Шаблон отчёта можно сгенерировать автоматически:
```powershell
powershell -ExecutionPolicy Bypass -File .\cabal-mcp-runtime\scripts\new-ide-e2e-report.ps1 -ReportPath .\spec\docs\ide_e2e_report.json
```
Шаблон заполняется со значениями `IDE-P1..IDE-P5=false`; перед финальной валидацией проставить `true` для пройденных проверок.

Опционально можно добавить проверку свежести отчёта (например 24 часа):
```powershell
powershell -ExecutionPolicy Bypass -File .\cabal-mcp-runtime\scripts\validate-ide-e2e-report.ps1 -ReportPath .\spec\docs\ide_e2e_report.json -MaxReportAgeHours 24
```

Smoke-проверка на эталонном fixture:
```powershell
powershell -ExecutionPolicy Bypass -File .\cabal-mcp-runtime\scripts\validate-ide-e2e-report.ps1 -ReportPath .\spec\contracts\ide_e2e_report.pass.json
```

## 5) Критерий PASS
Сессия считается успешной только если все `IDE-P1..IDE-P5` выполнены для:
- VS Code-профиля;
- JetBrains-профиля.
