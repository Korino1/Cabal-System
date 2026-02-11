# Cabal System — Логическая схема больших проектов
![Cabal System схема](https://raw.githubusercontent.com/Korino1/Cabal-System/main/.memory/Cabal%20System.png)

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
Коротко: пользователю не нужно вручную собирать карту логики, фазы, леммы и служебные журналы. Это делают агенты.

Минимальный сценарий для пользователя:
1. Описать цель и контекст задачи.
2. Указать ограничения, запреты и приоритеты.
3. Ответить на уточняющие вопросы агентов.
4. Подтвердить выбранный вариант на ключевых развилках.

Внутреннюю работу выполняют агенты:
1. Ведут протокол 3.1 и полную карту фаз/декомпозиции.
2. Делают математическое обоснование, `math-critique`, QA-анти-имитацию и синтез вариантов.
3. Поддерживают рабочие артефакты и записи (`.memory/*`, `spec/docs/*`).

Интеграция:
1. CLI IDE (opencode, factory droid, claudecode и т.п.): скопируйте файлы из `agent/` в нужные пути вашей IDE и настройте модели по документации IDE. Затем копируйте содержимое репозитория в рабочую область, кроме `agent/` (если IDE хранит агентов отдельно).
2. VS Code плагины (RooCode/KiloCode): агенты обычно представлены в виде `modes`. Замените внутренние правила каждого mode содержимым соответствующего файла из `agent/` и задайте mode имя, идентичное имени агента.

## Трёхъязычный синтез и выборка решений (RU/EN/ZH)
Это внутренний рабочий механизм агентов для математики и сложных логических узлов. Пользователь не обязан выполнять эти шаги вручную.

Базовый процесс:
1. Сформировать базовое решение и доказательства на русском (RU).
2. Независимо повторить рассуждение на английском (EN).
3. Независимо повторить рассуждение на китайском (ZH).
4. Сравнить RU/EN/ZH версии и явно зафиксировать разночтения (термины, кванторы, ограничения, условия применимости, параметры).
5. Выполнить отдельный `math-critique`: обозначить слабые/неучтённые места и риски.
6. Подготовить минимум 2 варианта решения для Пользователя (обычно: более строгий и более практичный).
7. Если Пользователь предлагает свой метод, этот метод обязательно проходит независимую математическую валидацию.
8. Если метод не предложен, решение ищется/синтезируется через тот же цикл RU/EN/ZH.
9. После выбора решения рабочие записи продолжаются на русском языке.

Почему это даёт преимущества:
- Снижает риск семантических ошибок: одна и та же идея проверяется в трёх языковых семантиках.
- Ловит скрытые допущения и слабые места, которые часто незаметны в одном языковом контуре.
- Улучшает качество выбора: у Пользователя есть минимум 2 осмысленных варианта с явными компромиссами.
- Повышает воспроизводимость и аудитопригодность: разночтения и основания выбора фиксируются явно.
- Уменьшает вероятность ложноположительного «всё корректно», если решение на самом деле хрупкое.

## Статус и область применения
Это процессный фреймворк и набор артефактов для планирования и координации. Он не является библиотекой или готовым кодом.

## Лицензия
Собственные материалы репозитория лицензированы по `Apache License 2.0` — см. файл `LICENSE` в корне.

В `arch/main/` содержатся third-party материалы Agner Fog со своими лицензиями (CC BY-SA 4.0 и GPLv3) — они не переопределяются `Apache-2.0`, см. `arch/main/THIRD_PARTY.md`.

## Вклад
Идеи, исправления и улучшения приветствуются. Открывайте issue или pull request с чётким описанием цели.

![Cabal System схема](https://raw.githubusercontent.com/Korino1/Cabal-System/refs/heads/main/.memory/NODjpg.jpg)
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
Short version: users do not manually build protocol maps, phases, lemma structures, or tracking journals. Agents handle that internally.

Minimal user flow:
1. Describe the goal and task context.
2. Specify constraints, prohibitions, and priorities.
3. Answer agent clarifying questions.
4. Approve selected options at key decision points.

Internal workflow handled by agents:
1. Maintain Protocol 3.1 and the full phase/decomposition map.
2. Run math proofing, `math-critique`, QA anti-simulation checks, and option synthesis.
3. Keep working artifacts and records updated (`.memory/*`, `spec/docs/*`).

Integration:
1. CLI IDEs (opencode, factory droid, claudecode, etc.): copy files from `agent/` into your IDE agent locations and configure models per the IDE docs. Then copy repository contents into your working area, excluding `agent/` if agents are stored separately.
2. VS Code plugins (RooCode/KiloCode): agents are usually represented as `modes`. Replace each mode's internal rules with the matching file from `agent/`, and keep mode names identical to agent names.

## Three-Language Synthesis and Solution Selection (RU/EN/ZH)
This is an internal agent workflow for mathematical and hard-logic decisions; end users do not execute these steps manually.

Core workflow:
1. Build the base solution and proofs in Russian (RU).
2. Re-run the reasoning independently in English (EN).
3. Re-run the reasoning independently in Chinese (ZH).
4. Compare RU/EN/ZH and explicitly capture divergences (terms, quantifiers, constraints, applicability conditions, parameters).
5. Run a dedicated `math-critique` pass to expose weak or missing points.
6. Prepare at least 2 solution options for the user (typically stricter vs more practical).
7. If the user provides a custom method, it must be independently validated by the math critic.
8. If no method is provided, the solution is searched/synthesized through the same RU/EN/ZH loop.
9. After selecting the final path, operational documentation continues in Russian.

Why this improves results:
- Reduces semantic-error risk by validating the same logic across three language semantics.
- Surfaces hidden assumptions and weak points that are often missed in a single-language pass.
- Improves decision quality by presenting at least 2 explicit options with trade-offs.
- Increases reproducibility and auditability: divergences and rationale are recorded explicitly.
- Lowers false confidence in fragile solutions that might look correct in one language only.

## Scope
This is a process framework and artifact set for planning and coordination. It is not a code library.

## License
Repository-owned materials are licensed under `Apache License 2.0`; see the root `LICENSE` file.

The `arch/main/` folder includes third-party materials by Agner Fog with their own licenses (CC BY-SA 4.0 and GPLv3); these are not overridden by `Apache-2.0`. See `arch/main/THIRD_PARTY.md`.

## Contributing
Ideas, fixes, and improvements are welcome. Open an issue or a pull request with a clear goal.
