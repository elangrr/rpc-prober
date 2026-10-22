# HANDOFF: rpc-prober (card t_15186fdf, R99)

Branch: main (local only, Rocky pushes). Last commit: see `git log -1`;
checks below were run at that commit.

## State: v1 done, ready for review (child card t_b5a12188)
- Crate builds on Rust 1.99.1 (rust-version 1.88: uses `if let ... &&` chains).
- fmt, clippy -D warnings, 34 tests (32 unit + 2 wiremock integration) all pass.
- Real run against examples/indonode.toml: exit 0, table/json/prom all fine.

## Changed this run (after the toolchain fix)
- Cargo.toml: R100 newest majors: reqwest 0.13 (feature `rustls`, was
  `rustls-tls`), toml 1.x. Both built and passed with no code change.
- src/probe/mod.rs: real bug. The client had its own timeout equal to the probe
  timeout, so reqwest raced tokio and slow endpoints said "request failed:
  operation timed out" instead of "timed out after N ms". The client now has no
  timeout; `probe_one` holds the only one (covers both EVM requests).
- src/probe/mod.rs `network_error`: reports only the root cause
  ("invalid peer certificate: ...", "Connection refused") not the reqwest wrappers.
- src/report/table.rs: ERROR cut at 80 chars (TLS errors were ~600 chars);
  --json keeps the full text. Test: long_errors_are_cut_to_80_chars_in_the_table.
- src/model.rs: clippy collapsible_if, now a let chain.
- tm2 fixture height is 995854; test expected 995852 (typo), fixed.
- README.md, LEARN.md written. LEARN.md excerpts verified verbatim against src.

## Test
```
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cargo run -- -c examples/indonode.toml; echo $?   # also --json, --prom
```

## Open / fragile
- Indonode RPCs: epix, safrochain, safrochain-t serve a TLS cert without their
  hostname; realio answers HTTP 415 (gRPC backend on the RPC host). Reported as
  down; other endpoints keep those chains healthy. Infra issue, not this code.
- EVM `catching_up` is always false (no eth_syncing in v1; LEARN.md exercise 2).
- Live public RPC lists in examples/ will drift over time.
