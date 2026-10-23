//! Prometheus text exposition format, for node_exporter's textfile collector.

use std::fmt::Write;

use crate::model::{EndpointReport, Status};

/// Render the four metrics. Height and behind are only written for endpoints
/// that are up: a down endpoint has no height, and the height of an endpoint
/// on the wrong chain belongs to another chain.
pub fn render(reports: &[EndpointReport]) -> String {
    let mut out = String::new();

    metric_header(
        &mut out,
        "rpc_up",
        "1 if the endpoint answered with a block height of the expected chain, else 0.",
    );
    for r in reports {
        let up = if is_up(r) { 1 } else { 0 };
        sample(&mut out, "rpc_up", r, &up.to_string());
    }

    metric_header(
        &mut out,
        "rpc_block_height",
        "Latest block height the endpoint reported.",
    );
    for r in reports.iter().filter(|r| is_up(r)) {
        if let Some(height) = r.height {
            sample(&mut out, "rpc_block_height", r, &height.to_string());
        }
    }

    metric_header(
        &mut out,
        "rpc_blocks_behind",
        "Blocks below the highest height seen for the same chain.",
    );
    for r in reports.iter().filter(|r| is_up(r)) {
        if let Some(behind) = r.behind {
            sample(&mut out, "rpc_blocks_behind", r, &behind.to_string());
        }
    }

    metric_header(
        &mut out,
        "rpc_latency_seconds",
        "Time the probe took, until the reply or the error.",
    );
    for r in reports {
        let seconds = r.latency_ms as f64 / 1000.0;
        sample(&mut out, "rpc_latency_seconds", r, &format!("{seconds:.3}"));
    }

    out
}

/// Up means: it answered with a height, and it is on the expected chain.
fn is_up(r: &EndpointReport) -> bool {
    r.height.is_some() && r.status != Status::WrongChain
}

fn metric_header(out: &mut String, name: &str, help: &str) {
    // Writing to a String cannot fail, so the Result is ignored.
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} gauge");
}

fn sample(out: &mut String, name: &str, r: &EndpointReport, value: &str) {
    let _ = writeln!(
        out,
        "{name}{{chain=\"{}\",endpoint=\"{}\",type=\"{}\"}} {value}",
        escape(&r.chain),
        escape(&r.endpoint),
        r.chain_type.as_str(),
    );
}

/// Label values must escape backslash, double quote and newline.
fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ChainType;

    fn row(endpoint: &str, height: Option<u64>, latency_ms: u64) -> EndpointReport {
        EndpointReport {
            chain: "atomone".into(),
            chain_type: ChainType::Cosmos,
            endpoint: endpoint.into(),
            status: if height.is_some() {
                Status::Ok
            } else {
                Status::Down
            },
            height,
            behind: height.map(|_| 0),
            latency_ms,
            chain_id: None,
            error: None,
        }
    }

    #[test]
    fn prom_output_matches_exposition_format() {
        let text = render(&[
            row("https://a", Some(100), 54),
            row("https://b", None, 5000),
        ]);
        let expected = r#"# HELP rpc_up 1 if the endpoint answered with a block height of the expected chain, else 0.
# TYPE rpc_up gauge
rpc_up{chain="atomone",endpoint="https://a",type="cosmos"} 1
rpc_up{chain="atomone",endpoint="https://b",type="cosmos"} 0
# HELP rpc_block_height Latest block height the endpoint reported.
# TYPE rpc_block_height gauge
rpc_block_height{chain="atomone",endpoint="https://a",type="cosmos"} 100
# HELP rpc_blocks_behind Blocks below the highest height seen for the same chain.
# TYPE rpc_blocks_behind gauge
rpc_blocks_behind{chain="atomone",endpoint="https://a",type="cosmos"} 0
# HELP rpc_latency_seconds Time the probe took, until the reply or the error.
# TYPE rpc_latency_seconds gauge
rpc_latency_seconds{chain="atomone",endpoint="https://a",type="cosmos"} 0.054
rpc_latency_seconds{chain="atomone",endpoint="https://b",type="cosmos"} 5.000
"#;
        assert_eq!(text, expected);
    }

    #[test]
    fn label_values_are_escaped() {
        assert_eq!(escape(r#"a"b\c"#), r#"a\"b\\c"#);
        assert_eq!(escape("a\nb"), "a\\nb");
    }
}
