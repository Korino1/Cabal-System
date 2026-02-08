---
id: phase_gate
updated: 2026-02-01
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

Примечание: Evidence достаточно минимального набора, если нет споров/проверок.

## Переключение (финальная проверка)
- Выполнен `scripts/validate_phase_exit.ps1 -PhaseId <Phase>`.
- Обновлён `STATE.md` и записан шаг в `PHASES/<Next>/WORKLOG.md`.
