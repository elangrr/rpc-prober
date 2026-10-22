//! Read and check the TOML config file.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::model::{ChainType, Target};

pub const DEFAULT_TIMEOUT_MS: u64 = 5000;
pub const DEFAULT_CONCURRENCY: usize = 16;

/// The whole config file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    /// Each `[[chain]]` table in the file.
    #[serde(rename = "chain", default)]
    pub chains: Vec<ChainConfig>,
}

/// One `[[chain]]` table.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub chain_type: ChainType,
    pub chain_id: Option<String>,
    pub rpcs: Vec<String>,
}

fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

fn default_concurrency() -> usize {
    DEFAULT_CONCURRENCY
}

impl Config {
    /// Read the file at `path`, then parse and check it.
    pub fn load(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read config file {}", path.display()))?;
        Config::parse(&text).with_context(|| format!("invalid config file {}", path.display()))
    }

    /// Parse TOML text and check that it makes sense.
    pub fn parse(text: &str) -> Result<Config> {
        let config: Config = toml::from_str(text)?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.timeout_ms == 0 {
            bail!("timeout_ms must be greater than 0");
        }
        if self.concurrency == 0 {
            bail!("concurrency must be greater than 0");
        }
        if self.chains.is_empty() {
            bail!("no [[chain]] tables found; add at least one");
        }
        let mut names = BTreeSet::new();
        for chain in &self.chains {
            if chain.name.trim().is_empty() {
                bail!("a [[chain]] has an empty name");
            }
            if !names.insert(chain.name.as_str()) {
                bail!("chain name \"{}\" is used twice", chain.name);
            }
            if chain.rpcs.is_empty() {
                bail!("chain \"{}\" has no rpcs", chain.name);
            }
            for rpc in &chain.rpcs {
                if !(rpc.starts_with("http://") || rpc.starts_with("https://")) {
                    bail!(
                        "chain \"{}\": rpc \"{}\" must start with http:// or https://",
                        chain.name,
                        rpc
                    );
                }
            }
        }
        Ok(())
    }

    /// One `Target` per RPC URL, in file order.
    pub fn targets(&self) -> Vec<Target> {
        let mut targets = Vec::new();
        for chain in &self.chains {
            for rpc in &chain.rpcs {
                targets.push(Target {
                    chain: chain.name.clone(),
                    chain_type: chain.chain_type,
                    expected_chain_id: chain.chain_id.clone(),
                    url: rpc.clone(),
                });
            }
        }
        targets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error_text(toml: &str) -> String {
        match Config::parse(toml) {
            Ok(_) => String::from("parsed without error"),
            Err(err) => format!("{err:#}"),
        }
    }

    #[test]
    fn example_config_parses_with_defaults() {
        let text = include_str!("../examples/indonode.toml");
        let config = Config::parse(text).expect("example config is valid");
        assert_eq!(config.timeout_ms, 5000);
        assert!(config.chains.iter().any(|c| c.chain_type == ChainType::Tm2));
        assert!(config.chains.iter().any(|c| c.chain_type == ChainType::Evm));
        assert_eq!(config.targets().len(), 22);
    }

    #[test]
    fn missing_timeout_uses_default() {
        let config =
            Config::parse("[[chain]]\nname = \"a\"\ntype = \"cosmos\"\nrpcs = [\"https://x\"]\n")
                .expect("valid");
        assert_eq!(config.timeout_ms, DEFAULT_TIMEOUT_MS);
        assert_eq!(config.concurrency, DEFAULT_CONCURRENCY);
        assert_eq!(config.chains[0].chain_id, None);
    }

    #[test]
    fn unknown_chain_type_is_rejected() {
        let err =
            error_text("[[chain]]\nname = \"a\"\ntype = \"solana\"\nrpcs = [\"https://x\"]\n");
        assert!(err.contains("unknown variant `solana`"), "{err}");
    }

    #[test]
    fn misspelled_key_is_rejected() {
        let err = error_text("[[chain]]\nname = \"a\"\ntype = \"cosmos\"\nrpc = [\"https://x\"]\n");
        assert!(err.contains("unknown field `rpc`"), "{err}");
    }

    #[test]
    fn empty_rpcs_is_rejected() {
        let err = error_text("[[chain]]\nname = \"a\"\ntype = \"cosmos\"\nrpcs = []\n");
        assert_eq!(err, "chain \"a\" has no rpcs");
    }

    #[test]
    fn rpc_without_scheme_is_rejected() {
        let err =
            error_text("[[chain]]\nname = \"a\"\ntype = \"evm\"\nrpcs = [\"rpc.example.com\"]\n");
        assert!(err.contains("must start with http:// or https://"), "{err}");
    }

    #[test]
    fn duplicate_chain_name_is_rejected() {
        let one = "[[chain]]\nname = \"a\"\ntype = \"cosmos\"\nrpcs = [\"https://x\"]\n";
        let err = error_text(&format!("{one}{one}"));
        assert_eq!(err, "chain name \"a\" is used twice");
    }

    #[test]
    fn config_without_chains_is_rejected() {
        assert_eq!(
            error_text("timeout_ms = 100\n"),
            "no [[chain]] tables found; add at least one"
        );
    }
}
