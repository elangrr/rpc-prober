# rpc-prober

Checks many blockchain RPC endpoints at the same time. For each endpoint it reports:
up or down, latest block height, blocks behind the best height of that chain,
latency, and the error when it is down.

Chain types:

- `cosmos`: CometBFT RPC, `GET {rpc}/status`
- `evm`: Ethereum JSON-RPC, `eth_blockNumber` (and `eth_chainId` when `chain_id` is set)
- `tm2`: Tendermint2 / Gno RPC, `GET {rpc}/status`

## Install

```sh
cargo build --release
# binary: target/release/rpc-prober
```

Needs Rust 1.88 or newer (edition 2024, `if let ... &&` chains). TLS is rustls, so OpenSSL is not needed.

## Usage

```sh
rpc-prober -c examples/indonode.toml            # table
rpc-prober -c examples/indonode.toml --json     # JSON, schema below
rpc-prober -c examples/indonode.toml --prom     # Prometheus text format
rpc-prober -c my.toml --timeout-ms 3000 --max-behind 20
```

Flags:

- `-c, --config <FILE>`: TOML config (required)
- `--timeout-ms <MS>`: time limit per endpoint; overrides `timeout_ms` in the config
- `--max-behind <N>`: more than N blocks behind the best height is `lagging` (default 50)
- `--json` / `--prom`: output format (default: table)

Exit codes:

- `0`: every chain has at least one endpoint with status `ok`
- `1`: at least one chain has no `ok` endpoint
- `2`: the config file or the flags are invalid

Sample table output (real run, 2026-10-22, some rows left out):

```
CHAIN               TYPE    ENDPOINT                                STATUS    HEIGHT  BEHIND  LATENCY  ERROR
atomone             cosmos  https://atomone-rpc.allinbits.services  ok      10924900       0   115 ms
atomone             cosmos  https://rpc-atomone.ecostake.com        ok      10924900       0    83 ms
epix                cosmos  https://rpc-epix.onenov.xyz             ok       6179536       0   316 ms
epix                cosmos  https://rpc.epix.indonode.net           down           -       -    41 ms  request failed: invalid peer certificate: certificate not valid for name "rpc...
epix-evm            evm     https://evm-epix.onenov.xyz             ok       6179535       0   312 ms
gno                 tm2     https://rpc.gno.land                    ok        996191       0   189 ms
realio              cosmos  https://rpc.realio.network              ok      20214995       0   191 ms
realio              cosmos  https://rpc.realio.indonode.net         down           -       -    44 ms  HTTP 415 Unsupported Media Type
```

The table cuts errors at 80 characters. `--json` has the full text.

## Config

```toml
timeout_ms = 5000     # optional, default 5000
concurrency = 16      # optional, default 16: endpoints checked at the same time

[[chain]]
name = "atomone"      # unique; used in every output
type = "cosmos"       # cosmos | evm | tm2
chain_id = "atomone-1" # optional; for evm write the decimal id, e.g. "1916"
rpcs = ["https://rpc.atomone.indonode.net", "https://rpc-atomone.ecostake.com"]
```

Unknown keys are an error, so a typo does not go unnoticed.
See `examples/indonode.toml` for a full file.

## Status

Checked in this order; the first match wins:

1. `down`: no height (network error, timeout, HTTP error, bad reply)
2. `wrong_chain`: `chain_id` is set and the endpoint reports another one
3. `catching_up`: the node says it is still syncing
4. `lagging`: more than `--max-behind` blocks below the best height
5. `ok`

The best height of a chain is the highest height among its endpoints that
answered and are on the right chain. A `wrong_chain` endpoint keeps the
height and chain id it reported, so you can see what answered, but gets no
`behind`: its height belongs to another chain. Set `chain_id` on every chain:
without it the prober cannot tell that an endpoint (or a cache in front of it)
answers for another network.

## JSON schema (schema_version 1)

```json
{
  "schema_version": 1,
  "healthy": true,
  "max_behind": 50,
  "unhealthy_chains": [],
  "endpoints": [
    {
      "chain": "atomone",
      "type": "cosmos",
      "endpoint": "https://rpc.atomone.indonode.net",
      "status": "ok",
      "height": 10924901,
      "behind": 0,
      "latency_ms": 48,
      "chain_id": "atomone-1",
      "error": null
    }
  ]
}
```

- `status`: `ok`, `lagging`, `catching_up`, `wrong_chain` or `down`
- `height`, `behind`, `chain_id`: `null` when the endpoint is down
- `behind`: also `null` when the status is `wrong_chain`
- `error`: `null` unless the status is `down` or `wrong_chain`
- `endpoints` is sorted by chain, then height (highest first), then endpoint.
- A renamed or removed field raises `schema_version`. New fields do not.

## Prometheus

`--prom` prints four gauges with labels `chain`, `endpoint`, `type`:

- `rpc_up`: 1 if the endpoint answered with a height of the expected chain,
  else 0. A `wrong_chain` endpoint is 0.
- `rpc_block_height`: only for endpoints with `rpc_up` 1
- `rpc_blocks_behind`: only for endpoints with `rpc_up` 1
- `rpc_latency_seconds`: time until the reply or the error

For node_exporter's textfile collector, write to a temp file and rename it.
Exit code 1 (a chain is unhealthy) still prints all metrics, so only skip the
rename on exit code 2:

```sh
rpc-prober -c my.toml --prom > rpc.prom.tmp; [ $? -le 1 ] && mv rpc.prom.tmp /path/to/textfile/rpc.prom
```

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Learning Rust with this code: see `LEARN.md`.
