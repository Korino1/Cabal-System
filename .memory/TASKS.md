---
id: tasks
updated: 2026-02-08
---

# Tasks (канбан)
> Политика: вместе с имплементационными пунктами обязательно веди задачи для размышлений и консультаций с Пользователем.  Формулируй их с префиксом `CONSULT` или `REFLECT`, следуя разделу «Практика CONSULT/REFLECT» в `agents.md`.  Все фикса-задачи обязаны содержать DEFECT ID (DEF-YYYYMMDD-###).  Каждая задача обязана иметь Owner: `[Owner: <Агент>]`.  Активная задача и Next Steps фиксируются в `.memory/STATE.md` (или через `scripts/checkpoint.ps1`).

> Формат Kanban с иерархией:  
> `[ ]` — не начато, `[~]` — в работе, `[x]` — выполнено.  
> Уровни: **EP → FEAT → US → T**.  
---
## 📘 Формат
**Уровни задач:**
- `EP` — Epic (цель, объединяет фичи)  
- `FEAT` — Feature (функциональная часть)  
- `US` — User Story (поведение пользователя)  
- `T` — Task (конкретное действие)


## TODO

- [ ] EP CONCEPT — Обобщённый концепт (C-0) [Owner: conceptualizer]
  [ ] US CONCEPT.GOV — Governance & Discovery [Owner: orchestrator]
    [ ] T CONCEPT.GOV.1 — CONSULT — границы и формат обобщённого концепта [Owner: orchestrator]
    [ ] T CONCEPT.GOV.2 — REFLECT — риски потери деталей при обобщении [Owner: orchestrator]
    [ ] T CONCEPT.GOV.3 — CONSULT — сквозные правила (запреты/разрешения методик/софта/библиотек) [Owner: orchestrator]
  [ ] FEAT CONCEPT.MASTER — Concept Master (C-0) [Owner: conceptualizer]
    [ ] US CONCEPT.MASTER.1 — Сведение концепта [Owner: conceptualizer]
      [ ] T CONCEPT.MASTER.1.0 — Сквозные правила: опросник + фиксация канона (CONCEPT_MASTER:6-7) [Owner: conceptualizer]
      [ ] T CONCEPT.MASTER.1.MATH — CONCEPT_MATH_PROOF: леммы/инварианты/коридоры параметров [Owner: mathematician]
      [ ] T CONCEPT.MASTER.1.1 — Сбор и синтез концепта (C-0.1) [Owner: conceptualizer]
      [ ] T CONCEPT.MASTER.1.2 — Проверка корректности и правки (C-0.2) [Owner: conceptualizer]

- [ ] EP ECDLP — Решение ecdlp secp256k1 puzzle 135 с квантовыми симуляциями и оптимизациями Zen4 [Owner: orchestrator]
  [ ] US ECDLP.GOV — Governance & Discovery [Owner: orchestrator]
    [ ] T ECDLP.GOV.1 — CONSULT — расположение файлов концепта и уточнение требований [Owner: orchestrator]
    [ ] T ECDLP.GOV.2 — REFLECT — анализ рисков и альтернатив архитектуры [Owner: orchestrator]
  [ ] FEAT ECDLP.CORE — Ядро решения (открывать после закрытия GOV) [Owner: orchestrator]

- [x] EP CTXSHARD — Разделение истории и контекста по этапам проекта [Owner: orchestrator]
  [x] US CTXSHARD.GOV — Governance & Discovery [Owner: orchestrator]
    [x] T CTXSHARD.GOV.1 — CONSULT — целевая структура фаз/этапов и правила переходов [Owner: orchestrator]
    [x] T CTXSHARD.GOV.2 — REFLECT — риски раздробления контекста и критерии минимального ядра [Owner: orchestrator]
  [x] FEAT CTXSHARD.RULES — Регламент хранения контекста по фазам [Owner: orchestrator]
    [x] US CTXSHARD.RULES.1 — Фазовая структура артефактов и шаблоны [Owner: orchestrator]
      [x] T CTXSHARD.RULES.1.1 — Создать PHASES/* и GLOBAL_INDEX [Owner: orchestrator]
      [x] T CTXSHARD.RULES.1.2 — Обновить TRACKING/BOOTSTRAP/README [Owner: orchestrator]
      [x] T CTXSHARD.RULES.1.3 — Обновить INDEX.yaml и checkpoint.ps1 [Owner: orchestrator]


## GOV Template (reference)
- [ ] EP XXX — Название эпика [Owner: orchestrator]  
  [ ] US XXX.GOV — Governance & Discovery [Owner: orchestrator]  
    [ ] T XXX.GOV.1 — CONSULT — ключевой вопрос к Пользователю [Owner: orchestrator]  
    [ ] T XXX.GOV.2 — REFLECT — анализ рисков/альтернатив [Owner: orchestrator]  
  [ ] FEAT XXX.Y — Функциональный блок (открывать после закрытия GOV) [Owner: orchestrator]
## Fix Task Template (requires DEFECT ID)
- [ ] EP BASE — Исправления дефектов [Owner: orchestrator]
  [ ] US BASE.GOV — Governance & Discovery [Owner: orchestrator]
    [ ] T BASE.GOV.1 — CONSULT — приоритеты/границы фиксов [Owner: orchestrator]
    [ ] T BASE.GOV.2 — REFLECT — риски регрессий [Owner: orchestrator]
  [ ] FEAT BASE.IM — Исправление дефектов [Owner: orchestrator]
    [ ] US BASE.IM.1 — Поток исправлений [Owner: orchestrator]
      [ ] T BASE.IM.1.1 — Fix DEF-YYYYMMDD-### — краткое описание [Owner: fixer]






