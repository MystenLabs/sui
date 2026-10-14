// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::CommandFactory;
use clap::FromArgMatches;
use clap::Parser;
use clap::Subcommand;
use reqwest::Url;
use serde::Serialize;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use tracing::info;

use crate::AdvanceCheckpointRequest;
use crate::AdvanceClockRequest;
use crate::DEFAULT_RPC_ADDR;
use crate::ForkingServiceClient;
use crate::GetStatusRequest;
use crate::GraphQLClient;
use crate::SeedAddressRequest;
use crate::SeedDynamicFieldsOfRequest;
use crate::SeedObjectRequest;
use crate::StartArgs;
use crate::proto::forking::SeedOutcome;
use crate::seed::SeedInput;

#[derive(Parser)]
#[command(name = "sui-fork", about = "Fork and interact with a Sui network")]
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
        #[command(flatten)]
        args: StartArgs,
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

    /// Advance to a new checkpoint
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

    /// Seed pre-fork state into a running fork, under the same rules as the `start` seed flags
    Seed {
        /// RPC server address to connect to
        #[arg(long, default_value = DEFAULT_RPC_ADDR, value_parser = parse_rpc_addr)]
        rpc_addr: Url,

        /// Address-owned objects to seed at their fork-checkpoint versions
        #[arg(long = "object")]
        object_ids: Vec<ObjectID>,

        /// Addresses whose owned objects and balances to seed, which requires the fork checkpoint
        /// to be within roughly the last hour
        #[arg(long = "address")]
        addresses: Vec<SuiAddress>,

        /// Objects whose dynamic fields at the fork checkpoint to seed
        #[arg(long = "dynamic-fields-of", value_name = "PARENT")]
        dynamic_field_parents: Vec<ObjectID>,
    },
}

#[derive(Serialize)]
struct StartOutput {
    network: String,
    checkpoint: u64,
    rpc_addr: String,
    #[serde(skip)]
    current_checkpoint: u64,
    #[serde(skip)]
    resuming: bool,
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

/// The outcome of every request a `seed` command made, in the order they were made.
#[derive(Serialize)]
struct SeedOutput {
    requests: Vec<SeedRequestOutput>,
}

#[derive(Serialize)]
struct SeedRequestOutput {
    kind: &'static str,
    target: String,
    seeded: Vec<SeededObjectOutput>,
    already_present: Vec<String>,
}

#[derive(Serialize)]
struct SeededObjectOutput {
    object_id: String,
    version: u64,
    digest: String,
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
    pub fn parse_with_version(version: &'static str) -> Self {
        let command = Self::command().version(version);
        let matches = command.get_matches();

        Self::from_arg_matches(&matches).unwrap_or_else(|err| err.exit())
    }

    pub async fn execute(self, version: &'static str) -> Result<()> {
        match self.command {
            Command::Start { args } => cmd_start(args, self.json_output, version).await,
            Command::AdvanceClock {
                rpc_addr,
                duration_ms,
            } => cmd_advance_clock(rpc_addr, duration_ms, self.json_output).await,
            Command::AdvanceCheckpoint { rpc_addr } => {
                cmd_advance_checkpoint(rpc_addr, self.json_output).await
            }
            Command::Status { rpc_addr } => cmd_status(rpc_addr, self.json_output).await,
            Command::Seed {
                rpc_addr,
                object_ids,
                addresses,
                dynamic_field_parents,
            } => {
                cmd_seed(
                    rpc_addr,
                    object_ids,
                    addresses,
                    dynamic_field_parents,
                    self.json_output,
                )
                .await
            }
        }
    }
}

impl SeedRequestOutput {
    fn new(kind: &'static str, target: String, outcome: Option<SeedOutcome>) -> Self {
        let outcome = outcome.unwrap_or_default();
        Self {
            kind,
            target,
            seeded: outcome
                .seeded
                .into_iter()
                .map(|object| SeededObjectOutput {
                    object_id: object.object_id,
                    version: object.version,
                    digest: object.digest,
                })
                .collect(),
            already_present: outcome.already_present,
        }
    }
}

impl std::fmt::Display for StartOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.resuming {
            return write!(
                f,
                "Resuming forked network from {}; forked at checkpoint {}, current checkpoint {} (rpc on {})",
                self.network, self.checkpoint, self.current_checkpoint, self.rpc_addr,
            );
        }
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

impl std::fmt::Display for SeedOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, request) in self.requests.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(f, "{request}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for SeedRequestOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Seeded {} {}", self.kind, self.target)?;
        write!(f, "\n  seeded:          {}", self.seeded.len())?;
        for object in &self.seeded {
            write!(f, "\n    {} (version {})", object.object_id, object.version)?;
        }
        write!(f, "\n  already present: {}", self.already_present.len())?;
        for object_id in &self.already_present {
            write!(f, "\n    {object_id}")?;
        }
        Ok(())
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

async fn cmd_start(args: StartArgs, json_output: bool, version: &'static str) -> Result<()> {
    let StartArgs {
        network: node,
        checkpoint,
        data_dir,
        addresses,
        object_ids,
        dynamic_field_parents,
        rpc_addr,
    } = args;
    let seed_input = SeedInput {
        addresses: addresses.into_iter().collect(),
        object_ids: object_ids.into_iter().collect(),
        dynamic_field_parents: dynamic_field_parents.into_iter().collect(),
    };
    let network_name = node.network_name();

    let resolved_start = crate::startup::resolve_start_checkpoint_from_local(
        &node,
        checkpoint,
        data_dir.as_deref(),
    )?;
    let checkpoint = match resolved_start.checkpoint {
        Some(cp) => cp,
        None => GraphQLClient::new(node.clone(), version)?
            .get_latest_checkpoint_sequence_number()
            .await?
            .with_context(|| format!("failed to get latest checkpoint for {}", network_name))?,
    };

    let (context, subscription_handle) =
        crate::startup::initialize(node, checkpoint, version, data_dir, seed_input).await?;
    let current_checkpoint = {
        let sim = context.simulacrum().read().await;
        sim.store()
            .get_highest_verified_checkpoint()?
            .map(|checkpoint| checkpoint.data().sequence_number)
            .unwrap_or(checkpoint)
    };

    let output = StartOutput {
        network: network_name.clone(),
        checkpoint,
        rpc_addr: rpc_addr.to_string(),
        current_checkpoint,
        resuming: resolved_start.resuming,
    };
    print_output(&output, json_output);

    if resolved_start.resuming {
        info!(
            "Resuming forked network from {}; forked at checkpoint {}, current checkpoint {} (rpc on {})",
            network_name, checkpoint, current_checkpoint, rpc_addr,
        );
    } else {
        info!(
            "Starting forked network from {} at checkpoint {} (rpc on {})",
            network_name, checkpoint, rpc_addr,
        );
    }

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

/// Send one seed request per named object, address, and parent, and print what each changed.
///
/// Requests run in order and stop at the first failure, which is reported after the outcomes of
/// the requests that already applied, because those seeds stay in the fork.
async fn cmd_seed(
    rpc_addr: Url,
    object_ids: Vec<ObjectID>,
    addresses: Vec<SuiAddress>,
    dynamic_field_parents: Vec<ObjectID>,
    json_output: bool,
) -> Result<()> {
    if object_ids.is_empty() && addresses.is_empty() && dynamic_field_parents.is_empty() {
        bail!("nothing to seed: pass at least one --object, --address, or --dynamic-fields-of");
    }

    let mut client = ForkingServiceClient::connect(rpc_endpoint(&rpc_addr)).await?;
    let mut output = SeedOutput {
        requests: Vec::new(),
    };
    let result = async {
        for object_id in object_ids {
            let response = client
                .seed_object(SeedObjectRequest {
                    object_id: Some(object_id.to_string()),
                })
                .await
                .with_context(|| format!("failed to seed object {object_id}"))?
                .into_inner();
            output.requests.push(SeedRequestOutput::new(
                "object",
                object_id.to_string(),
                response.outcome,
            ));
        }
        for address in addresses {
            let response = client
                .seed_address(SeedAddressRequest {
                    address: Some(address.to_string()),
                })
                .await
                .with_context(|| format!("failed to seed address {address}"))?
                .into_inner();
            output.requests.push(SeedRequestOutput::new(
                "address",
                address.to_string(),
                response.outcome,
            ));
        }
        for parent in dynamic_field_parents {
            let response = client
                .seed_dynamic_fields_of(SeedDynamicFieldsOfRequest {
                    parent: Some(parent.to_string()),
                })
                .await
                .with_context(|| format!("failed to seed the dynamic fields of {parent}"))?
                .into_inner();
            output.requests.push(SeedRequestOutput::new(
                "dynamic-fields-of",
                parent.to_string(),
                response.outcome,
            ));
        }
        anyhow::Ok(())
    }
    .await;

    if !output.requests.is_empty() || result.is_ok() {
        print_output(&output, json_output);
    }
    result
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_output_describes_new_start() {
        let output = StartOutput {
            network: "mainnet".to_owned(),
            checkpoint: 11,
            rpc_addr: "127.0.0.1:9000".to_owned(),
            current_checkpoint: 11,
            resuming: false,
        };

        assert_eq!(
            output.to_string(),
            "Starting forked network from mainnet at checkpoint 11 (rpc on 127.0.0.1:9000)",
        );
    }

    #[test]
    fn start_output_describes_resume() {
        let output = StartOutput {
            network: "mainnet".to_owned(),
            checkpoint: 11,
            rpc_addr: "127.0.0.1:9000".to_owned(),
            current_checkpoint: 17,
            resuming: true,
        };

        assert_eq!(
            output.to_string(),
            "Resuming forked network from mainnet; forked at checkpoint 11, current checkpoint 17 (rpc on 127.0.0.1:9000)",
        );
    }

    #[test]
    fn client_commands_accept_default_rpc_addr() {
        for command in ["advance-clock", "advance-checkpoint", "status"] {
            let cli = Cli::try_parse_from(["sui-fork", command]).unwrap();
            let rpc_addr = match cli.command {
                Command::AdvanceClock { rpc_addr, .. }
                | Command::AdvanceCheckpoint { rpc_addr }
                | Command::Status { rpc_addr } => rpc_addr,
                Command::Start { .. } | Command::Seed { .. } => {
                    panic!("expected client command")
                }
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

    #[test]
    fn seed_command_collects_repeatable_flags() {
        let object = ObjectID::random();
        let address = SuiAddress::random_for_testing_only();
        let parent = ObjectID::random();
        let cli = Cli::try_parse_from([
            "sui-fork".to_owned(),
            "seed".to_owned(),
            "--object".to_owned(),
            object.to_string(),
            "--address".to_owned(),
            address.to_string(),
            "--dynamic-fields-of".to_owned(),
            parent.to_string(),
        ])
        .unwrap();

        let Command::Seed {
            rpc_addr,
            object_ids,
            addresses,
            dynamic_field_parents,
        } = cli.command
        else {
            panic!("expected the seed command");
        };
        assert_eq!(rpc_addr, Url::parse("http://127.0.0.1:9000").unwrap());
        assert_eq!(object_ids, vec![object]);
        assert_eq!(addresses, vec![address]);
        assert_eq!(dynamic_field_parents, vec![parent]);
    }

    #[test]
    fn seed_output_lists_seeded_and_already_present_objects() {
        let output = SeedOutput {
            requests: vec![SeedRequestOutput {
                kind: "address",
                target: "0xa".to_owned(),
                seeded: vec![SeededObjectOutput {
                    object_id: "0xb".to_owned(),
                    version: 7,
                    digest: "digest".to_owned(),
                }],
                already_present: vec!["0xc".to_owned()],
            }],
        };

        assert_eq!(
            output.to_string(),
            "Seeded address 0xa\n  seeded:          1\n    0xb (version 7)\n  already present: 1\n    0xc",
        );
    }
}
