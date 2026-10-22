//! Tendermint2 chains (Gno): GET {rpc}/status.
//!
//! The reply looks like CometBFT's, with differences documented in
//! tests/fixtures/README.md. The one that matters here: we accept the
//! height as a string ("995854") or as a plain number (995854).

use reqwest::Client;
use serde::Deserialize;

use super::{HeightSource, ProbeError, get_text, join_url, parse_decimal_height, parse_envelope};
use crate::model::Reading;

/// Probe for Tendermint2 / Gno chains.
pub struct Tm2;

#[derive(Debug, Deserialize)]
struct StatusResult {
    node_info: NodeInfo,
    sync_info: SyncInfo,
}

#[derive(Debug, Deserialize)]
struct NodeInfo {
    network: String,
}

#[derive(Debug, Deserialize)]
struct SyncInfo {
    latest_block_height: Height,
    #[serde(default)]
    catching_up: bool,
}

/// A height that may arrive as JSON text or as a JSON number.
/// `untagged` tells serde to try each variant in order.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Height {
    Number(u64),
    Text(String),
}

impl Height {
    fn to_u64(&self) -> Result<u64, ProbeError> {
        match self {
            Height::Number(n) => Ok(*n),
            Height::Text(text) => parse_decimal_height(text),
        }
    }
}

impl HeightSource for Tm2 {
    async fn fetch(&self, client: &Client, rpc: &str) -> Result<Reading, ProbeError> {
        let body = get_text(client, &join_url(rpc, "status")).await?;
        parse_status(&body)
    }
}

/// Turn a tm2 `/status` reply body into a `Reading`.
pub(crate) fn parse_status(body: &str) -> Result<Reading, ProbeError> {
    let status: StatusResult = parse_envelope(body)?;
    Ok(Reading {
        height: status.sync_info.latest_block_height.to_u64()?,
        chain_id: Some(status.node_info.network),
        catching_up: status.sync_info.catching_up,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_gno_status_reply_parses() {
        let body = include_str!("../../tests/fixtures/tm2_status.json");
        let reading = parse_status(body).expect("fixture parses");
        assert_eq!(reading.height, 995_854);
        assert_eq!(reading.chain_id.as_deref(), Some("gnoland-1"));
        assert!(!reading.catching_up);
    }

    #[test]
    fn height_as_json_number_parses() {
        let body = r#"{"jsonrpc":"2.0","id":"","result":{"node_info":{"network":"dev"},"sync_info":{"latest_block_height":1234,"catching_up":true}}}"#;
        let reading = parse_status(body).expect("number height parses");
        assert_eq!(reading.height, 1234);
        assert!(reading.catching_up);
    }

    #[test]
    fn non_numeric_height_text_is_a_parse_error() {
        let body = r#"{"jsonrpc":"2.0","id":"","result":{"node_info":{"network":"dev"},"sync_info":{"latest_block_height":"abc"}}}"#;
        let err = parse_status(body).expect_err("bad height");
        assert_eq!(
            err.to_string(),
            "unexpected reply: height \"abc\" is not a whole number"
        );
    }
}
