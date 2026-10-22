//! Stable JSON output for scripts. See README.md for the schema.

use serde::Serialize;

use crate::model::{EndpointReport, unhealthy_chains};

/// Bump this when a field is renamed or removed. Adding a field keeps it.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Serialize)]
struct JsonReport<'a> {
    schema_version: u32,
    healthy: bool,
    max_behind: u64,
    unhealthy_chains: Vec<String>,
    endpoints: &'a [EndpointReport],
}

/// Render the full report as pretty-printed JSON.
pub fn render(reports: &[EndpointReport], max_behind: u64) -> serde_json::Result<String> {
    let unhealthy = unhealthy_chains(reports);
    let report = JsonReport {
        schema_version: SCHEMA_VERSION,
        healthy: unhealthy.is_empty(),
        max_behind,
        unhealthy_chains: unhealthy,
        endpoints: reports,
    };
    serde_json::to_string_pretty(&report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ChainType, Status};

    #[test]
    fn json_has_the_documented_fields() {
        let rows = vec![EndpointReport {
            chain: "epix-evm".into(),
            chain_type: ChainType::Evm,
            endpoint: "https://evm".into(),
            status: Status::Down,
            height: None,
            behind: None,
            latency_ms: 12,
            chain_id: None,
            error: Some("HTTP 502 Bad Gateway".into()),
        }];
        let text = render(&rows, 50).expect("serializes");
        let value: serde_json::Value = serde_json::from_str(&text).expect("valid json");
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["healthy"], false);
        assert_eq!(value["max_behind"], 50);
        assert_eq!(value["unhealthy_chains"][0], "epix-evm");
        let row = &value["endpoints"][0];
        assert_eq!(row["chain"], "epix-evm");
        assert_eq!(row["type"], "evm");
        assert_eq!(row["endpoint"], "https://evm");
        assert_eq!(row["status"], "down");
        assert!(row["height"].is_null());
        assert!(row["behind"].is_null());
        assert_eq!(row["latency_ms"], 12);
        assert!(row["chain_id"].is_null());
        assert_eq!(row["error"], "HTTP 502 Bad Gateway");
    }
}
