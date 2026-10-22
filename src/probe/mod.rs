//! Check endpoints over HTTP. One `HeightSource` per chain type.

mod cosmos;
mod evm;
mod tm2;

use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::Deserialize;
use tokio::sync::Semaphore;

use crate::model::{ChainType, Outcome, Reading, Target};

use cosmos::Cosmos;
use evm::Evm;
use tm2::Tm2;

/// Everything that can go wrong while checking one endpoint.
#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("timed out after {0} ms")]
    Timeout(u64),
    #[error("request failed: {0}")]
    Network(String),
    #[error("HTTP {0}")]
    Http(String),
    #[error("unexpected reply: {0}")]
    Parse(String),
    #[error("RPC error {code}: {message}")]
    Rpc { code: i64, message: String },
    #[error("probe task failed: {0}")]
    Task(String),
}

/// Something that can ask an RPC endpoint for its latest block height.
///
/// There is one implementation per chain type: `Cosmos`, `Evm` and `Tm2`.
pub(crate) trait HeightSource {
    async fn fetch(&self, client: &Client, rpc: &str) -> Result<Reading, ProbeError>;
}

/// Build the HTTP client used for every request.
///
/// It has no timeout of its own: `probe_one` puts one time limit around the
/// whole probe (EVM makes two requests), so every slow endpoint gets the same
/// "timed out after N ms" error.
pub fn build_client() -> Result<Client, reqwest::Error> {
    Client::builder()
        .user_agent(concat!("rpc-prober/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// Check every target, at most `concurrency` at the same time.
/// The result has one `Outcome` per target, in the same order.
pub async fn probe_all(
    client: &Client,
    targets: Vec<Target>,
    timeout: Duration,
    concurrency: usize,
) -> Vec<Outcome> {
    let permits = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut handles = Vec::with_capacity(targets.len());

    for target in targets {
        let client = client.clone();
        let permits = Arc::clone(&permits);
        let task_target = target.clone();
        let handle = tokio::spawn(async move {
            // Wait for a free slot. The semaphore is never closed, so this only waits.
            let _permit = permits.acquire_owned().await;
            probe_one(&client, task_target, timeout).await
        });
        handles.push((target, handle));
    }

    let mut outcomes = Vec::with_capacity(handles.len());
    for (target, handle) in handles {
        let outcome = match handle.await {
            Ok(outcome) => outcome,
            Err(err) => Outcome {
                target,
                latency: Duration::ZERO,
                result: Err(ProbeError::Task(err.to_string())),
            },
        };
        outcomes.push(outcome);
    }
    outcomes
}

/// Check one target and time it. A probe slower than `timeout` counts as down.
pub async fn probe_one(client: &Client, target: Target, timeout: Duration) -> Outcome {
    let start = Instant::now();
    let fetch = fetch_reading(client, &target);
    let result = match tokio::time::timeout(timeout, fetch).await {
        Ok(result) => result,
        Err(_elapsed) => Err(ProbeError::Timeout(duration_ms(timeout))),
    };
    Outcome {
        target,
        latency: start.elapsed(),
        result,
    }
}

/// Pick the right `HeightSource` for the chain type and run it.
async fn fetch_reading(client: &Client, target: &Target) -> Result<Reading, ProbeError> {
    match target.chain_type {
        ChainType::Cosmos => Cosmos.fetch(client, &target.url).await,
        ChainType::Tm2 => Tm2.fetch(client, &target.url).await,
        ChainType::Evm => {
            let evm = Evm {
                check_chain_id: target.expected_chain_id.is_some(),
            };
            evm.fetch(client, &target.url).await
        }
    }
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// `https://host/` + `status` -> `https://host/status`.
pub(crate) fn join_url(rpc: &str, path: &str) -> String {
    format!("{}/{}", rpc.trim_end_matches('/'), path)
}

/// GET a URL and return the body. Non-2xx answers become `ProbeError::Http`.
pub(crate) async fn get_text(client: &Client, url: &str) -> Result<String, ProbeError> {
    let response = client.get(url).send().await.map_err(network_error)?;
    read_body(response).await
}

/// POST a JSON body and return the reply body.
pub(crate) async fn post_json(
    client: &Client,
    url: &str,
    body: &serde_json::Value,
) -> Result<String, ProbeError> {
    let response = client
        .post(url)
        .json(body)
        .send()
        .await
        .map_err(network_error)?;
    read_body(response).await
}

async fn read_body(response: reqwest::Response) -> Result<String, ProbeError> {
    let status = response.status();
    if !status.is_success() {
        return Err(ProbeError::Http(status.to_string()));
    }
    response.text().await.map_err(network_error)
}

/// reqwest errors hide the real cause (DNS, TLS, refused) in `source()`.
/// Walk to the end of the chain and report only that root cause: the outer
/// layers ("error sending request", "client error (Connect)") add no news.
fn network_error(err: reqwest::Error) -> ProbeError {
    let err = err.without_url();
    let mut message = err.to_string();
    let mut source = std::error::Error::source(&err);
    while let Some(cause) = source {
        message = cause.to_string();
        source = cause.source();
    }
    ProbeError::Network(message)
}

/// The JSON-RPC 2.0 wrapper every chain type uses: `result` or `error`.
#[derive(Debug, Deserialize)]
pub(crate) struct Envelope<T> {
    result: Option<T>,
    error: Option<RpcErrorBody>,
}

#[derive(Debug, Deserialize)]
struct RpcErrorBody {
    code: i64,
    message: String,
}

impl<T> Envelope<T> {
    /// Return the `result`, or turn the `error` into a `ProbeError`.
    pub(crate) fn into_result(self) -> Result<T, ProbeError> {
        match (self.result, self.error) {
            (_, Some(err)) => Err(ProbeError::Rpc {
                code: err.code,
                message: err.message,
            }),
            (Some(result), None) => Ok(result),
            (None, None) => Err(ProbeError::Parse(
                "reply has neither result nor error".to_string(),
            )),
        }
    }
}

/// Parse a JSON-RPC reply body into `T`.
pub(crate) fn parse_envelope<T>(body: &str) -> Result<T, ProbeError>
where
    T: for<'de> Deserialize<'de>,
{
    let envelope: Envelope<T> =
        serde_json::from_str(body).map_err(|err| ProbeError::Parse(err.to_string()))?;
    envelope.into_result()
}

/// Parse a decimal height like `"10924691"`.
pub(crate) fn parse_decimal_height(text: &str) -> Result<u64, ProbeError> {
    text.parse::<u64>()
        .map_err(|_| ProbeError::Parse(format!("height {text:?} is not a whole number")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_url_handles_trailing_slash() {
        assert_eq!(join_url("https://a.b", "status"), "https://a.b/status");
        assert_eq!(join_url("https://a.b/", "status"), "https://a.b/status");
        assert_eq!(
            join_url("https://a.b/rpc/", "status"),
            "https://a.b/rpc/status"
        );
    }

    #[test]
    fn decimal_height_rejects_garbage() {
        assert_eq!(parse_decimal_height("42").ok(), Some(42));
        assert!(parse_decimal_height("-1").is_err());
        assert!(parse_decimal_height("").is_err());
    }

    #[test]
    fn reply_without_result_or_error_is_a_parse_error() {
        let err = parse_envelope::<String>(r#"{"jsonrpc":"2.0","id":1}"#);
        assert!(matches!(err, Err(ProbeError::Parse(_))));
    }
}
