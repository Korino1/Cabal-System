---
id: tasks_phase
phase: C-0
updated: 2026-02-11
---
# Tasks (phase)

- [ ] T C-0.0 — Сквозные правила (опросник + фиксация запретов/разрешений в CONCEPT_MASTER) [Owner: conceptualizer]
- [ ] T C-0.MATH — Математическое обоснование (CONCEPT_MATH_PROOF: леммы/инварианты/коридоры параметров) [Owner: mathematician]
- [ ] T C-0.1 — Сбор и синтез концепта [Owner: conceptualizer]
- [ ] T C-0.2 — Дублирующая верификация концепта на EN [Owner: conceptualizer]
- [ ] T C-0.3 — Дублирующая верификация концепта на ZH [Owner: conceptualizer]
- [ ] T C-0.4 — Синтез RU/EN/ZH/DE/FR и фиксация расхождений [Owner: conceptualizer]
- [ ] T C-0.5 — Критический math-critique и подготовка вариантов решения [Owner: mathematician]
- [ ] T C-0.6 — Синхронизация канона после выбора Пользователя (обновить CONCEPT_MATH_PROOF + CONCEPT_MASTER) [Owner: conceptualizer]

## Rule: C-0 Iterative Cycle
- C-0 выполняется итерациями до стабилизации выбранного Пользователем варианта.
- Если после C-0.6 Пользователь меняет вариант/параметры, Оркестратор обязан повторно открыть C-0.2..C-0.6 (вернуть в `[ ]`) и запустить новый цикл.
- Переход в GA-1 разрешён только после закрытия C-0.2..C-0.6 в последнем актуальном цикле и синхронизации канона.

## Link to Global TASKS
- Ref: US/FEAT in .memory/TASKS.md
