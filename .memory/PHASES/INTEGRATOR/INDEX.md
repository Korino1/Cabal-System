---
id: phase_index
phase: INTEGRATOR
updated: 2026-02-01
owner: orchestrator
---
# Phase INTEGRATOR — Связи функций

## Purpose
- Связи функций внутри/между блоками.

## Scope
- In-scope:
  - Связи функций внутри/между блоками.
- Out-of-scope:
  - Реализация.

## Inputs
- Итог фазы ARCH: `PHASES/ARCH/DIGEST.md`.
- Описания функций (GA-5).

## Outputs/Artifacts
- Раздел INTEGRATOR в `.memory/LOGIC_PROTOCOL.md`.
- `PHASES/INTEGRATOR/DIGEST.md`.

## Status
- State: pending
- Entry Criteria:
  - ARCH завершен.
- Exit Criteria:
  - Раздел INTEGRATOR заполнен; DIGEST.md заполнен.
- 
## Evidence
- .memory/LOGIC_PROTOCOL.md (раздел INTEGRATOR).
- PHASES/INTEGRATOR/DIGEST.md.
- PHASES/INTEGRATOR/INDEX.md.
- .memory/GLOBAL_INDEX.md.

## Dependency Matrix
| ID | DependsOn | Status | Notes |
| -- | --------- | ------ | ----- |
| INPUTS | PHASES/ARCH/DIGEST.md; описания функций | required | входы |
| LINKS_MAP | INPUTS | pending | LOGIC_PROTOCOL.md (INTEGRATOR) |
| DIGEST | LINKS_MAP | pending | PHASES/INTEGRATOR/DIGEST.md |

## Active Links
- Tasks: .memory/PHASES/INTEGRATOR/TASKS.md
- Worklog: .memory/PHASES/INTEGRATOR/WORKLOG.md
- State History: .memory/PHASES/INTEGRATOR/STATE_HISTORY.md
- Digest: .memory/PHASES/INTEGRATOR/DIGEST.md


