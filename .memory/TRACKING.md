---
id: tracking
updated: 2026-02-11
owner: techlead
---
# Система отслеживания прогресса и восстановления

## 1) Цель
Сохранить прогресс на всех уровнях (эпик → фича → пользовательский сценарий → задача) и обеспечить восстановление после обрыва связи без потерь контекста.

## 2) Источники истины (файлы учета)
- `.memory\TASKS.md` — канбан EP/FEAT/US/T и зависимые ветки.
- `.memory\PHASES\<PHASE>\WORKLOG.md` — журнал шагов и решений по конкретной фазе.
- `.memory\PROGRESS.md` — короткий прогресс (одна строка на событие).
- `.memory\STATE.md` — текущий снимок (точка продолжения).
- `.memory\PHASES\<PHASE>\STATE_HISTORY.md` — история снимков фазы (append-only).
- `.memory\DEFECTS.md` — передача дефектов между debuger → fixer.
- `.memory\ASKS.md` — история запросов и обратной связи.
- `.memory\DECISIONS.md` + `spec/adr/*` — архитектурные решения.
- `spec\contracts\VERSION.json` — версия контрактов/изменений.
- `.memory\GLOBAL_INDEX.md` — реестр фаз и активная фаза.

## 3) Правила обновления
- TASKS: глобальный `TASKS.md` содержит EP/FEAT/US; фазовые `PHASES/<Phase>/TASKS.md` содержат только T-уровень и обязаны ссылаться на US/FEAT из глобального TASKS.
- WORKLOG: фиксируй каждый завершенный шаг (что сделано, где, почему, что дальше) внутри активной фазы.
- STATE: обновляй при смене контекста, перед паузой, после консультаций и перед checkpoint.
- STATE_HISTORY: добавляй снимок при каждом обновлении STATE в журнал активной фазы.
- PROGRESS: одна строка после checkpoint или значимого события.

- TASKS (fix): каждая фикса-задача обязана содержать DEFECT ID (пример: DEF-YYYYMMDD-###).
- ASKS/DECISIONS/CONTRACTS: обновляй при появлении новых запросов, решений и изменений API.

## 4) Протокол снимка (STATE)
- Формат ID: `SNAP-YYYYMMDD-HHMM`.
- Снимок обязателен: перед паузой, перед запросом `CONSULT`, после закрытия ключевого шага.
- В снимке должны быть: активная задача, следующий шаг, блокеры, измененные файлы, тесты.

## 4.1) Фазовое разделение (обязательное)
- Все детальные журналы ведутся внутри `.memory/PHASES/<PHASE>/`.
- В корне `.memory` остаются только: `STATE.md`, `TASKS.md`, `GLOBAL_INDEX.md`, `PROGRESS.md`, `ASKS.md`, `DECISIONS.md`, `DEFECTS.md`.
- Оркестратор читает только: `STATE.md` → `GLOBAL_INDEX.md` → `PHASES/<Active>/INDEX.md` (+ файлы фазы по необходимости).
- Источник истины для Active Phase: `.memory/GLOBAL_INDEX.md`. `STATE.md` хранит снимок и может отставать.
- `.memory/WORKLOG.md` удалён; использовать только `.memory/PHASES/<Active>/WORKLOG.md`.

## 4.2) Завершение фазы (Exit Criteria)
- Фаза считается закрытой только если:
  1) Заполнен `.memory/PHASES/<Phase>/DIGEST.md`.
  2) В `.memory/PHASES/<Phase>/INDEX.md` заполнены Exit Criteria и Status = done.
  3) В `.memory/GLOBAL_INDEX.md` обновлён статус фазы.
  4) Для C-0: подтверждена синхронизация канона (`spec/docs/CONCEPT_MATH_PROOF.md` + `spec/docs/CONCEPT_MASTER.md`), закрыты задачи C-0.1..C-0.6.
  5) Для C-0: если после последней C-0.6 Пользователь менял вариант/параметры, C-0.2..C-0.6 выполнены повторно и закрыты для последнего цикла.



## 4.4) Уплотнение контекста (обязательно при длинной работе)
- WORKLOG.md в фазе используется как индекс.
- Детальные записи ведутся в WORKLOG-YYYYMMDD-HHMM.md.
- Оркестратор читает только: STATE.md → GLOBAL_INDEX.md → PHASES/<Active>/INDEX.md → PHASES/<Active>/DIGEST.md.
- WORKLOG-*.md читать только по явной необходимости.
- При превышении 200 строк в WORKLOG-*.md — вынести итог в DIGEST.md.
- Если активный WORKLOG ведётся более 2 дней без обновления DIGEST — обновить DIGEST.md.
 
## 4.4.1) Хранение описаний функций (GA-5)
- Иерархия хранения: **Блок → Метод → Функции**.
- Методы размещаются в папках соответствующих блоков.
- Описания функций размещаются в папках соответствующих методов.
- Это правило предотвращает разрастание канонических файлов.

## 4.5) Компактность LOGIC_PROTOCOL
- `LOGIC_PROTOCOL.md` хранит только канон и итоговые своды по этапам.
- Детали и расчёты писать в `PHASES/<Active>/WORKLOG*.md` или `spec/docs/*`.
- Восстановление при форсмажоре: `STATE.md` → `GLOBAL_INDEX.md` → `PHASES/<Active>/DIGEST.md` → (при необходимости) `PHASES/<Active>/WORKLOG*.md`.

## 4.3) Смена активной фазы
1) Пройти чеклист `spec/docs/PHASE_GATE.md` (Exit/Entry).
2) Убедиться, что Exit Criteria выполнены и Digest заполнен.
3) Обновить статус текущей фазы на `done` в `.memory/GLOBAL_INDEX.md`.
4) Выставить `Active Phase` на следующую фазу в `.memory/GLOBAL_INDEX.md`.
5) Обновить `.memory/PHASES/<Next>/INDEX.md` (Status = in_progress).
6) Создать/обновить `.memory/PHASES/<Next>/DIGEST.md` (минимум: `- Status: in_progress` + краткий Scope/Open questions).

## 5) Восстановление после обрыва
1) Прочитать `STATE.md` и выполнить шаги раздела «Next Steps».
2) Сверить `PROGRESS.md` и `PHASES/<Active>/WORKLOG.md`.
3) Открыть `TASKS.md` и отметить актуальный статус задач (для фиксов — с DEFECT ID).
4) Проверить `DEFECTS.md` и `PHASES/<Active>/STATE_HISTORY.md` на последние обновления.
5) Проверить `ASKS.md` на незакрытые запросы.
6) Учитывать `DECISIONS.md` и `VERSION.json` при продолжении.

## 6) Шаблоны

### 6.1) Снимок (STATE)
```
SNAPSHOT:
- ID: SNAP-YYYYMMDD-HHMM
- Timestamp: 2025-12-26 12:00
- Active: T BASE.IM.2.3 (кратко)
- Phase: IM
- Status: [~]
- Next: 1) ... 2) ... 3) ...
- Blockers: ...
- Questions: ...
- Files touched: ...
- Tests: ...
- Commands: ...
- Notes: ...
```

### 6.2) Запись в WORKLOG
```
- YYYY-MM-DD HH:MM: Кратко что сделано (ID задач). Файлы: ... Следующий шаг: ...
```

### 6.3) Запись в PROGRESS
```
YYYY-MM-DD: <BASE>.<PHASE> — краткий результат / checkpoint
```

### 6.4) Матрица зависимостей
```
| ID | DependsOn | Status | Notes |
| -- | --------- | ------ | ----- |
```

### 6.5) Мини-чеклист готовности (DoD)
- Контракты обновлены (если менялось поведение)
- Тесты обновлены/пройдены
- WORKLOG/PROGRESS синхронизированы
- STATE обновлен

### 6.6) Карточка дефекта (handoff)
```
## DEF-YYYYMMDD-###
- Status: New | Triaged | InFix | Fixed | Verified
- Source: debuger
- Related Tasks: T BASE...
- Summary:
- Symptoms:
- Repro:
- Root Cause:
- Scope/Impact:
- Suspected Files:
- Fix Plan:
- Tests:
- Risks:
- Notes:
```

## 7) Checkpoint (единый снимок)
- Запусти scripts/checkpoint.ps1 перед паузой и после значимых шагов.
- Скрипт обновляет: STATE.md, PHASES/<PhaseId>/STATE_HISTORY.md, PHASES/<PhaseId>/WORKLOG.md и (опционально) PROGRESS.md.
- PhaseId обязателен (единый журнал — только в `.memory/PHASES/<PhaseId>/`).

## 8) Проверка правил
- Запусти `scripts/validate_defect_ids.ps1` перед checkpoint для фикса-задач.
- Для закрытия фазы запусти `scripts/validate_phase_exit.ps1 -PhaseId <PHASE>`.
- Для контроля разрастания логов запусти `scripts/validate_worklog_size.ps1 -PhaseId <PHASE>`.

## 8.1) Быстрые операции
- Создание нового лог-энтри: `scripts/new_worklog_entry.ps1 -PhaseId <PHASE> -Title "..."`.
- Переключение фаз: `scripts/switch_phase.ps1 -Current <PHASE> -Next <PHASE>`.

## 9) Протокол передачи дефектов (Debuger → Fixer)
1) Debuger создает запись в `DEFECTS.md`, заполняет поля, присваивает ID.
2) Debuger фиксирует ссылку на DEFECT ID в `PHASES/<Active>/WORKLOG.md` и при необходимости в `STATE.md`.
3) Fixer принимает дефект: меняет статус на `InFix`, связывает с задачами в `TASKS.md` (с DEFECT ID).
4) После исправления Fixer выставляет `Fixed`, добавляет тесты/проверки и запись в `PROGRESS.md`.
5) После проверки — статус `Verified`; дефект считается закрытым.
