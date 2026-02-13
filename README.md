# Cabal System — Логическая схема больших проектов
![Cabal System схема](https://raw.githubusercontent.com/Korino1/Cabal-System/main/.memory/Cabal%20System.png)

## Назначение
Этот репозиторий содержит логическую схему (Protocol 3.1) для создания больших проектов и координации AI/агент‑workflow. Подход документационный: от концепта к логике, методам, функциям и интеграции. Код создаётся только после завершения описания функций.

## Состав репозитория
- .memory/ — рабочая память и состояние (миссия, контекст, задачи, фазы, журналы).
- spec/ — формальные документы (контракты, ADR, схема и гейты фаз, концепт + мат.обоснование).
- arch/ — архитектурные заметки, регистровые карты и референсы для низкоуровневой оптимизации (Zen4: `arch/zen4/zen4_registers.md`; Agner Fog: `arch/main/` и `arch/main/THIRD_PARTY.md`).
- agent/ — роли субагентов и их обязанности.
- scripts/ — утилиты (checkpoint, phase tools, Edit Harness scripts).
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

## Режимы CONSULT: USER_TRACKING и YOLO
Перед первым запуском GA-1 после окончательного утверждения концепта Пользователь выбирает режим маршрутизации CONSULT:
- `USER_TRACKING` — Пользователь сам отслеживает прогресс и отвечает на CONSULT напрямую.
- `YOLO` — каждый CONSULT уходит Оркестратору.

Как работает `YOLO`:
1. Оркестратор запрашивает у Пользователя уточнение и расширение сквозных правил.
2. Новые правила фиксируются как дополнение к уже действующим сквозным правилам.
3. Только после этого режим `YOLO` считается активированным.
4. Далее каждый CONSULT обрабатывается Оркестратором: он выбирает профильного исполнителя и ставит задачу строго по сквозным правилам.

## Пятиязычный синтез и выборка решений (RU/EN/ZH/DE/FR)
Это внутренний рабочий механизм агентов для математики и сложных логических узлов. Пользователь не обязан выполнять эти шаги вручную.

Почему результаты между языками действительно могут отличаться:
- У каждого языка своя семантика: одна и та же идея в RU/EN/ZH/DE/FR по-разному раскрывает логику, допущения и граничные условия.
- Следующий токен генерируется по-разному в зависимости от языка (разные вероятностные распределения и паттерны продолжения рассуждения).
- Обучающие материалы по языкам имели разную наполненность (объём, плотность терминов, типовые формулировки), поэтому модель может по-разному приходить к выводу.
- На практике это даёт разные цепочки рассуждений и иногда разные математические результаты, поэтому в протоколе обязателен явный синтез RU/EN/ZH/DE/FR.

Базовый процесс:
1. Сформировать базовое решение и доказательства на русском (RU).
2. Независимо повторить рассуждение на английском (EN).
3. Независимо повторить рассуждение на китайском (ZH).
4. Независимо повторить рассуждение на немецком (DE).
5. Независимо повторить рассуждение на французском (FR).
6. Сравнить RU/EN/ZH/DE/FR версии и явно зафиксировать разночтения (термины, кванторы, ограничения, условия применимости, параметры).
7. Выполнить отдельный `math-critique`: обозначить слабые/неучтённые места и риски.
8. Подготовить минимум 2 варианта решения для Пользователя (обычно: более строгий и более практичный).
9. Если Пользователь предлагает свой метод, этот метод обязательно проходит независимую математическую валидацию.
10. Если метод не предложен, решение ищется/синтезируется через тот же цикл RU/EN/ZH/DE/FR.
11. После выбора решения рабочие записи продолжаются на русском языке; итог неизменен: глобальный синтез для нахождения корректного решения.

Почему это даёт преимущества:
- Снижает риск семантических ошибок: одна и та же идея проверяется в пяти языковых семантиках.
- Ловит скрытые допущения и слабые места, которые часто незаметны в одном языковом контуре.
- Улучшает качество выбора: у Пользователя есть минимум 2 осмысленных варианта с явными компромиссами.
- Повышает воспроизводимость и аудитопригодность: разночтения и основания выбора фиксируются явно.
- Уменьшает вероятность ложноположительного «всё корректно», если решение на самом деле хрупкое.

## Edit Harness для правок кода и документов
Для повышения точности правок в Cabal System введён обязательный протокол `read -> hash verify -> apply`.

Что это даёт:
- Снижает ошибки при частичных/пересекающихся правках (защита от `stale edit`).
- Делает изменения воспроизводимыми: каждая операция проверяется по `expected_hash`.
- Уменьшает риск «слепых» замен и скрытых регрессий в документах и коде.

Где зафиксировано:
- Канон: `spec/docs/EDIT_HARNESS.md`.
- Чтение диапазона: `scripts/harness_read.ps1`.
- Применение операций: `scripts/harness_apply.ps1`.

Для всех ролей с правом правки это сквозное правило и QA-гейт.

## Low-level fallback для инженерных ролей (JA -> ZH -> EN)
Этот режим применяется для low-level задач в ролях `rust-engineer`, `simd-specialist`, `debuger`, `fixer` (и для `qa-agent` при проверке этих итераций).

Когда обязателен:
- задачи по `asm`, intrinsics, `unsafe`, FFI, ABI/calling convention, alignment/aliasing/UB, memory ordering, а также сложные performance-регрессии hot-path;
- ситуация помечена как «нерешаемо/трудно решаемо» в текущем контексте.

Цикл fallback:
1. Проверка/поиск сначала на японском (JA).
2. Затем проверка на китайском (ZH).
3. Затем проверка на английском (EN).
4. Синтез найденного и выбор итогового решения с явным обоснованием.

Фиксация результата:
- В отчёте/WORKLOG фиксируется, что именно найдено на JA/ZH/EN и почему выбран итоговый вариант.
- После нахождения рабочего направления дальнейшая рабочая фиксация ведётся на русском языке (RU).

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
- scripts/ — utilities (checkpoint, phase tools, Edit Harness scripts).
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

## CONSULT Modes: USER_TRACKING and YOLO
Before the first GA-1 run (after the concept is finalized), the user selects a CONSULT routing mode:
- `USER_TRACKING` — the user tracks progress and answers CONSULT items directly.
- `YOLO` — every CONSULT is routed to the Orchestrator.

How `YOLO` works:
1. The Orchestrator asks the user to clarify and extend cross-cutting rules.
2. These rules are recorded as additions to existing cross-cutting rules.
3. Only then is `YOLO` considered active.
4. After activation, each CONSULT is handled by the Orchestrator, who assigns a suitable subagent and enforces all cross-cutting rules.

## Five-Language Synthesis and Solution Selection (RU/EN/ZH/DE/FR)
This is an internal agent workflow for mathematical and hard-logic decisions; end users do not execute these steps manually.

Why results can genuinely differ across languages:
- Each language carries its own semantics, so the same idea can expose logic, assumptions, and boundary conditions differently in RU/EN/ZH/DE/FR.
- Next-token generation is language-dependent (different probability distributions and continuation patterns).
- Training corpora differ by language (volume, term density, typical formulations), so the model may reach conclusions differently.
- In practice this changes reasoning trajectories and can produce different mathematical outcomes, which is why explicit RU/EN/ZH/DE/FR synthesis is mandatory in the protocol.

Core workflow:
1. Build the base solution and proofs in Russian (RU).
2. Re-run the reasoning independently in English (EN).
3. Re-run the reasoning independently in Chinese (ZH).
4. Re-run the reasoning independently in German (DE).
5. Re-run the reasoning independently in French (FR).
6. Compare RU/EN/ZH/DE/FR and explicitly capture divergences (terms, quantifiers, constraints, applicability conditions, parameters).
7. Run a dedicated `math-critique` pass to expose weak or missing points.
8. Prepare at least 2 solution options for the user (typically stricter vs more practical).
9. If the user provides a custom method, it must be independently validated by the math critic.
10. If no method is provided, the solution is searched/synthesized through the same RU/EN/ZH/DE/FR loop.
11. After selecting the final path, operational documentation continues in Russian; the final goal remains a global synthesis to find the correct solution.

Why this improves results:
- Reduces semantic-error risk by validating the same logic across five language semantics.
- Surfaces hidden assumptions and weak points that are often missed in a single-language pass.
- Improves decision quality by presenting at least 2 explicit options with trade-offs.
- Increases reproducibility and auditability: divergences and rationale are recorded explicitly.
- Lowers false confidence in fragile solutions that might look correct in one language only.

## Edit Harness for Code and Document Changes
Cabal System now enforces a mandatory `read -> hash verify -> apply` protocol for file edits.

Why this helps:
- Reduces partial-overlap edit failures (`stale edit` protection).
- Makes edits reproducible: each operation is checked against `expected_hash`.
- Lowers the risk of blind replacements and hidden regressions in docs/code.

Where it is defined:
- Canon: `spec/docs/EDIT_HARNESS.md`.
- Range read tool: `scripts/harness_read.ps1`.
- Apply tool: `scripts/harness_apply.ps1`.

For editing roles this is a cross-cutting rule and a QA gate.

## Low-Level Fallback for Engineering Roles (JA -> ZH -> EN)
This mode is used for low-level tasks in `rust-engineer`, `simd-specialist`, `debuger`, `fixer` (and by `qa-agent` when reviewing those iterations).

When it is mandatory:
- tasks involving `asm`, intrinsics, `unsafe`, FFI, ABI/calling convention, alignment/aliasing/UB, memory ordering, and hard hot-path performance regressions;
- the case is marked as "unsolved/hard to solve" in the current context.

Fallback cycle:
1. Analyze/search in Japanese (JA) first.
2. Then analyze in Chinese (ZH).
3. Then analyze in English (EN).
4. Synthesize findings and choose a final solution with explicit rationale.

Result logging:
- The report/WORKLOG must explicitly state what was found in JA/ZH/EN and why the final option was chosen.
- After a workable direction is found, ongoing operational documentation continues in Russian (RU).

## Scope
This is a process framework and artifact set for planning and coordination. It is not a code library.

## License
Repository-owned materials are licensed under `Apache License 2.0`; see the root `LICENSE` file.

The `arch/main/` folder includes third-party materials by Agner Fog with their own licenses (CC BY-SA 4.0 and GPLv3); these are not overridden by `Apache-2.0`. See `arch/main/THIRD_PARTY.md`.

## Contributing
Ideas, fixes, and improvements are welcome. Open an issue or a pull request with a clear goal.
