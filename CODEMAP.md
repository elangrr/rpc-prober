# CODEMAP

- `src/main.rs`: CLI (clap). Flags, exit codes 0/1/2, picks table/json/prom.
- `src/lib.rs`: declares the modules so tests/ can use them.
- `src/config.rs`: TOML config: parse, defaults, validation, `targets()`.
- `src/model.rs`: shared types (ChainType, Target, Reading, Outcome, Status, EndpointReport) + health math (`evaluate`, `blocks_behind`, `status_for`, `unhealthy_chains`).
- `src/probe/mod.rs`: `HeightSource` trait, `ProbeError`, concurrency (`probe_all`: tokio::spawn + Semaphore), timeout (`probe_one`), HTTP + JSON-RPC envelope helpers.
- `src/probe/{cosmos,evm,tm2}.rs`: one `HeightSource` impl per chain type + reply parsers.
- `src/report/{table,json,prom}.rs`: output formats.
- `tests/probe_mock.rs`: integration test with wiremock (up/down/timeout/refused/wrong chain, concurrency cap).
- `tests/fixtures/`: real replies + README.md documenting the tm2 shape.
- `examples/indonode.toml`: Indonode RPCs + public reference RPCs.
- `README.md`: usage, flags, exit codes, config, JSON schema, Prometheus metrics.
- `LEARN.md`: Rust learning guide, file by file, with excerpts from the real code.
