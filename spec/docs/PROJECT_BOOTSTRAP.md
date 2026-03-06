---
id: project_bootstrap
updated: 2026-02-01
owner: techlead
---
# Project Bootstrap

## 1) Цель
Стандартизировать старт нового проекта и гарантировать восстановление прогресса после обрыва связи.

## 2) Стартовый набор файлов
- `AGENTS.md`
- `.memory/MISSION.md`
- `.memory/CONTEXT.md`
- `.memory/TASKS.md`
- `.memory/PROGRESS.md`
- `.memory/ASKS.md`
- `.memory/DECISIONS.md`
- `.memory/INDEX.yaml`
- `.memory/TRACKING.md`
- `.memory/STATE.md`
- `.memory/GLOBAL_INDEX.md`
- `.memory/PHASES/<PHASE>/*` (INDEX/WORKLOG/STATE_HISTORY/TASKS/DIGEST)
- `.memory/DEFECTS.md`
- `spec/contracts/VERSION.json`
- `spec/adr/ADR-0001.md` (шаблон)
- `spec/docs/PROJECT_BOOTSTRAP.md`
- `spec/docs/PHASE_SCHEMA.md`
- `spec/docs/PHASE_GATE.md`

## 3) Набор агентов

## 4) Процедура запуска
1) Скопировать набор файлов и проверить UTF-8 без BOM.
2) Заполнить `MISSION.md` и `CONTEXT.md`.
3) Инициализировать `TASKS.md` (EP/FEAT/US/T + `US <BASE>.GOV`).
4) Инициализировать структуру фаз в `.memory/PHASES/` и `GLOBAL_INDEX.md` (активная фаза).
5) Создать первый `STATE.md` (snapshot) и записать начальный шаг в `PHASES/<Active>/WORKLOG.md`.
6) Обновить `.memory/INDEX.yaml` (даты и список артефактов).
7) Проверить `TRACKING.md` и наличие `DEFECTS.md`.

## 5) Минимальный контроль качества
- См. регламент `.memory/TRACKING.md` (checkpoint, валидаторы, синхронизации).

## 6) Восстановление после обрыва
См. раздел «Восстановление после обрыва» в `.memory/TRACKING.md`.


