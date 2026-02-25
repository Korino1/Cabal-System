# Cabal System — Логическая схема больших проектов
![Cabal System схема](https://raw.githubusercontent.com/Korino1/Cabal-System/main/.memory/Cabal%20System.png)

## Назначение
`Cabal System` — это система управления работой ИИ в проекте.

Простыми словами:
- вы подключаете Cabal через MCP в IDE;
- ставите задачу обычным текстом;
- Cabal управляет порядком шагов, проверками, ограничениями и логированием.

Главная идея: не “свободный чат”, а управляемый процесс с правилами и контролем качества.

## Состав репозитория
- .memory/ — рабочая память и состояние (миссия, контекст, задачи, фазы, журналы).
- spec/ — формальные документы (контракты, ADR, схема и гейты фаз, концепт + мат.обоснование).
- arch/ — архитектурные заметки, регистровые карты и референсы для низкоуровневой оптимизации (Zen4: `arch/zen4/zen4_registers.md`; Agner Fog: `arch/main/` и `arch/main/THIRD_PARTY.md`).
- agent/ — роли субагентов и их обязанности.
- scripts/ — утилиты (checkpoint, phase tools, Edit Harness scripts).
- ENG/ — (опционально) английское зеркало ключевых документов.
- cabal-mcp-runtime/ — программный runtime Cabal (MCP control plane, policy/gate/audit/proxy).

## Cabal-MCP режим (актуальный)
- Для исполнения логики проектов используйте `cabal-mcp-runtime` через MCP (`stdio`).
- Документы схемы (`spec/docs/*`, `.memory/*`) остаются источником формализации и аудита, но не обязательны для чтения моделью в `MCP-only` контуре.
- Для строгого file-based gate включайте `cabal.set_gate_policy {"strict_artifacts": true}`.

## Статус агентского контура (GitHub)
- Все профили в `agent/*.md` актуализированы под `MCP-only`.
- В каждом профиле добавлен обязательный раздел `MCP-Only Контракт (Cabal-MCP, обязательно)`.
- Файлы логической схемы используются как артефакты формализации и аудита, а не как исполняемые инструкции для модели.

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
Коротко: пользователю не нужно вручную вести фазы, журналы и служебные проверки. Это делает Cabal.

Минимальный сценарий для пользователя:
1. Подключить Cabal-MCP в IDE.
2. Описать цель проекта простым языком.
3. Сразу указать ограничения: что запрещено, что критично, какие сроки и приоритеты.
4. Выбрать режим CONSULT: `USER_TRACKING` или `YOLO`.
5. Подтверждать ключевые развилки (или делегировать их Оркестратору в `YOLO`).

Что делает система автоматически:
1. Ведёт фазовый протокол и контроль переходов.
2. Проверяет правила, ограничения и гейты.
3. Маршрутизирует CONSULT-запросы нужным ролям.
4. Ведёт audit/log для воспроизводимости.

## Быстрый старт (без программирования)
1. Установите IDE/плагин с поддержкой MCP (например RooCode).
2. Добавьте MCP-сервер Cabal в настройки IDE (путь к `cabal-mcp-runtime`).
3. Перезапустите IDE и откройте чат с моделью.
4. Напишите задачу в формате:
`Цель + ограничения + что запрещено + какой результат нужен`.
5. Работайте через Cabal-инструменты, а не через ручные “обходы”.

## Подключение Cabal-MCP (детально)
Ниже самый простой и надёжный путь для Windows.

1. Подготовьте бинарник Cabal.
   - Если у вас уже есть файл `C:\cabal-mcp\cabal-mcp-runtime-rooformat.exe`, можно использовать его.
   - Если файла нет, соберите его из `cabal-mcp-runtime` и укажите полный путь к `cabal-mcp-runtime.exe`.
2. Откройте в IDE настройки MCP.
   - Найдите раздел MCP Servers (или “Добавить MCP сервер”).
   - Нажмите “Add server”.
3. Добавьте сервер с именем `cabal-server`.
   - Транспорт: `stdio`.
   - Команда: полный путь к exe.
   - Аргументы: пусто (`[]`).
   - Переменные окружения: как в примере ниже.
4. Сохраните настройки и перезапустите IDE (или MCP panel).
5. Проверьте подключение.
   - В MCP-панели сервер должен быть в статусе `Connected`.
   - В списке инструментов должен появиться Cabal (минимальный bootstrap-набор).
6. Сделайте быструю проверку в чате модели:
   - запросите `cabalget_state`;
   - затем `cabaltool_search` с простым запросом, например `"state"`.

Важно:
- Путь к `command` должен быть абсолютным.
- Если путь содержит пробелы, всё равно указывайте его одной строкой в поле `command` (не разбивайте на части).
- Не запускайте через `cmd /c ...`, если IDE этого не требует.

Пример MCP-конфига (Windows, `stdio`):
```json
{
  "transport": "stdio",
  "command": "C:\\cabal-mcp\\cabal-mcp-runtime-rooformat.exe",
  "args": [],
  "env": {
    "CABAL_MCP_TOOL_NAME_FORMAT": "roo",
    "CABAL_MCP_COMPAT_ALIAS_PROFILE": "none"
  }
}
```

Если не подключается:
1. Ошибка `MCP error -32001: Request timed out`:
   - чаще всего неверный путь в `command`;
   - проверьте, что файл существует и запускается вручную.
2. Ошибка `Connection closed`:
   - обычно процесс завершился сразу из-за неправильного конфига;
   - проверьте JSON на лишние символы и перезапустите IDE.
3. Ошибка вида `'D:\\... is not recognized...'`:
   - путь был передан некорректно (разбит из-за пробелов);
   - укажите путь как один полный путь к `.exe`.

## Важно про инструменты MCP
- Cabal использует role-based доступ: набор инструментов зависит от активной роли.
- Включён lazy-режим: в `tools/list` показывается только минимальный bootstrap-набор, а остальные инструменты подтягиваются через поиск.
- Для поиска используйте `cabal.tool_search`, для полной схемы конкретного инструмента — `cabal.get_tool_schema`.
- Для последовательных вызовов используйте `cabal.programmatic_call` (уменьшает нагрузку на контекст).

## Примеры запуска (Cabal MCP Runtime)
Из корня репозитория:

1. Быстрый локальный smoke:
```powershell
cd .\cabal-mcp-runtime
cargo test -q
```

2. Консольная справка и pre-start gate-флаг:
```powershell
cd .\cabal-mcp-runtime
cargo run --release -- --help
cargo run --release -- --strict-artifacts
```

3. Unified release gate (stress + IDE contract + schema):
```powershell
cd .\cabal-mcp-runtime
powershell -ExecutionPolicy Bypass -File .\scripts\check-release-gates.ps1 -WithIntegration
```

4. Финальная готовность (snapshot-режим, без GitHub API):
```powershell
cd .\cabal-mcp-runtime
$snapshot = ".\.cabal_runtime\branch_protection_snapshot.json"
@{ required_status_checks = @{ contexts = @("stress-sla-gate","ide-contract-gate","ide-e2e-report-schema-gate","release-summary-schema-gate","release-gate") } } | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 $snapshot
powershell -ExecutionPolicy Bypass -File .\scripts\check-final-readiness.ps1 -IdeE2EReportPath .\..\spec\contracts\ide_e2e_report.pass.json -ProtectionJsonPath $snapshot
```

5. Финальная готовность (GitHub-режим):
```powershell
cd .\cabal-mcp-runtime
powershell -ExecutionPolicy Bypass -File .\scripts\check-final-readiness.ps1 -IdeE2EReportPath .\..\spec\docs\ide_e2e_report.json -RepoOwner "<owner>" -RepoName "<repo>" -Branch "main"
```

## Режимы CONSULT: USER_TRACKING и YOLO
Перед первым запуском GA-1 после окончательного утверждения концепта Пользователь выбирает режим маршрутизации CONSULT:
- `USER_TRACKING` — Пользователь сам отслеживает прогресс и отвечает на CONSULT напрямую.
- `YOLO` — каждый CONSULT уходит Оркестратору.

Как работает `YOLO`:
1. Оркестратор запрашивает у Пользователя уточнение и расширение сквозных правил.
2. Новые правила фиксируются как дополнение к уже действующим сквозным правилам.
3. Только после этого режим `YOLO` считается активированным.
4. Далее каждый CONSULT обрабатывается Оркестратором: он выбирает профильного исполнителя и ставит задачу строго по сквозным правилам.

## Обязательное ознакомление со сквозными правилами
- Перед началом каждой новой задачи/итерации любой агент и субагент обязан прочитать полный актуальный список сквозных правил из `spec/docs/CONCEPT_MASTER.md` (раздел 6).
- Если правила были расширены, исполнитель сначала знакомится со всем перечнем (базовые + новые), и только после этого начинает работу.
- В режиме `YOLO` Оркестратор обязан передавать исполнителю в каждой CONSULT-постановке полный актуальный набор сквозных правил.
- Работа без актуального ознакомления со сквозными правилами считается нарушением протокола.

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
- cabal-mcp-runtime/ — Cabal runtime implementation (MCP control plane, policy/gate/audit/proxy).

## Cabal-MCP Mode (Current)
- For execution, use `cabal-mcp-runtime` through MCP (`stdio` transport).
- Logical documents (`spec/docs/*`, `.memory/*`) remain formalization/audit artifacts and are not required as model-readable instructions in `MCP-only` flow.
- Enable strict file-based gates only when needed via `cabal.set_gate_policy {"strict_artifacts": true}`.

## Agent Runtime Status (GitHub)
- All profiles in `agent/*.md` are aligned to `MCP-only`.
- Each profile contains a mandatory section: `MCP-Only Контракт (Cabal-MCP, обязательно)`.
- Logical-scheme files are treated as formalization/audit artifacts, not executable model instructions.

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
Short version: non-technical users can work with Cabal without manually managing phases, logs, or internal rules.

Simple flow:
1. Connect Cabal MCP server in your IDE.
2. Write your goal in plain language.
3. Add constraints: what is forbidden, what is critical, what has priority.
4. Choose CONSULT mode: `USER_TRACKING` or `YOLO`.
5. Confirm key decisions (or delegate to Orchestrator in `YOLO`).

What Cabal handles automatically:
1. Phase protocol and gate checks.
2. Routing requests to the right role.
3. Audit trail and reproducible state.

MCP context optimization:
1. `tools/list` shows a small bootstrap set in lazy mode.
2. Use `cabal.tool_search` to find tools.
3. Use `cabal.get_tool_schema` to load full schema only when needed.
4. Use `cabal.programmatic_call` for compact multi-step execution.

## Quick Start (No Coding)
1. Install an IDE/plugin that supports MCP (for example, RooCode).
2. Add Cabal MCP server to IDE settings.
3. Restart IDE and open a model chat.
4. Describe your task in plain language:
`Goal + constraints + what is forbidden + expected result`.
5. Work through Cabal tools and avoid direct bypass commands.

Example MCP config (Windows, `stdio`):
```json
{
  "transport": "stdio",
  "command": "C:\\cabal-mcp\\cabal-mcp-runtime-rooformat.exe",
  "args": [],
  "env": {
    "CABAL_MCP_TOOL_NAME_FORMAT": "roo",
    "CABAL_MCP_COMPAT_ALIAS_PROFILE": "none"
  }
}
```

## Run Examples (Cabal MCP Runtime)
From repository root:

1. Quick local smoke:
```powershell
cd .\cabal-mcp-runtime
cargo test -q
```

2. Console help and pre-start gate flag:
```powershell
cd .\cabal-mcp-runtime
cargo run --release -- --help
cargo run --release -- --strict-artifacts
```

3. Unified release gate (stress + IDE contract + schema):
```powershell
cd .\cabal-mcp-runtime
powershell -ExecutionPolicy Bypass -File .\scripts\check-release-gates.ps1 -WithIntegration
```

4. Final readiness (snapshot mode, no GitHub API):
```powershell
cd .\cabal-mcp-runtime
$snapshot = ".\.cabal_runtime\branch_protection_snapshot.json"
@{ required_status_checks = @{ contexts = @("stress-sla-gate","ide-contract-gate","ide-e2e-report-schema-gate","release-summary-schema-gate","release-gate") } } | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 $snapshot
powershell -ExecutionPolicy Bypass -File .\scripts\check-final-readiness.ps1 -IdeE2EReportPath .\..\spec\contracts\ide_e2e_report.pass.json -ProtectionJsonPath $snapshot
```

5. Final readiness (GitHub mode):
```powershell
cd .\cabal-mcp-runtime
powershell -ExecutionPolicy Bypass -File .\scripts\check-final-readiness.ps1 -IdeE2EReportPath .\..\spec\docs\ide_e2e_report.json -RepoOwner "<owner>" -RepoName "<repo>" -Branch "main"
```

## CONSULT Modes: USER_TRACKING and YOLO
Before the first GA-1 run (after the concept is finalized), the user selects a CONSULT routing mode:
- `USER_TRACKING` — the user tracks progress and answers CONSULT items directly.
- `YOLO` — every CONSULT is routed to the Orchestrator.

How `YOLO` works:
1. The Orchestrator asks the user to clarify and extend cross-cutting rules.
2. These rules are recorded as additions to existing cross-cutting rules.
3. Only then is `YOLO` considered active.
4. After activation, each CONSULT is handled by the Orchestrator, who assigns a suitable subagent and enforces all cross-cutting rules.

## Mandatory Rule Review Before Work
- Before each new task/iteration, every agent and subagent must read the full current cross-cutting rules in `spec/docs/CONCEPT_MASTER.md` (section 6).
- If rules were extended, the assignee must review the complete list (base + additions) before starting work.
- In `YOLO` mode, the Orchestrator must include the full current cross-cutting rule set in every CONSULT assignment.
- Any work started without this review is a protocol violation.

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
