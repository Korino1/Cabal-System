---
id: phase_index
phase: GA-1
updated: 2026-02-08
owner: orchestrator
---
# Phase GA-1 — Схема концепта

## Purpose
- Построить логическую схему концепта: связи, блоки, последовательность логики.

## Scope
- In-scope:
  - Анализ концепта и выделение логических блоков.
  - Определение связей между блоками.
  - Фиксация последовательности применения логики.
- Out-of-scope:
  - Декомпозиция блоков на методы (GA-2).
  - Детализация функций (GA-4/GA-5).

## Inputs
- `spec/docs/CONCEPT_MASTER.md`.
- `spec/docs/CONCEPT_MATH_PROOF.md`.
- Итог фазы C-0: `PHASES/C-0/DIGEST.md`.

## Outputs/Artifacts
- Раздел GA-1 в `.memory/LOGIC_PROTOCOL.md`.
- `PHASES/GA-1/DIGEST.md`.

## Status
- State: pending
- Entry Criteria:
  - C-0 завершен; `spec/docs/CONCEPT_MASTER.md` и `spec/docs/CONCEPT_MATH_PROOF.md` заполнены.
  - Активная фаза = GA-1 в `.memory/GLOBAL_INDEX.md`.
- Exit Criteria:
  - В `.memory/LOGIC_PROTOCOL.md` заполнен раздел GA-1.
  - В `.memory/PHASES/GA-1/DIGEST.md` кратко зафиксирован итог.

## Evidence
- .memory/LOGIC_PROTOCOL.md (раздел GA-1).
- PHASES/GA-1/DIGEST.md.
- PHASES/GA-1/INDEX.md.
- .memory/GLOBAL_INDEX.md.

## Dependency Matrix
| ID | DependsOn | Status | Notes |
| -- | --------- | ------ | ----- |
| INPUTS | spec/docs/CONCEPT_MASTER.md; spec/docs/CONCEPT_MATH_PROOF.md; PHASES/C-0/DIGEST.md | required | входы |
| GA1_SCHEMA | INPUTS | pending | LOGIC_PROTOCOL.md (GA-1) |
| DIGEST | GA1_SCHEMA | pending | PHASES/GA-1/DIGEST.md |

## Active Links
- Tasks: .memory/PHASES/GA-1/TASKS.md
- Worklog: .memory/PHASES/GA-1/WORKLOG.md
- State History: .memory/PHASES/GA-1/STATE_HISTORY.md
- Digest: .memory/PHASES/GA-1/DIGEST.md

