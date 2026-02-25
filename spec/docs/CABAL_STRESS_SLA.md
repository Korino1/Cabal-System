# CABAL Stress SLA

## Scope
Этот документ фиксирует SLA для stress-профилей `cabal-mcp-runtime` по операциям:
- `audit.query`
- `audit.export`
- `audit.replay`

Контур применяется к тестам:
- `tests/runtime_stress.rs::stress_audit_query_export_replay_profile`
- `tests/runtime_stress.rs::stress_audit_query_export_replay_multirun_p95_p99`

## Run Command
```powershell
cargo test --test runtime_stress -- --ignored --nocapture
```

## Current Dataset Profiles
- single-run: `10_000` событий
- multi-run: `5` прогонов по `5_000` событий

## SLA Thresholds (Gate)
- `audit.query p99 < 10000 ms`
- `audit.export p99 < 10000 ms`
- `audit.replay p99 < 10000 ms`

## Latest Observed (2026-02-25)
- single-run:
  - ingest=`999 ms`
  - query=`112 ms`
  - export=`134 ms`
  - replay=`72 ms`
- multi-run:
  - query `p95/p99 = 59/59 ms`
  - export `p95/p99 = 82/82 ms`
  - replay `p95/p99 = 36/36 ms`

## Notes
- Порог выбран консервативным, как regression guardrail (а не оптимизационная цель).
- После изменения runtime I/O, proxy, audit serialization thresholds должны быть пересмотрены и подтверждены новым stress-run.
