---
id: cabal_error_codes
updated: 2026-02-24
owner: orchestrator
version: 1
---
# Cabal Runtime Error Codes (SDK Contract)

Назначение:
- единый и стабильный контракт кодов отказов для IDE/MCP клиентов;
- исключить парсинг свободного текста ошибок в клиентах;
- обеспечить совместимость при обновлении runtime.

## 1) Формат JSON-RPC ошибки
```json
{
  "jsonrpc": "2.0",
  "id": "same_as_request_or_null",
  "error": {
    "code": -32010,
    "message": "human-readable message",
    "data": {
      "cabal_code": "REVISION_MISMATCH",
      "retryable": true,
      "method": "tools/call",
      "tool": "cabal.apply_policy_bundle"
    }
  }
}
```

## 2) Стабильность и версия
- `cabal_code` считается стабильным ключом для SDK логики.
- `error.code` (JSON-RPC integer) также стабилен внутри этой версии контракта.
- Любое изменение существующего `cabal_code` требует bump `version`.
- Добавление новых `cabal_code` допустимо в минорных обновлениях без ломки существующих.

## 3) Таблица кодов (v1)
| cabal_code | rpc_code | retryable | Класс |
| --- | ---: | :---: | --- |
| `PARSE_ERROR` | -32700 | false | malformed json payload |
| `TRANSPORT_ERROR` | -32060 | false | invalid frame/header/body length |
| `UNSUPPORTED_METHOD` | -32601 | false | unsupported json-rpc method |
| `UNKNOWN_TOOL` | -32601 | false | unknown mcp tool |
| `INVALID_REQUEST` | -32602 | false | invalid/missing params |
| `REVISION_MISMATCH` | -32010 | true | optimistic lock mismatch |
| `SIGNATURE_INVALID` | -32011 | false | signature/key validation failed |
| `NONCE_REPLAY` | -32012 | false | anti-replay triggered |
| `GATE_FAIL` | -32020 | false | phase gate validation failed |
| `POLICY_DENY` | -32030 | false | action denied by policy |
| `PROXY_DENY` | -32031 | false | proxy denied request |
| `EXECUTOR_FAILURE` | -32040 | true | shell/network executor failed |
| `STORAGE_FAILURE` | -32050 | true | state/audit persistence failure |
| `IO_FAILURE` | -32051 | false | fs operation failure on target path |
| `STATE_CORRUPT` | -32052 | false | state.json malformed |
| `INTERNAL_ERROR` | -32000 | true | unclassified runtime error |

## 4) Рекомендации для IDE SDK
- Обрабатывать логику по `cabal_code`, а не по `message`.
- Использовать `retryable=true` только как сигнал, но не безусловный автоповтор.
- Для `INVALID_REQUEST`, `POLICY_DENY`, `PROXY_DENY`, `GATE_FAIL` не делать silent retry.
- Для `REVISION_MISMATCH` обновлять state/revision и повторять с новым `expected_revision`.

## 5) Совместимость
- Runtime обязан публиковать ту же таблицу через `cabal.get_error_codes`.
- При расхождении между `CABAL_ERROR_CODES.md` и runtime-tool источником истины считается runtime-tool.
