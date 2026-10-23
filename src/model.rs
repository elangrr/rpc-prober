//! The data types every other module shares, plus the health rules.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::probe::ProbeError;

/// Which kind of RPC an endpoint speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChainType {
    /// CometBFT / Tendermint RPC: GET /status.
    Cosmos,
    /// Ethereum JSON-RPC: POST eth_blockNumber.
    Evm,
    /// Tendermint2 (Gno) RPC: GET /status, a slightly different reply.
    Tm2,
}

impl ChainType {
    pub fn as_str(self) -> &'static str {
        match self {
            ChainType::Cosmos => "cosmos",
            ChainType::Evm => "evm",
            ChainType::Tm2 => "tm2",
        }
    }
}

/// One endpoint to check.
#[derive(Debug, Clone)]
pub struct Target {
    pub chain: String,
    pub chain_type: ChainType,
    pub expected_chain_id: Option<String>,
    pub url: String,
}

/// What a healthy reply told us.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub height: u64,
    /// The chain id the endpoint reported, if it reports one.
    pub chain_id: Option<String>,
    pub catching_up: bool,
}

/// The raw result of checking one endpoint, before we compare endpoints.
#[derive(Debug)]
pub struct Outcome {
    pub target: Target,
    pub latency: Duration,
    pub result: Result<Reading, ProbeError>,
}

/// The verdict for one endpoint. Only `Ok` counts as healthy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    Lagging,
    CatchingUp,
    WrongChain,
    Down,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Lagging => "lagging",
            Status::CatchingUp => "catching_up",
            Status::WrongChain => "wrong_chain",
            Status::Down => "down",
        }
    }
}

/// One row of the final report. This is also the JSON shape of one endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct EndpointReport {
    pub chain: String,
    #[serde(rename = "type")]
    pub chain_type: ChainType,
    pub endpoint: String,
    pub status: Status,
    pub height: Option<u64>,
    pub behind: Option<u64>,
    pub latency_ms: u64,
    pub chain_id: Option<String>,
    pub error: Option<String>,
}

/// Turn raw outcomes into report rows.
///
/// The best height of a chain is the highest height among its endpoints that
/// answered and are on the expected chain. Each endpoint is compared to it.
/// Rows come back sorted by chain, then height (highest first), then endpoint.
pub fn evaluate(outcomes: Vec<Outcome>, max_behind: u64) -> Vec<EndpointReport> {
    let mut best: BTreeMap<String, u64> = BTreeMap::new();
    for outcome in &outcomes {
        if let Ok(reading) = &outcome.result
            && chain_id_matches(&outcome.target, reading)
        {
            let entry = best.entry(outcome.target.chain.clone()).or_insert(0);
            *entry = (*entry).max(reading.height);
        }
    }

    let mut reports: Vec<EndpointReport> = outcomes
        .into_iter()
        .map(|outcome| {
            let best_height = best.get(&outcome.target.chain).copied();
            to_report(outcome, best_height, max_behind)
        })
        .collect();

    reports.sort_by(|a, b| {
        a.chain
            .cmp(&b.chain)
            .then(b.height.cmp(&a.height))
            .then(a.endpoint.cmp(&b.endpoint))
    });
    reports
}

fn to_report(outcome: Outcome, best_height: Option<u64>, max_behind: u64) -> EndpointReport {
    let Outcome {
        target,
        latency,
        result,
    } = outcome;
    let latency_ms = u64::try_from(latency.as_millis()).unwrap_or(u64::MAX);

    let mut report = EndpointReport {
        chain: target.chain.clone(),
        chain_type: target.chain_type,
        endpoint: target.url.clone(),
        status: Status::Down,
        height: None,
        behind: None,
        latency_ms,
        chain_id: None,
        error: None,
    };

    match result {
        Err(err) => report.error = Some(err.to_string()),
        Ok(reading) => {
            let behind = blocks_behind(best_height, reading.height);
            report.status = status_for(&target, &reading, behind, max_behind);
            report.behind = Some(behind);
            if report.status == Status::WrongChain {
                report.error = Some(format!(
                    "chain id is {}, expected {}",
                    reading.chain_id.as_deref().unwrap_or("unknown"),
                    target.expected_chain_id.as_deref().unwrap_or("unknown"),
                ));
                // The height belongs to another chain, so "blocks behind" means nothing.
                report.behind = None;
            }
            report.height = Some(reading.height);
            report.chain_id = reading.chain_id;
        }
    }
    report
}

/// How many blocks `height` is below the best height. Never negative.
pub fn blocks_behind(best_height: Option<u64>, height: u64) -> u64 {
    best_height.unwrap_or(height).saturating_sub(height)
}

/// Pick the status of an endpoint that answered. Earlier checks win.
pub fn status_for(target: &Target, reading: &Reading, behind: u64, max_behind: u64) -> Status {
    if !chain_id_matches(target, reading) {
        Status::WrongChain
    } else if reading.catching_up {
        Status::CatchingUp
    } else if behind > max_behind {
        Status::Lagging
    } else {
        Status::Ok
    }
}

/// True when no chain id is expected, or the endpoint reported the expected one.
fn chain_id_matches(target: &Target, reading: &Reading) -> bool {
    match (&target.expected_chain_id, &reading.chain_id) {
        (Some(expected), Some(actual)) => expected == actual,
        _ => true,
    }
}

/// Names of chains that have no endpoint with status `ok`, sorted.
pub fn unhealthy_chains(reports: &[EndpointReport]) -> Vec<String> {
    let mut healthy: BTreeMap<&str, bool> = BTreeMap::new();
    for report in reports {
        let entry = healthy.entry(report.chain.as_str()).or_insert(false);
        *entry = *entry || report.status == Status::Ok;
    }
    healthy
        .into_iter()
        .filter(|(_, ok)| !ok)
        .map(|(chain, _)| chain.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(chain: &str, url: &str, expected: Option<&str>) -> Target {
        Target {
            chain: chain.to_string(),
            chain_type: ChainType::Cosmos,
            expected_chain_id: expected.map(str::to_string),
            url: url.to_string(),
        }
    }

    fn up(chain: &str, url: &str, height: u64) -> Outcome {
        Outcome {
            target: target(chain, url, Some("chain-1")),
            latency: Duration::from_millis(40),
            result: Ok(Reading {
                height,
                chain_id: Some("chain-1".to_string()),
                catching_up: false,
            }),
        }
    }

    fn down(chain: &str, url: &str) -> Outcome {
        Outcome {
            target: target(chain, url, None),
            latency: Duration::from_millis(5000),
            result: Err(ProbeError::Timeout(5000)),
        }
    }

    #[test]
    fn behind_is_best_minus_height_and_never_negative() {
        assert_eq!(blocks_behind(Some(1000), 990), 10);
        assert_eq!(blocks_behind(Some(1000), 1000), 0);
        assert_eq!(blocks_behind(Some(1000), 1005), 0);
        assert_eq!(blocks_behind(None, 1000), 0);
    }

    #[test]
    fn endpoint_more_than_max_behind_is_lagging_and_exactly_max_is_ok() {
        let reports = evaluate(
            vec![
                up("a", "https://best", 1000),
                up("a", "https://edge", 950),
                up("a", "https://slow", 949),
            ],
            50,
        );
        let by_url = |url: &str| reports.iter().find(|r| r.endpoint == url).cloned();
        assert_eq!(by_url("https://best").map(|r| r.status), Some(Status::Ok));
        assert_eq!(by_url("https://edge").map(|r| r.status), Some(Status::Ok));
        let slow = by_url("https://slow").expect("slow row");
        assert_eq!(slow.status, Status::Lagging);
        assert_eq!(slow.behind, Some(51));
    }

    #[test]
    fn wrong_chain_endpoint_does_not_set_the_best_height() {
        let mut other = up("a", "https://other", 9_999_999);
        if let Ok(reading) = &mut other.result {
            reading.chain_id = Some("other-1".to_string());
        }
        let reports = evaluate(vec![up("a", "https://good", 1000), other], 50);
        let good = reports.iter().find(|r| r.endpoint == "https://good");
        assert_eq!(good.map(|r| r.status), Some(Status::Ok));
        let bad = reports.iter().find(|r| r.endpoint == "https://other");
        assert_eq!(bad.map(|r| r.status), Some(Status::WrongChain));
        assert_eq!(
            bad.and_then(|r| r.error.clone()).as_deref(),
            Some("chain id is other-1, expected chain-1")
        );
    }

    #[test]
    fn catching_up_endpoint_is_not_healthy() {
        let mut node = up("a", "https://syncing", 1000);
        if let Ok(reading) = &mut node.result {
            reading.catching_up = true;
        }
        let reports = evaluate(vec![node], 50);
        assert_eq!(reports[0].status, Status::CatchingUp);
        assert_eq!(unhealthy_chains(&reports), vec!["a".to_string()]);
    }

    #[test]
    fn rows_sort_by_chain_then_height_desc_with_down_last() {
        let reports = evaluate(
            vec![
                down("b", "https://b-down"),
                up("b", "https://b-low", 10),
                up("a", "https://a", 5),
                up("b", "https://b-high", 20),
            ],
            50,
        );
        let order: Vec<&str> = reports.iter().map(|r| r.endpoint.as_str()).collect();
        assert_eq!(
            order,
            vec![
                "https://a",
                "https://b-high",
                "https://b-low",
                "https://b-down"
            ]
        );
    }

    #[test]
    fn chain_is_unhealthy_only_when_no_endpoint_is_ok() {
        let reports = evaluate(
            vec![
                up("a", "https://a1", 100),
                down("a", "https://a2"),
                down("b", "https://b1"),
            ],
            50,
        );
        assert_eq!(unhealthy_chains(&reports), vec!["b".to_string()]);
    }
}
