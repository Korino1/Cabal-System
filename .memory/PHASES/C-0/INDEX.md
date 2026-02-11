---
id: phase_index
phase: C-0
updated: 2026-02-11
owner: orchestrator
---
# Phase C-0 — Обобщённый концепт

## Purpose
- Зафиксировать сквозные правила и математическое обоснование концепта (леммы/инварианты/ограничения).
- Свести все концептуальные материалы в единый каноничный концепт (без кода).
- Owner: conceptualizer.

## Scope
- In-scope:
  - Сбор и синтез концепта (RU).
  - Дублирующая верификация концепта (EN, ZH) и синтез расхождений.
  - Критический math-critique с вариантами решения.
  - Синхронизация канона после выбора Пользователя (C-0.6).
- Out-of-scope:
  - Декомпозиция на блоки/методы/функции.

## Inputs
- Базовые контекстные файлы: `.memory/MISSION.md`, `.memory/CONTEXT.md`, `.memory/USECASES.md`.
- Первичные материалы проекта (тексты/схемы/заметки/ядра).

## Outputs/Artifacts
- `spec/docs/CONCEPT_MATH_PROOF.md`.
- `spec/docs/CONCEPT_MASTER.md`.
- `PHASES/C-0/DIGEST.md`.

## Status
- State: in_progress
- Entry Criteria:
  - Получено исходное задание/концептуальные материалы.
- Exit Criteria:
  - `spec/docs/CONCEPT_MATH_PROOF.md` заполнен (определения/леммы/ограничения/инварианты + трассировка claim→лемма/теорема).
  - `spec/docs/CONCEPT_MASTER.md` заполнен.
  - В `spec/docs/CONCEPT_MASTER.md` заполнен раздел 6 (сквозные правила: запреты/разрешения).
  - Проведены верификации EN/ZH и синтез расхождений (C-0.2..C-0.4) с фиксацией в `WORKLOG`.
  - Проведён критический math-critique (C-0.5), подготовлены варианты для Пользователя.
  - После выбора Пользователем варианта и подбора параметров Математиком обновления синхронно внесены в `spec/docs/CONCEPT_MATH_PROOF.md` и `spec/docs/CONCEPT_MASTER.md`.
  - В `PHASES/C-0/TASKS.md` закрыты C-0.1..C-0.6 для последнего актуального цикла.
  - Если после C-0.6 Пользователь меняет вариант/параметры, C-0.2..C-0.6 повторно открываются и выполняются заново.
  - Обновление канона зафиксировано в `PHASES/C-0/WORKLOG.md`.
  - DIGEST.md заполнен.

## Evidence
- spec/docs/CONCEPT_MATH_PROOF.md.
- spec/docs/CONCEPT_MASTER.md.
- PHASES/C-0/DIGEST.md.
- PHASES/C-0/INDEX.md.
- .memory/GLOBAL_INDEX.md.

## Dependency Matrix
| ID | DependsOn | Status | Notes |
| -- | --------- | ------ | ----- |
| INPUTS | .memory/MISSION.md; .memory/CONTEXT.md; .memory/USECASES.md; первичные материалы | required | входы |
| CONCEPT_MATH_PROOF | INPUTS | pending | spec/docs/CONCEPT_MATH_PROOF.md |
| CONCEPT_MASTER | INPUTS; CONCEPT_MATH_PROOF | pending | spec/docs/CONCEPT_MASTER.md |
| CONCEPT_SYNC | CONCEPT_MASTER; CONCEPT_MATH_PROOF; выбор Пользователя | pending | синхронизация обновлённого канона |
| DIGEST | CONCEPT_SYNC | pending | PHASES/C-0/DIGEST.md |

## Active Links
- Tasks: .memory/PHASES/C-0/TASKS.md
- Worklog: .memory/PHASES/C-0/WORKLOG.md
- State History: .memory/PHASES/C-0/STATE_HISTORY.md
- Digest: .memory/PHASES/C-0/DIGEST.md
