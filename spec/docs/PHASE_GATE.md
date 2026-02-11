---
id: phase_gate
updated: 2026-02-11
owner: techlead
---
# Гейт‑чеклист входа/выхода фаз

## Entry (старт фазы)
- Active Phase в `.memory/GLOBAL_INDEX.md` соответствует целевой фазе.
- Entry Criteria заполнены в `PHASES/<Phase>/INDEX.md`.
- Все входные артефакты доступны и актуальны.

## Exit (закрытие фазы)
- Exit Criteria заполнены в `PHASES/<Phase>/INDEX.md`.
- `PHASES/<Phase>/DIGEST.md` заполнен.
- Evidence в `PHASES/<Phase>/INDEX.md` указывает на фактические артефакты.
- Статус фазы обновлён в `.memory/GLOBAL_INDEX.md`.
- Для C-0: выбранный Пользователем вариант и обновлённые параметры синхронно внесены в `spec/docs/CONCEPT_MATH_PROOF.md` и `spec/docs/CONCEPT_MASTER.md`.
- Для C-0: в `PHASES/C-0/TASKS.md` закрыты C-0.1..C-0.6.
- Для C-0: в `CONCEPT_MASTER.md` и `CONCEPT_MATH_PROOF.md` отсутствуют `TODO` в содержательном ядре.
- Для C-0: в `PHASES/C-0/WORKLOG.md` есть запись о синхронизации канона (`CONCEPT_MATH_PROOF` + `CONCEPT_MASTER`).
- Для C-0: если после последней C-0.6 появился новый выбор/новые параметры от Пользователя, фаза не закрывается до повторного выполнения C-0.2..C-0.6.

Примечание: Evidence достаточно минимального набора, если нет споров/проверок.

## Переключение (финальная проверка)
- Выполнен `scripts/validate_phase_exit.ps1 -PhaseId <Phase>`.
- Обновлён `STATE.md` и записан шаг в `PHASES/<Next>/WORKLOG.md`.
