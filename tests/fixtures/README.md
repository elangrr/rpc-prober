# Test fixtures

Real replies, fetched on 2026-10-22 and saved unchanged.

| File | Source | Request |
|---|---|---|
| `cosmos_status.json` | `https://rpc.atomone.indonode.net` (CometBFT 0.38.22) | `GET /status` |
| `tm2_status.json` | `https://rpc.gno.land` (gno.land `gnoland-1`, tm2 `v1.0.0-rc.0`) | `GET /status` |
| `evm_block_number.json` | `https://evmrpc.epix.zone` | `POST eth_blockNumber` |
| `evm_chain_id.json` | `https://evmrpc.epix.zone` | `POST eth_chainId` |
| `evm_error.json` | `https://evmrpc.epix.zone` | `POST` with an unknown method |

## tm2 (Gno) `/status` compared with CometBFT

Both wrap the answer in JSON-RPC 2.0 and both have
`result.node_info.network` and `result.sync_info.latest_block_height`,
so the paths the prober reads are the same. The differences:

| Field | CometBFT | tm2 |
|---|---|---|
| `id` | number `-1` | empty string `""` |
| `node_info.protocol_version` | object of strings | missing; tm2 has `version_set`, a list of `{Name, Version, Optional}` |
| `node_info.id`, `listen_addr` | hex id, `tcp://` address | missing; tm2 has `net_address` as `g1...@host:port` |
| `node_info.channels` | hex string | base64 string |
| `sync_info.*_hash` | uppercase hex | base64 |
| `sync_info.earliest_*` | present | missing |
| `validator_info.pub_key` | `{"type": "tendermint/PubKeyEd25519"}` | `{"@type": "/tm.PubKeyEd25519"}` |
| `validator_info.address` | hex | bech32 `g1...` |
| `latest_block_height` | string | string in this sample |

Amino JSON in tm2 writes 64-bit integers as strings. The prober also
accepts a plain number, so a proxy or a later tm2 version that sends one
still works. `src/probe/tm2.rs` has a unit test for the number form.
