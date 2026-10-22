//! EVM chains: POST JSON-RPC eth_blockNumber (and eth_chainId).

use reqwest::Client;
use serde_json::json;

use super::{HeightSource, ProbeError, parse_envelope, post_json};
use crate::model::Reading;

/// Probe for EVM JSON-RPC endpoints.
pub struct Evm {
    /// Also call eth_chainId, so a wrong chain can be reported.
    pub check_chain_id: bool,
}

impl HeightSource for Evm {
    async fn fetch(&self, client: &Client, rpc: &str) -> Result<Reading, ProbeError> {
        let height = call_hex(client, rpc, "eth_blockNumber");
        if !self.check_chain_id {
            return Ok(reading(height.await?, None));
        }
        // Send both requests at the same time and wait for both.
        let chain_id = call_hex(client, rpc, "eth_chainId");
        let (height, chain_id) = tokio::join!(height, chain_id);
        Ok(reading(height?, Some(chain_id?.to_string())))
    }
}

fn reading(height: u64, chain_id: Option<String>) -> Reading {
    Reading {
        height,
        chain_id,
        // EVM JSON-RPC has eth_syncing, but v1 does not call it.
        catching_up: false,
    }
}

/// Call a JSON-RPC method that returns a hex quantity like "0x5e4a12".
async fn call_hex(client: &Client, rpc: &str, method: &str) -> Result<u64, ProbeError> {
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": []});
    let body = post_json(client, rpc, &request).await?;
    parse_hex_reply(&body)
}

/// Parse a reply body whose `result` is a hex string.
pub(crate) fn parse_hex_reply(body: &str) -> Result<u64, ProbeError> {
    let hex: String = parse_envelope(body)?;
    parse_hex_u64(&hex)
}

/// "0x5e4a12" -> 6179346. The "0x" prefix is required, as in JSON-RPC.
pub fn parse_hex_u64(text: &str) -> Result<u64, ProbeError> {
    let digits = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .ok_or_else(|| ProbeError::Parse(format!("{text:?} is not a 0x hex number")))?;
    // from_str_radix accepts a leading "+", so check the digits first.
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ProbeError::Parse(format!(
            "{text:?} is not a 0x hex number"
        )));
    }
    u64::from_str_radix(digits, 16)
        .map_err(|_| ProbeError::Parse(format!("{text:?} is not a 0x hex number")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parsing() {
        assert_eq!(parse_hex_u64("0x0").ok(), Some(0));
        assert_eq!(parse_hex_u64("0x77c").ok(), Some(1916));
        assert_eq!(parse_hex_u64("0X1F").ok(), Some(31));
        assert_eq!(parse_hex_u64("0xffffffffffffffff").ok(), Some(u64::MAX));
        assert!(parse_hex_u64("0x10000000000000000").is_err(), "overflow");
        assert!(parse_hex_u64("5e4a12").is_err(), "missing 0x");
        assert!(parse_hex_u64("0x").is_err(), "no digits");
        assert!(parse_hex_u64("0xzz").is_err(), "not hex");
        assert!(parse_hex_u64("0x+1").is_err(), "sign");
    }

    #[test]
    fn real_epix_block_number_reply_parses() {
        let body = include_str!("../../tests/fixtures/evm_block_number.json");
        assert_eq!(parse_hex_reply(body).ok(), Some(6_179_346));
    }

    #[test]
    fn real_epix_chain_id_reply_parses() {
        let body = include_str!("../../tests/fixtures/evm_chain_id.json");
        assert_eq!(parse_hex_reply(body).ok(), Some(1916));
    }

    #[test]
    fn real_error_reply_becomes_rpc_error() {
        let body = include_str!("../../tests/fixtures/evm_error.json");
        let err = parse_hex_reply(body).expect_err("error reply");
        assert_eq!(
            err.to_string(),
            "RPC error -32601: the method nope does not exist/is not available"
        );
    }
}
