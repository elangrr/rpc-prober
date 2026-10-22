//! rpc-prober: check many blockchain RPC endpoints at the same time.
//!
//! main.rs is the command line. This library holds the logic, so the
//! integration tests in tests/ can call it directly.

pub mod config;
pub mod model;
pub mod probe;
pub mod report;
