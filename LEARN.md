# Learn Rust with rpc-prober

This guide walks through every file. For each file: what it does, which Rust
features it uses, and why. The code excerpts are copied from the real files.

## How to run it

```sh
cargo build                                     # compile (debug)
cargo test                                      # run all tests
cargo run -- -c examples/indonode.toml          # table
cargo run -- -c examples/indonode.toml --json   # JSON
cargo run -- -c examples/indonode.toml --prom   # Prometheus
echo $?                                         # 0 healthy, 1 unhealthy, 2 bad config
cargo clippy --all-targets -- -D warnings       # lint
cargo fmt                                       # format the code
```

Everything after `--` goes to our program, not to cargo.

## The flow

```
  config.toml
      |
      v
  Config::load  ------------>  Vec<Target>   (one per RPC URL)
                                   |
                 probe_all: one tokio task per target,
                 at most `concurrency` running at once
             +---------------+---------------+
             v               v               v
        Cosmos.fetch     Evm.fetch       Tm2.fetch      (each inside a timeout)
             +---------------+---------------+
                                   |
                                   v
                            Vec<Outcome>     (height or error, latency)
                                   |
                   model::evaluate: best height per chain, status
                                   |
                                   v
                         Vec<EndpointReport>
                                   |
               +-------------------+-------------------+
               v                   v                   v
         report::table       report::json        report::prom
                                   |
                         exit code 0 / 1 / 2
```

## Words you will see

- **struct**: a type with named fields. Like a record.
- **enum**: a type that is one of several variants. Each variant can carry data.
- **trait**: a list of methods a type promises to have. Like an interface.
- **impl**: the block where you write methods for a type, or implement a trait.
- **`Result<T, E>`**: either `Ok(value)` or `Err(error)`. Rust has no exceptions.
- **`?`**: "if this is an `Err`, return it now; else unwrap the `Ok`".
- **`Option<T>`**: either `Some(value)` or `None`. Rust has no null.
- **`&`**: a borrow. You lend a value without giving it away.
- **derive**: `#[derive(...)]` asks the compiler to write common code for you.

---

## Cargo.toml

The project file. It names the crate and lists the libraries (crates) we use:
`tokio` (async runtime), `reqwest` (HTTP), `serde` + `serde_json` + `toml`
(read and write data), `clap` (command line flags), `anyhow` and `thiserror`
(errors). `wiremock` is only for tests, so it is under `[dev-dependencies]`.
`edition = "2024"` picks the newest set of language rules.

## src/lib.rs

Declares the modules. A **module** is a file (or folder) of code with its own
namespace. `pub` makes it visible outside the crate.

```rust
pub mod config;
pub mod model;
pub mod probe;
pub mod report;
```

**Why a lib and a bin?** `main.rs` is the program. `lib.rs` is a library. The
tests in `tests/` can only use a library, so the logic lives there and
`main.rs` stays small.

## src/main.rs

The command line. It reads flags, runs everything, prints, and sets the exit code.

**derive macros + clap.** We write a plain struct. `#[derive(Parser)]` makes
clap generate all the flag parsing from it. The `///` comments become the
`--help` text.

```rust
#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    /// Path to the TOML config file.
    #[arg(short, long)]
    config: PathBuf,
```

**async/await.** Network calls wait a lot. `async fn` can pause while waiting
so other work runs. `#[tokio::main]` starts the tokio runtime that drives them.

**match on Result.** `run` returns `Result<bool>`. `match` handles every case,
and the compiler checks that we did not forget one:

```rust
    match run(args).await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(2)
        }
    }
```

**`?` and `Option`.** `Config::load(...)?` returns early on error.
`args.timeout_ms` is an `Option<u64>`: `unwrap_or` uses the config value when
the flag was not given.

```rust
    let config = Config::load(&args.config)?;
    let timeout_ms = args.timeout_ms.unwrap_or(config.timeout_ms);
```

## src/config.rs

Reads the TOML file and checks it.

**serde derive.** `#[derive(Deserialize)]` turns TOML text into our struct.
`deny_unknown_fields` makes a typo an error. `rename = "chain"` maps the TOML
name `[[chain]]` to our field `chains`.

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
```

**anyhow and context.** `anyhow::Result` can hold any error. `with_context`
adds a sentence on top, so the user sees *which* file failed and *why*:

```rust
    pub fn load(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read config file {}", path.display()))?;
        Config::parse(&text).with_context(|| format!("invalid config file {}", path.display()))
    }
```

**Borrowing.** `path: &Path` borrows the path. The caller keeps it. `load`
only reads it, so it does not need to own it.

**bail!** returns an error right away: `bail!("timeout_ms must be greater than 0");`

## src/model.rs

The shared types and the health rules. No network code here, so it is easy to test.

**enum with derive.** `ChainType` is one of three values. `Copy` means it is
copied like a number instead of moved. `rename_all = "lowercase"` lets the
config say `type = "cosmos"`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChainType {
    /// CometBFT / Tendermint RPC: GET /status.
    Cosmos,
    /// Ethereum JSON-RPC: POST eth_blockNumber.
    Evm,
```

**Result inside a struct.** An `Outcome` holds either a reading or an error.
Errors are plain values in Rust, so you can store them like any other data.

```rust
pub struct Outcome {
    pub target: Target,
    pub latency: Duration,
    pub result: Result<Reading, ProbeError>,
}
```

**Option and safe math.** `best_height` may be missing (`None`). `u64` cannot
be negative, so `saturating_sub` stops at 0 instead of crashing:

```rust
pub fn blocks_behind(best_height: Option<u64>, height: u64) -> u64 {
    best_height.unwrap_or(height).saturating_sub(height)
}
```

**if/else as a value.** The whole `if` chain *is* the return value. No `return` needed.

```rust
    if !chain_id_matches(target, reading) {
        Status::WrongChain
    } else if reading.catching_up {
        Status::CatchingUp
    } else if behind > max_behind {
        Status::Lagging
    } else {
        Status::Ok
    }
```

**Ownership.** `evaluate(outcomes: Vec<Outcome>, ...)` takes the vector by
value: it *owns* it now. That lets `into_iter()` move each `Outcome` into a
report without copying. The caller cannot use `outcomes` afterwards, and the
compiler enforces that.

**let chains (edition 2024).** Two conditions in one `if`:

```rust
        if let Ok(reading) = &outcome.result
            && chain_id_matches(&outcome.target, reading)
        {
```

## src/probe/mod.rs

Runs the checks: the trait, the error type, the concurrency and the timeout.

**trait.** Every chain type must be able to "fetch a reading". One trait,
three implementations. Code that runs a probe does not care which one it has.

```rust
pub(crate) trait HeightSource {
    async fn fetch(&self, client: &Client, rpc: &str) -> Result<Reading, ProbeError>;
}
```

**thiserror.** `ProbeError` is an enum of everything that can go wrong. The
`#[error(...)]` line is the message the user sees.

```rust
#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("timed out after {0} ms")]
    Timeout(u64),
    #[error("request failed: {0}")]
    Network(String),
```

**Concurrency: tasks, Arc and Semaphore.** `tokio::spawn` starts each probe as
its own task, so they run at the same time. A `Semaphore` with N permits lets
only N run at once. `Arc` is a shared pointer: many tasks can hold the same
semaphore safely. `async move` moves the clones into the task, because the task
may outlive this function.

```rust
        let handle = tokio::spawn(async move {
            // Wait for a free slot. The semaphore is never closed, so this only waits.
            let _permit = permits.acquire_owned().await;
            probe_one(&client, task_target, timeout).await
        });
```

The permit is released when `_permit` goes out of scope. This is **RAII**:
cleanup happens when a value is dropped.

**Timeout.** `tokio::time::timeout` wraps the whole probe. If it is too slow,
we get `Err(_elapsed)` and turn it into our own error:

```rust
    let result = match tokio::time::timeout(timeout, fetch).await {
        Ok(result) => result,
        Err(_elapsed) => Err(ProbeError::Timeout(duration_ms(timeout))),
    };
```

**Generics.** `Envelope<T>` is the JSON-RPC wrapper. `T` is "whatever is
inside `result`". The same code works for the cosmos status and the EVM hex string.

## src/probe/cosmos.rs

Asks a CometBFT node `GET /status` and reads the height.

**Partial structs.** We describe only the fields we need. serde skips the rest
of the big reply.

```rust
#[derive(Debug, Deserialize)]
struct SyncInfo {
    /// CometBFT sends the height as a string: "10924691".
    latest_block_height: String,
    catching_up: bool,
}
```

**impl Trait for Type.** `Cosmos` is an empty struct (it has no settings). It
exists so it can implement `HeightSource`:

```rust
impl HeightSource for Cosmos {
    async fn fetch(&self, client: &Client, rpc: &str) -> Result<Reading, ProbeError> {
        let body = get_text(client, &join_url(rpc, "status")).await?;
        parse_status(&body)
    }
}
```

**Why split fetch and parse?** `parse_status` takes a `&str`, so the unit tests
can feed it a saved reply from `tests/fixtures/` with no network.

## src/probe/evm.rs

Asks an EVM node `eth_blockNumber` (and `eth_chainId`) over JSON-RPC.

**A struct with settings.** Unlike `Cosmos`, `Evm` has a field.

**Two requests at once.** A future does nothing until awaited. We create both,
then `tokio::join!` waits for both at the same time:

```rust
        let chain_id = call_hex(client, rpc, "eth_chainId");
        let (height, chain_id) = tokio::join!(height, chain_id);
        Ok(reading(height?, Some(chain_id?.to_string())))
```

**Option to Result.** `strip_prefix` returns an `Option`. `ok_or_else` turns
`None` into an error, so `?` can use it:

```rust
    let digits = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .ok_or_else(|| ProbeError::Parse(format!("{text:?} is not a 0x hex number")))?;
```

## src/probe/tm2.rs

Gno (Tendermint2) `GET /status`. The height may come as text or as a number.

**untagged enum.** serde tries each variant in order until one fits:

```rust
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Height {
    Number(u64),
    Text(String),
}
```

**match on enum variants.** Each variant has its own way to become a `u64`.
`*n` copies the number out of the borrow.

```rust
        match self {
            Height::Number(n) => Ok(*n),
            Height::Text(text) => parse_decimal_height(text),
        }
```

## src/report/mod.rs

Only declares the three output modules: `pub mod json; pub mod prom; pub mod table;`

## src/report/table.rs

Prints an aligned table. Two passes: first measure the widest cell in each
column, then pad every cell to that width.

**Fixed-size arrays.** A row is `[String; 8]`: always exactly 8 cells. The
compiler checks the count.

**Iterators.** `.iter().map(cells).collect()` turns every report into a row
without a hand-written loop:

```rust
    let rows: Vec<[String; 8]> = reports.iter().map(cells).collect();
```

**chars, not bytes.** `shorten` counts `chars()`. A Rust `String` is UTF-8, and
cutting by bytes could split a character in half.

## src/report/json.rs

Builds the `--json` output.

**Lifetimes.** `JsonReport<'a>` borrows the rows (`&'a [EndpointReport]`)
instead of copying them. `'a` tells the compiler: "this struct must not live
longer than the rows it borrows".

```rust
#[derive(Serialize)]
struct JsonReport<'a> {
    schema_version: u32,
    healthy: bool,
    max_behind: u64,
    unhealthy_chains: Vec<String>,
    endpoints: &'a [EndpointReport],
}
```

## src/report/prom.rs

Writes Prometheus metrics as text.

**`&mut` borrow.** Helper functions get `out: &mut String`: a borrow that may
change the string. Only one `&mut` can exist at a time, which rules out many bugs.

**write! into a String.** `writeln!` works on anything that implements the
`Write` trait, including `String`:

```rust
fn metric_header(out: &mut String, name: &str, help: &str) {
    // Writing to a String cannot fail, so the Result is ignored.
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} gauge");
}
```

## Tests

**Unit tests** live at the bottom of each file in `mod tests`.
`#[cfg(test)]` means "only compile this for `cargo test`". They can test
private functions, because they are inside the module.

**tests/probe_mock.rs** is an integration test. It starts real local HTTP
servers with `wiremock`: one healthy, one slow, one broken, one with nothing
listening. Then it checks each gets the right status:

```rust
    let timed_out = find(&reports, &slow.uri());
    assert_eq!(timed_out.status, Status::Down);
    assert_eq!(timed_out.error.as_deref(), Some("timed out after 300 ms"));
```

`as_deref()` turns `Option<String>` into `Option<&str>`, so it can be compared
with a string literal.

**tests/fixtures/** holds real replies saved from live RPCs.
`include_str!` pastes a file into the program at compile time.

## Rules this code follows

- No `unwrap()` outside tests: every error becomes a message, never a crash.
- No `unsafe`.
- Small files, one job each.

## Try this next

1. Add a `--chain <NAME>` flag that checks only one chain. Hint: filter
   `config.targets()` in `main.rs`.
2. EVM nodes can be syncing too. Call `eth_syncing` in `evm.rs` and set
   `catching_up` when the result is not `false`.
3. Add a `rpc_catching_up` gauge to `prom.rs`, with a unit test.
4. Make the table print latency in red when it is over 1000 ms. Only do it
   when the output is a terminal (`std::io::IsTerminal`).
5. Add a new chain type `solana` (`getSlot` JSON-RPC): one new file in
   `probe/`, one new `ChainType` variant. The compiler will show you every
   `match` that needs the new case.
