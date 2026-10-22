//! CometBFT / Tendermint chains: GET {rpc}/status.

use reqwest::Client;
use serde::Deserialize;

use super::{HeightSource, ProbeError, get_text, join_url, parse_decimal_height, parse_envelope};
use crate::model::Reading;

/// Probe for Cosmos SDK chains (CometBFT RPC).
pub struct Cosmos;

/// The parts of the `/status` result we read. serde ignores the rest.
#[derive(Debug, Deserialize)]
struct StatusResult {
    node_info: NodeInfo,
    sync_info: SyncInfo,
}

#[derive(Debug, Deserialize)]
struct NodeInfo {
    /// The chain id, for example "atomone-1".
    network: String,
}

#[derive(Debug, Deserialize)]
struct SyncInfo {
    /// CometBFT sends the height as a string: "10924691".
    latest_block_height: String,
    catching_up: bool,
}

impl HeightSource for Cosmos {
    async fn fetch(&self, client: &Client, rpc: &str) -> Result<Reading, ProbeError> {
        let body = get_text(client, &join_url(rpc, "status")).await?;
        parse_status(&body)
    }
}

/// Turn a `/status` reply body into a `Reading`.
pub(crate) fn parse_status(body: &str) -> Result<Reading, ProbeError> {
    let status: StatusResult = parse_envelope(body)?;
    Ok(Reading {
        height: parse_decimal_height(&status.sync_info.latest_block_height)?,
        chain_id: Some(status.node_info.network),
        catching_up: status.sync_info.catching_up,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_atomone_status_reply_parses() {
        let body = include_str!("../../tests/fixtures/cosmos_status.json");
        let reading = parse_status(body).expect("fixture parses");
        assert_eq!(reading.height, 10_924_691);
        assert_eq!(reading.chain_id.as_deref(), Some("atomone-1"));
        assert!(!reading.catching_up);
    }

    #[test]
    fn rpc_error_reply_becomes_rpc_error() {
        let body = r#"{"jsonrpc":"2.0","id":-1,"error":{"code":-32603,"message":"Internal error","data":"x"}}"#;
        let err = parse_status(body).expect_err("error reply");
        assert_eq!(err.to_string(), "RPC error -32603: Internal error");
    }

    #[test]
    fn html_page_is_a_parse_error() {
        let err = parse_status("<html>502 Bad Gateway</html>").expect_err("not json");
        assert!(matches!(err, ProbeError::Parse(_)));
    }
}
