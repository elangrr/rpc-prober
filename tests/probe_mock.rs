//! End-to-end probes against local mock HTTP servers (wiremock).

use std::net::TcpListener;
use std::time::{Duration, Instant};

use rpc_prober::model::{self, ChainType, EndpointReport, Status, Target};
use rpc_prober::probe;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const COSMOS_STATUS: &str = include_str!("fixtures/cosmos_status.json");
const TM2_STATUS: &str = include_str!("fixtures/tm2_status.json");
const EVM_BLOCK_NUMBER: &str = include_str!("fixtures/evm_block_number.json");
const EVM_CHAIN_ID: &str = include_str!("fixtures/evm_chain_id.json");

fn target(chain: &str, chain_type: ChainType, chain_id: Option<&str>, url: String) -> Target {
    Target {
        chain: chain.to_string(),
        chain_type,
        expected_chain_id: chain_id.map(str::to_string),
        url,
    }
}

/// A mock server that answers GET /status with `body` after `delay`.
async fn status_server(status: u16, body: &str, delay: Duration) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/status"))
        .respond_with(
            ResponseTemplate::new(status)
                .set_body_string(body)
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    server
}

/// A local port with nothing listening on it.
fn closed_port_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a free port");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

fn find<'a>(reports: &'a [EndpointReport], url: &str) -> &'a EndpointReport {
    reports
        .iter()
        .find(|r| r.endpoint == url)
        .expect("a report row for every target")
}

#[tokio::test]
async fn up_down_and_timeout_endpoints_get_the_right_status() {
    let healthy = status_server(200, COSMOS_STATUS, Duration::ZERO).await;
    let slow = status_server(200, COSMOS_STATUS, Duration::from_secs(5)).await;
    let broken = status_server(502, "bad gateway", Duration::ZERO).await;
    let gno = status_server(200, TM2_STATUS, Duration::ZERO).await;

    let evm = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("eth_blockNumber"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EVM_BLOCK_NUMBER))
        .mount(&evm)
        .await;
    Mock::given(method("POST"))
        .and(body_string_contains("eth_chainId"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EVM_CHAIN_ID))
        .mount(&evm)
        .await;

    let refused = closed_port_url();
    let targets = vec![
        target(
            "atomone",
            ChainType::Cosmos,
            Some("atomone-1"),
            healthy.uri(),
        ),
        target("atomone", ChainType::Cosmos, Some("atomone-1"), slow.uri()),
        target(
            "atomone",
            ChainType::Cosmos,
            Some("atomone-1"),
            broken.uri(),
        ),
        target(
            "atomone",
            ChainType::Cosmos,
            Some("atomone-1"),
            refused.clone(),
        ),
        target("wrong", ChainType::Cosmos, Some("other-1"), healthy.uri()),
        target("gno", ChainType::Tm2, Some("gnoland-1"), gno.uri()),
        target("epix-evm", ChainType::Evm, Some("1916"), evm.uri()),
    ];

    let timeout = Duration::from_millis(300);
    let client = probe::build_client().expect("client");
    let outcomes = probe::probe_all(&client, targets, timeout, 4).await;
    let reports = model::evaluate(outcomes, 50);

    let up = find(&reports, &healthy.uri());
    assert_eq!(up.status, Status::Ok);
    assert_eq!(up.height, Some(10_924_691));

    let timed_out = find(&reports, &slow.uri());
    assert_eq!(timed_out.status, Status::Down);
    assert_eq!(timed_out.error.as_deref(), Some("timed out after 300 ms"));

    let bad_gateway = find(&reports, &broken.uri());
    assert_eq!(bad_gateway.status, Status::Down);
    assert_eq!(bad_gateway.error.as_deref(), Some("HTTP 502 Bad Gateway"));

    let no_listener = find(&reports, &refused);
    assert_eq!(no_listener.status, Status::Down);
    let error = no_listener.error.clone().unwrap_or_default();
    assert!(error.starts_with("request failed:"), "{error}");

    let gno_row = find(&reports, &gno.uri());
    assert_eq!(
        (gno_row.status, gno_row.height),
        (Status::Ok, Some(995_854))
    );

    let evm_row = find(&reports, &evm.uri());
    assert_eq!(
        (evm_row.status, evm_row.height),
        (Status::Ok, Some(6_179_346))
    );
    assert_eq!(evm_row.chain_id.as_deref(), Some("1916"));

    // "wrong" expects other-1 but the server says atomone-1.
    assert_eq!(model::unhealthy_chains(&reports), vec!["wrong".to_string()]);
}

#[tokio::test]
async fn concurrency_cap_limits_parallel_requests() {
    let delay = Duration::from_millis(200);
    let server = status_server(200, COSMOS_STATUS, delay).await;
    let targets: Vec<Target> = (0..4)
        .map(|_| target("atomone", ChainType::Cosmos, None, server.uri()))
        .collect();

    let timeout = Duration::from_secs(5);
    let client = probe::build_client().expect("client");
    let start = Instant::now();
    let outcomes = probe::probe_all(&client, targets, timeout, 2).await;

    // 4 requests, 2 at a time, 200 ms each: at least 2 rounds.
    assert!(start.elapsed() >= delay * 2, "took {:?}", start.elapsed());
    assert!(outcomes.iter().all(|o| o.result.is_ok()));
}
