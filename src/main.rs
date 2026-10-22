//! The command line: read flags, run the probes, print the report.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Parser;

use rpc_prober::config::Config;
use rpc_prober::{model, probe, report};

/// Check blockchain RPC endpoints: up or down, block height, blocks behind, latency.
///
/// Exit code: 0 when every chain has at least one endpoint with status ok,
/// 1 when a chain has none, 2 when the config or flags are invalid.
#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    /// Path to the TOML config file.
    #[arg(short, long)]
    config: PathBuf,

    /// Time limit per endpoint in milliseconds. Overrides timeout_ms in the config.
    #[arg(long)]
    timeout_ms: Option<u64>,

    /// An endpoint more than this many blocks behind the best height is "lagging".
    #[arg(long, default_value_t = 50)]
    max_behind: u64,

    /// Print JSON instead of a table.
    #[arg(long, conflicts_with = "prom")]
    json: bool,

    /// Print Prometheus metrics instead of a table.
    #[arg(long)]
    prom: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();
    match run(args).await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(2)
        }
    }
}

/// Returns Ok(true) when every chain is healthy.
async fn run(args: Args) -> Result<bool> {
    let config = Config::load(&args.config)?;
    let timeout_ms = args.timeout_ms.unwrap_or(config.timeout_ms);
    if timeout_ms == 0 {
        bail!("--timeout-ms must be greater than 0");
    }
    let timeout = Duration::from_millis(timeout_ms);

    let client = probe::build_client().context("cannot create the HTTP client")?;
    let outcomes = probe::probe_all(&client, config.targets(), timeout, config.concurrency).await;
    let reports = model::evaluate(outcomes, args.max_behind);
    let unhealthy = model::unhealthy_chains(&reports);

    if args.json {
        println!("{}", report::json::render(&reports, args.max_behind)?);
    } else if args.prom {
        print!("{}", report::prom::render(&reports));
    } else {
        print!("{}", report::table::render(&reports));
        if !unhealthy.is_empty() {
            println!("\nno healthy endpoint: {}", unhealthy.join(", "));
        }
    }
    Ok(unhealthy.is_empty())
}
