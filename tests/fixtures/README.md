# Test fixtures

Real replies, saved unchanged (fetched 2026-10-22 unless the table says otherwise).

| File | Source | Request |
|---|---|---|
| `cosmos_status.json` | `https://rpc.atomone.indonode.net` (CometBFT 0.38.22) | `GET /status` |
| `tm2_status.json` | `https://rpc.gno.land` (gno.land `gnoland-1`, tm2 `v1.0.0-rc.0`) | `GET /status` |
| `evm_block_number.json` | `https://evmrpc.epix.zone` | `POST eth_blockNumber` |
| `evm_chain_id.json` | `https://evmrpc.epix.zone` | `POST eth_chainId` |
| `evm_error.json` | `https://evmrpc.epix.zone` | `POST` with an unknown method |
| `lumen_status.json` | `https://rpc.lumen.indonode.net` (CometBFT 0.38.19), fetched 2026-10-23 | `GET /status` |
| `lumen_status_limonata_cache.json` | not a capture, see below | `GET /status` |

`lumen_status_limonata_cache.json` reproduces a production bug: a shared nginx
cache made `rpc.lumen.indonode.net` answer with another chain's `/status`
(network `limonata_10777-1`, height 4,923,377). It is `lumen_status.json` with
only those two values changed:

```sh
sed -e 's/"network":"lumen"/"network":"limonata_10777-1"/' \
    -e 's/"latest_block_height":"5302462"/"latest_block_height":"4923377"/' \
    lumen_status.json > lumen_status_limonata_cache.json
```

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
