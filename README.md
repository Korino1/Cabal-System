# Cabal System — Логическая схема больших проектов

## Назначение
Этот репозиторий содержит логическую схему (Protocol 3.1) для создания больших проектов и координации AI/агент‑workflow. Подход документационный: от концепта к логике, методам, функциям и интеграции. Код создаётся только после завершения описания функций.

## Состав репозитория
- .memory/ — рабочая память и состояние (миссия, контекст, задачи, фазы, журналы).
- spec/ — формальные документы (контракты, ADR, схема и гейты фаз, концепт + мат.обоснование).
- arch/ — архитектурные заметки, регистровые карты и референсы для низкоуровневой оптимизации (Zen4: `arch/zen4/zen4_registers.md`; Agner Fog: `arch/main/` и `arch/main/THIRD_PARTY.md`).
- agent/ — роли субагентов и их обязанности.
- scripts/ — утилиты (например, checkpoint).
- ENG/ — (опционально) английское зеркало ключевых документов.

## Коротко о протоколе
- C-0 — обобщённый концепт.
- GA-1 — логическая схема концепта.
- GA-2 — блоки → методы.
- GA-3 — схемы блоков.
- GA-4 — методы → функции.
- GA-5 — описания функций.
- ARCH — дополнение плана архитектурой.
- INTEGRATOR — связи функций.
- ORCHESTRATOR — назначения субагентов и порядок работ.

Кодирование допускается только после GA-5.

## Как пользоваться
1. Зафиксируйте/обновите математическое обоснование и инварианты в `spec/docs/CONCEPT_MATH_PROOF.md` и сквозные правила (разделы 6-7) в `spec/docs/CONCEPT_MASTER.md` (фаза C-0).
2. Следуйте активной фазе из .memory/GLOBAL_INDEX.md.
3. Записывайте результаты в .memory/LOGIC_PROTOCOL.md и PHASES/<Phase>/DIGEST.md.
4. Применяйте единый формат фаз из spec/docs/PHASE_SCHEMA.md и чек‑лист из spec/docs/PHASE_GATE.md.
5. Ведите учёт прогресса в .memory/* (WORKLOG, STATE и т.д.).
6. Интеграция агентов в CLI IDE (opencode, factory droid, claudecode и т.п.): скопируйте файлы из `agent/` в нужные пути вашей IDE и настройте модели по их документации. Затем копируйте содержимое репозитория в рабочую область, КРОМЕ папки `agent/` (если ваша IDE хранит агентов отдельно).
7. Интеграция для плагинов VS Code (RooCode/KiloCode): агенты обычно представлены в виде `modes`. Замените внутренние правила каждого mode на правила из соответствующего файла в `agent/` и задайте каждому mode имя, идентичное имени агента.
8. При указании стартового промпта советую максимально детализировать и не жалеть токенов для описания. От этого зависят уточняющие вопросы и сквозные правила, которые протянутся на всех этапах на каждого агента.

## Статус и область применения
Это процессный фреймворк и набор артефактов для планирования и координации. Он не является библиотекой или готовым кодом.

## Лицензия
В корне репозитория пока нет общего файла `LICENSE` (добавьте, чтобы явно зафиксировать условия использования).

В `arch/main/` содержатся third-party материалы Agner Fog со своими лицензиями (CC BY-SA 4.0 и GPLv3) — см. `arch/main/THIRD_PARTY.md`.

## Вклад
Идеи, исправления и улучшения приветствуются. Открывайте issue или pull request с чётким описанием цели.

---

# Cabal System — Logical Protocol for Large Projects (Protocol 3.1)

## Purpose
This repository contains a logical protocol (3.1) for building large projects and coordinating AI/agent workflows. The approach is documentation‑first: concept → logic → methods → functions → integration. Code is written only after function descriptions are completed.

## Repository Layout
- .memory/ — working memory and state (mission, context, tasks, phases, logs).
- spec/ — formal documents (contracts, ADRs, phase schema/gates, concept + math proof).
- arch/ — architecture notes, register maps, and low-level optimization references (Zen4: `arch/zen4/zen4_registers.md`; Agner Fog: `arch/main/` and `arch/main/THIRD_PARTY.md`).
- agent/ — subagent roles and responsibilities.
- scripts/ — utilities (e.g., checkpoint).
- ENG/ — (optional) English mirror of core docs.

## Protocol Overview
- C-0 — consolidated concept.
- GA-1 — logical concept schema.
- GA-2 — blocks → methods.
- GA-3 — block schemas.
- GA-4 — methods → functions.
- GA-5 — function descriptions.
- ARCH — architecture plan additions.
- INTEGRATOR — function links.
- ORCHESTRATOR — subagent assignments and order.

Coding is allowed only after GA-5.

## How to Use
1. Capture/update the math proof and invariants in `spec/docs/CONCEPT_MATH_PROOF.md` and the cross-cutting rules (sections 6-7) in `spec/docs/CONCEPT_MASTER.md` (phase C-0).
2. Follow the active phase in .memory/GLOBAL_INDEX.md.
3. Record outputs in .memory/LOGIC_PROTOCOL.md and PHASES/<Phase>/DIGEST.md.
4. Use spec/docs/PHASE_SCHEMA.md and spec/docs/PHASE_GATE.md for phase format and gates.
5. Keep progress accounting in .memory/* (WORKLOG, STATE, etc.).
6. CLI IDE integration (opencode, factory droid, claudecode, etc.): copy the files from `agent/` into your tool's agent locations and configure models per its docs. Then copy the repository contents into your working project directory, excluding `agent/` (if your tool keeps agents separately).
7. VS Code plugins (RooCode/KiloCode): agents are commonly represented as `modes`. Replace each mode's internal rules with the contents of the corresponding file in `agent/`, and name each mode identically to the agent name.
8. Make the initial prompt as detailed as possible. This strongly affects follow-up questions and the cross-cutting rules that will be enforced at every stage.

## Scope
This is a process framework and artifact set for planning and coordination. It is not a code library.

## License
The repository root currently has no global `LICENSE` file (add one to define usage terms).

The `arch/main/` folder includes third-party materials by Agner Fog with their own licenses (CC BY-SA 4.0 and GPLv3); see `arch/main/THIRD_PARTY.md`.

## Contributing
Ideas, fixes, and improvements are welcome. Open an issue or a pull request with a clear goal.
