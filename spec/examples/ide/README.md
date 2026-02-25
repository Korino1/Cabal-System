# Cabal MCP IDE Adapters (Templates)

Эта папка содержит шаблоны подключения `cabal-mcp-runtime` через stdio в IDE-клиентах MCP.

## Базовые шаги
1. Соберите runtime бинарник:
```powershell
cd .\cabal-mcp-runtime
cargo build --release
```
2. Используйте шаблон нужной IDE:
- `vscode.mcp.jsonc`
- `jetbrains.mcp.jsonc`
3. Замените `command` на абсолютный путь к бинарнику `cabal-mcp-runtime`.
4. При необходимости задайте `CABAL_PROXY_SHELL_TIMEOUT_MS` через `env`.

## Важно
- Эти файлы являются шаблонами-адаптерами и не содержат логики Cabal.
- Логика, policy и gate проверяются только внутри `cabal-mcp-runtime` по MCP контракту.
- Для production рекомендуется закрепить CI gate (`cabal-mcp-runtime-stress-gate`) как required status check.
