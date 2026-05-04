// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use reqwest::Url;
use serde::Serialize;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use tracing::info;

use crate::AdvanceCheckpointRequest;
use crate::AdvanceClockRequest;
use crate::ForkingServiceClient;
use crate::GetStatusRequest;
use crate::GraphQLClient;
use crate::Node;
use crate::seed::SeedInput;

/// Default bind address for the RPC server.
pub const DEFAULT_RPC_ADDR: &str = "127.0.0.1:9000";

#[derive(Parser)]
#[command(name = "sui-forking", about = "Fork and interact with a Sui network")]
pub struct Cli {
    /// Output results as JSON
    #[arg(long = "json", global = true)]
    json_output: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start a forked Sui network
    Start {
        /// Network to fork from: mainnet, testnet, devnet, or a custom GraphQL URL
        #[arg(long, default_value = "mainnet")]
        network: Node,

        /// Checkpoint sequence number to fork at (defaults to latest)
        #[arg(long)]
        checkpoint: Option<u64>,

        /// Base directory for on-disk storage (overrides FORKING_DATA_STORE env var)
        #[arg(long, env = "FORKING_DATA_STORE")]
        data_dir: Option<PathBuf>,

        /// Address whose owned objects should seed the initial owned-object index
        #[arg(long = "address")]
        addresses: Vec<SuiAddress>,

        /// Object ID to fetch and seed if it is address-owned
        #[arg(long = "object")]
        object_ids: Vec<ObjectID>,

        /// Address to bind the RPC server to
        #[arg(long, default_value = DEFAULT_RPC_ADDR)]
        rpc_addr: SocketAddr,
    },

    /// Advance the network clock by a given duration
    AdvanceClock {
        /// RPC server address to connect to
        #[arg(long, default_value = DEFAULT_RPC_ADDR, value_parser = parse_rpc_addr)]
        rpc_addr: Url,

        /// Duration in milliseconds to advance the clock (defaults to 1)
        #[arg(long)]
        duration_ms: Option<u64>,
    },

    /// Seal pending transactions into a new checkpoint
    AdvanceCheckpoint {
        /// RPC server address to connect to
        #[arg(long, default_value = DEFAULT_RPC_ADDR, value_parser = parse_rpc_addr)]
        rpc_addr: Url,
    },

    /// Get the current status of the forked network
    Status {
        /// RPC server address to connect to
        #[arg(long, default_value = DEFAULT_RPC_ADDR, value_parser = parse_rpc_addr)]
        rpc_addr: Url,
    },
}

#[derive(Serialize)]
struct StartOutput {
    network: String,
    checkpoint: u64,
    rpc_addr: String,
}

#[derive(Serialize)]
struct AdvanceClockOutput {
    timestamp_ms: u64,
    timestamp: String,
    tx_digest: String,
}

#[derive(Serialize)]
struct AdvanceCheckpointOutput {
    checkpoint_sequence_number: u64,
    timestamp_ms: u64,
    timestamp: String,
}

#[derive(Serialize)]
struct StatusOutput {
    epoch: u64,
    checkpoint_sequence_number: u64,
    timestamp_ms: u64,
    timestamp: String,
    forked_at_checkpoint: u64,
}

impl Cli {
    pub async fn execute(self, version: &'static str) -> Result<()> {
        match self.command {
            Command::Start {
                network,
                checkpoint,
                data_dir,
                addresses,
                object_ids,
                rpc_addr,
            } => {
                cmd_start(
                    network,
                    checkpoint,
                    data_dir,
                    SeedInput {
                        addresses,
                        object_ids,
                    },
                    rpc_addr,
                    self.json_output,
                    version,
                )
                .await
            }
            Command::AdvanceClock {
                rpc_addr,
                duration_ms,
            } => cmd_advance_clock(rpc_addr, duration_ms, self.json_output).await,
            Command::AdvanceCheckpoint { rpc_addr } => {
                cmd_advance_checkpoint(rpc_addr, self.json_output).await
            }
            Command::Status { rpc_addr } => cmd_status(rpc_addr, self.json_output).await,
        }
    }
}

fn print_output<T: Serialize + std::fmt::Display>(value: &T, json_output: bool) {
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(value).expect("serialization cannot fail")
        );
    } else {
        println!("{value}");
    }
}

async fn cmd_start(
    node: Node,
    checkpoint: Option<u64>,
    data_dir: Option<PathBuf>,
    seed_input: SeedInput,
    rpc_addr: SocketAddr,
    json_output: bool,
    version: &'static str,
) -> Result<()> {
    let network_name = node.network_name();

    let checkpoint = match checkpoint {
        Some(cp) => cp,
        None => GraphQLClient::new(node.clone(), version)?
            .get_latest_checkpoint_sequence_number()
            .await?
            .with_context(|| format!("failed to get latest checkpoint for {}", network_name))?,
    };

    let (context, subscription_handle) =
        crate::startup::initialize(node, checkpoint, version, data_dir, seed_input).await?;

    let output = StartOutput {
        network: network_name.clone(),
        checkpoint,
        rpc_addr: rpc_addr.to_string(),
    };
    print_output(&output, json_output);

    info!(
        "Starting forked network from {} at checkpoint {} (rpc on {})",
        network_name, checkpoint, rpc_addr,
    );

    let handle = tokio::spawn(crate::startup::run(
        context,
        subscription_handle,
        rpc_addr,
        version,
    ));
    handle.await??;

    Ok(())
}

async fn cmd_advance_clock(
    rpc_addr: Url,
    duration_ms: Option<u64>,
    json_output: bool,
) -> Result<()> {
    let mut client = ForkingServiceClient::connect(rpc_endpoint(&rpc_addr)).await?;
    let resp = client
        .advance_clock(AdvanceClockRequest { duration_ms })
        .await?
        .into_inner();

    let output = AdvanceClockOutput {
        timestamp_ms: resp.timestamp_ms,
        timestamp: format_timestamp(resp.timestamp_ms),
        tx_digest: resp.tx_digest,
    };
    print_output(&output, json_output);
    Ok(())
}

async fn cmd_advance_checkpoint(rpc_addr: Url, json_output: bool) -> Result<()> {
    let mut client = ForkingServiceClient::connect(rpc_endpoint(&rpc_addr)).await?;
    let resp = client
        .advance_checkpoint(AdvanceCheckpointRequest {})
        .await?
        .into_inner();

    let output = AdvanceCheckpointOutput {
        checkpoint_sequence_number: resp.checkpoint_sequence_number,
        timestamp_ms: resp.timestamp_ms,
        timestamp: format_timestamp(resp.timestamp_ms),
    };
    print_output(&output, json_output);
    Ok(())
}

async fn cmd_status(rpc_addr: Url, json_output: bool) -> Result<()> {
    let mut client = ForkingServiceClient::connect(rpc_endpoint(&rpc_addr)).await?;
    let resp = client.get_status(GetStatusRequest {}).await?.into_inner();

    let output = StatusOutput {
        epoch: resp.epoch,
        checkpoint_sequence_number: resp.checkpoint_sequence_number,
        timestamp_ms: resp.timestamp_ms,
        timestamp: format_timestamp(resp.timestamp_ms),
        forked_at_checkpoint: resp.forked_at_checkpoint,
    };
    print_output(&output, json_output);
    Ok(())
}

fn parse_rpc_addr(rpc_addr: &str) -> std::result::Result<Url, String> {
    let endpoint = if rpc_addr.contains("://") {
        rpc_addr.to_owned()
    } else {
        format!("http://{rpc_addr}")
    };
    let url =
        Url::parse(&endpoint).map_err(|err| format!("invalid --rpc-addr `{rpc_addr}`: {err}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!(
            "--rpc-addr must use http or https, got `{}`",
            url.scheme(),
        ));
    }
    Ok(url)
}

fn rpc_endpoint(rpc_addr: &Url) -> String {
    rpc_addr.as_str().to_owned()
}

fn format_timestamp(ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(ms as i64)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| format!("{ms}ms"))
}

impl std::fmt::Display for StartOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Starting forked network from {} at checkpoint {} (rpc on {})",
            self.network, self.checkpoint, self.rpc_addr,
        )
    }
}

impl std::fmt::Display for AdvanceClockOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Clock advanced")?;
        writeln!(f, "  timestamp: {} ({})", self.timestamp_ms, self.timestamp)?;
        write!(f, "  tx digest: {}", self.tx_digest)
    }
}

impl std::fmt::Display for AdvanceCheckpointOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Checkpoint created")?;
        writeln!(
            f,
            "  checkpoint number: {}",
            self.checkpoint_sequence_number
        )?;
        write!(
            f,
            "  timestamp:         {} ({})",
            self.timestamp_ms, self.timestamp,
        )
    }
}

impl std::fmt::Display for StatusOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Forked network status")?;
        writeln!(f, "  epoch:                {}", self.epoch)?;
        writeln!(
            f,
            "  checkpoint:           {}",
            self.checkpoint_sequence_number,
        )?;
        writeln!(
            f,
            "  timestamp:            {} ({})",
            self.timestamp_ms, self.timestamp,
        )?;
        write!(f, "  forked at checkpoint: {}", self.forked_at_checkpoint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_commands_accept_default_rpc_addr() {
        for command in ["advance-clock", "advance-checkpoint", "status"] {
            let cli = Cli::try_parse_from(["sui-forking", command]).unwrap();
            let rpc_addr = match cli.command {
                Command::AdvanceClock { rpc_addr, .. }
                | Command::AdvanceCheckpoint { rpc_addr }
                | Command::Status { rpc_addr } => rpc_addr,
                Command::Start { .. } => panic!("expected client command"),
            };

            assert_eq!(rpc_addr, Url::parse("http://127.0.0.1:9000").unwrap());
            assert_eq!(rpc_endpoint(&rpc_addr), "http://127.0.0.1:9000/");
        }
    }

    #[test]
    fn rpc_endpoint_accepts_bare_address_or_full_url() {
        assert_eq!(
            parse_rpc_addr("127.0.0.1:9001").unwrap(),
            Url::parse("http://127.0.0.1:9001").unwrap(),
        );
        assert_eq!(
            parse_rpc_addr("http://127.0.0.1:9001").unwrap(),
            Url::parse("http://127.0.0.1:9001").unwrap(),
        );
    }

    #[test]
    fn rpc_endpoint_rejects_non_http_schemes() {
        let err = parse_rpc_addr("ftp://127.0.0.1:9000").unwrap_err();

        assert!(err.contains("must use http or https"));
    }
}
