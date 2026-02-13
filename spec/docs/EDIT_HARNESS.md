---
id: edit_harness
updated: 2026-02-13
owner: orchestrator
status: draft
---
# Edit Harness Protocol

Назначение:
- Снизить ошибки правок кода/документов за счёт детерминированного цикла `read -> verify hash -> apply`.
- Исключить «слепые» массовые замены и тихую потерю контекста (`stale edit`).
- Сделать правки воспроизводимыми и проверяемыми в QA.

## 1) Область действия
- Протокол обязателен для правок кода и документации в рамках проекта.
- Применяется к ролям, которые вносят изменения в файлы (исполнители и фиксеры).
- QA проверяет соблюдение протокола как отдельный гейт.

## 2) Базовые инварианты
- Путь к файлу указывается как repo-relative (от корня репозитория).
- Перед любой правкой сначала читается диапазон и фиксируется `range_hash`.
- Применение правки выполняется только при совпадении `expected_hash`.
- При несовпадении хеша правка отклоняется: нужно перечитать диапазон и пересобрать правку.

## 3) Канонизация диапазона и хеш
- Кодировка: UTF-8.
- Текст диапазона: объединение строк через `\n`.
- Если диапазон непустой, в конец добавляется завершающий `\n`.
- Хеш: SHA-256 от канонизированного текста диапазона.

## 4) Операция чтения (read_range)
Используй:
- `scripts/harness_read.ps1`

Пример:
```powershell
powershell -File scripts/harness_read.ps1 `
  -Path spec/docs/CONCEPT_MASTER.md `
  -StartLine 61 `
  -EndLine 70 `
  -AsJson
```

Результат чтения должен содержать:
- `path`
- `start_line`
- `end_line`
- `total_lines`
- `range_hash`
- `range_lines`

## 5) Операции правки (apply)
Используй:
- `scripts/harness_apply.ps1`

Вход: JSON-спецификация с массивом `operations`.

Поддерживаемые операции:
- `replace_range`
  - `path`, `start_line`, `end_line`, `expected_hash`, `new_text` (или `new_lines`)
- `insert_after`
  - `path`, `after_line`, `expected_hash`, `new_text` (или `new_lines`)
- `delete_range`
  - `path`, `start_line`, `end_line`, `expected_hash`

Пример spec:
```json
{
  "version": "1",
  "operations": [
    {
      "op": "replace_range",
      "path": "README.md",
      "start_line": 10,
      "end_line": 12,
      "expected_hash": "sha256_hex_here",
      "new_text": "новый текст\nв две строки"
    }
  ]
}
```

Пример применения:
```powershell
powershell -File scripts/harness_apply.ps1 -SpecPath .memory/harness_ops.json
```

## 6) Поведение при конфликте
- Если `expected_hash` не совпадает с текущим диапазоном, операция завершается ошибкой.
- После ошибки обязательно:
  1) повторно выполнить `read_range`,
  2) пересчитать хеш,
  3) пересобрать правку.
- Если конфликт не снимается, создать `CONSULT`.

## 7) Требования к QA
QA должен проверить:
- Есть evidence использования `harness_read.ps1` и `harness_apply.ps1` (или эквивалентной процедуры).
- Нет прямых «слепых» замен без `expected_hash`.
- Для конфликтов `stale edit` зафиксирована корректная обработка (re-read/re-apply или CONSULT).
