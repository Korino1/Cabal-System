---
id: phase_schema
updated: 2026-02-01
owner: techlead
---
# Каноническая структура фазы (машиночитаемая)

## Назначение
Единый формат описания фаз, чтобы агентам и инструментам было одинаково понятно, что считать входами/выходами и как подтверждать завершение.

## Формат (обязательные секции)
1) Purpose  
2) Scope (если применяется)  
3) Inputs  
4) Outputs/Artifacts  
5) Entry Criteria  
6) Exit Criteria  
7) Evidence  
8) Dependency Matrix

## Правила
- Заголовки обязательны и идут в фиксированном порядке.
- В Evidence указываются пути к артефактам и регистрам статуса (INDEX/DIGEST/GLOBAL_INDEX).
- Если раздел не заполнен — ставь `- TODO` или явный «нет».
- Допускается секция **Status**, внутри которой размещаются Entry Criteria и Exit Criteria.

## Минимальный оверхед (обязательно)
- **Dependency Matrix** ведётся кратко: базовые строки INPUTS → CORE_OUTPUT → DIGEST.
- Новые строки добавляются **только** когда блоки/методы/функции реально появились.
- Идентификаторы короткие: `BLK:<id>`, `MTD:<id>`, `FN:<id>`.
- Не перечисляй все функции без необходимости; достаточно уровня блок/метод, если это покрывает зависимость.
- **Evidence**: минимальный набор — `LOGIC_PROTOCOL.md` (раздел фазы), `PHASES/<Phase>/DIGEST.md`, `PHASES/<Phase>/INDEX.md`, `.memory/GLOBAL_INDEX.md`.
- Дополнительные Evidence (WORKLOG/STATE_HISTORY) добавляй только при споре, проверке или исправлениях.

## Шаблон (Markdown)
```markdown
# Phase <ID> — <Title>

## Purpose
- ...

## Scope
- In-scope: ...
- Out-of-scope: ...

## Inputs
- ...

## Outputs/Artifacts
- ...

## Status
- State: pending|in_progress|done
- Entry Criteria:
  - ...
- Exit Criteria:
  - ...

## Evidence
- ...

## Dependency Matrix
| ID | DependsOn | Status | Notes |
| -- | --------- | ------ | ----- |
```

## Опционально: структурированный вид (YAML)
```yaml
phase: <ID>
title: <Title>
purpose:
  - ...
scope:
  in_scope:
    - ...
  out_of_scope:
    - ...
inputs:
  - ...
outputs:
  - ...
entry_criteria:
  - ...
exit_criteria:
  - ...
evidence:
  - ...
dependencies:
  - id: ...
    depends_on: ...
    status: ...
    notes: ...
```
