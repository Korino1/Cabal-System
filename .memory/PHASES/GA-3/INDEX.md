---
id: phase_index
phase: GA-3
updated: 2026-02-01
owner: orchestrator
---
# Phase GA-3 — Схемы блоков

## Purpose
- Схемы по каждому блоку.

## Scope
- In-scope:
  - Схемы по каждому блоку.
- Out-of-scope:
  - Декомпозиция методов в функции.

## Inputs
- Раздел GA-2 в `.memory/LOGIC_PROTOCOL.md`.
- Итог фазы GA-2: `PHASES/GA-2/DIGEST.md`.

## Outputs/Artifacts
- Раздел GA-3 в `.memory/LOGIC_PROTOCOL.md`.
- `PHASES/GA-3/DIGEST.md`.

## Status
- State: pending
- Entry Criteria:
  - GA-2 завершен; блоки и методы определены.
- Exit Criteria:
  - Раздел GA-3 заполнен по каждому блоку; DIGEST.md заполнен.
- 
## Evidence
- .memory/LOGIC_PROTOCOL.md (раздел GA-3).
- PHASES/GA-3/DIGEST.md.
- PHASES/GA-3/INDEX.md.
- .memory/GLOBAL_INDEX.md.

## Dependency Matrix
| ID | DependsOn | Status | Notes |
| -- | --------- | ------ | ----- |
| INPUTS | LOGIC_PROTOCOL.md (GA-2); PHASES/GA-2/DIGEST.md | required | входы |
| GA3_SCHEMES | INPUTS | pending | LOGIC_PROTOCOL.md (GA-3) |
| DIGEST | GA3_SCHEMES | pending | PHASES/GA-3/DIGEST.md |

## Active Links
- Tasks: .memory/PHASES/GA-3/TASKS.md
- Worklog: .memory/PHASES/GA-3/WORKLOG.md
- State History: .memory/PHASES/GA-3/STATE_HISTORY.md
- Digest: .memory/PHASES/GA-3/DIGEST.md


