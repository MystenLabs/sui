// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    db_tool::{execute_db_tool_command, print_db_all_tables, DbToolCommand},
    get_object, get_transaction_block, make_clients, restore_from_db_checkpoint,
    ConciseObjectOutput, GroupedObjectOutput, VerboseObjectOutput,
};
use anyhow::Result;
use tokio::sync::mpsc;
use sui_network::default_mysten_network_config;
use std::{path::PathBuf, sync::Arc, str::FromStr};
use sui_sdk::{SuiClientBuilder, apis::ReadApi};
use sui_config::genesis::Genesis;
use sui_core::{authority_client::{AuthorityAPI, NetworkAuthorityClient}, quorum_driver, epoch::committee_store::CommitteeStore, safe_client::SafeClientMetricsBase, authority_aggregator::{AuthAggMetrics, AuthorityAggregator}};

use sui_types::{base_types::*, object::Owner};
use mysten_network::Multiaddr;
use sui_json_rpc_types::{SuiTransactionBlockResponseQuery, SuiTransactionBlockResponseOptions};
use clap::*;
use sui_config::Config;
use sui_types::messages_checkpoint::{
    CheckpointRequest, CheckpointResponse, CheckpointSequenceNumber,
};

#[derive(Parser, Clone, ValueEnum)]
pub enum Verbosity {
    Grouped,
    Concise,
    Verbose,
}

#[derive(Parser)]
#[clap(
    name = "sui-tool",
    about = "Debugging utilities for sui",
    rename_all = "kebab-case",
    author,
    version
)]
pub enum ToolCommand {
    /// Fetch the same object from all validators
    #[clap(name = "fetch-object")]
    FetchObject {
        #[clap(long, help = "The object ID to fetch")]
        id: ObjectID,

        #[clap(long, help = "Fetch object at a specific sequence")]
        version: Option<u64>,

        #[clap(
            long,
            help = "Validator to fetch from - if not specified, all validators are queried"
        )]
        validator: Option<AuthorityName>,

        #[clap(long = "genesis")]
        genesis: PathBuf,

        #[clap(long = "history", help = "show full history of object")]
        history: bool,

        /// Concise mode groups responses by results.
        /// prints tabular output suitable for processing with unix tools. For
        /// instance, to quickly check that all validators agree on the history of an object:
        /// ```text
        /// $ sui-tool fetch-object --id 0x260efde76ebccf57f4c5e951157f5c361cde822c \
        ///      --genesis $HOME/.sui/sui_config/genesis.blob \
        ///      --history --verbosity concise --concise-no-header
        /// ```
        #[clap(
            value_enum,
            long = "verbosity",
            default_value = "grouped",
            ignore_case = true
        )]
        verbosity: Verbosity,

        #[clap(
            long = "concise-no-header",
            help = "don't show header in concise output"
        )]
        concise_no_header: bool,
    },

    /// Fetch the effects association with transaction `digest`
    #[clap(name = "fetch-transaction")]
    FetchTransaction {
        #[clap(long = "genesis")]
        genesis: PathBuf,

        #[clap(long, help = "The transaction ID to fetch")]
        digest: TransactionDigest,

        /// If true, show the input transaction as well as the effects
        #[clap(long = "show-tx")]
        show_input_tx: bool,
    },

    /// Tool to read validator & node db.
    #[clap(name = "db-tool")]
    DbTool {
        /// Path of the DB to read
        #[clap(long = "db-path")]
        db_path: String,
        #[clap(subcommand)]
        cmd: Option<DbToolCommand>,
    },

    #[clap(name = "dump-validators")]
    DumpValidators {
        #[clap(long = "genesis")]
        genesis: PathBuf,

        #[clap(
            long = "concise",
            help = "show concise output - name, protocol key and network address"
        )]
        concise: bool,
    },

    #[clap(name = "dump-genesis")]
    DumpGenesis {
        #[clap(long = "genesis")]
        genesis: PathBuf,
    },

    /// Fetch authenticated checkpoint information at a specific sequence number.
    /// If sequence number is not specified, get the latest authenticated checkpoint.
    #[clap(name = "fetch-checkpoint")]
    FetchCheckpoint {
        #[clap(long = "genesis")]
        genesis: PathBuf,
        #[clap(long, help = "Fetch checkpoint at a specific sequence number")]
        sequence_number: Option<CheckpointSequenceNumber>,
    },

    #[clap(name = "anemo")]
    Anemo {
        #[clap(next_help_heading = "foo", flatten)]
        args: anemo_cli::Args,
    },

    #[clap(name = "restore-db")]
    RestoreFromDBCheckpoint {
        #[clap(long = "config-path")]
        config_path: PathBuf,
        #[clap(long = "db-checkpoint-path")]
        db_checkpoint_path: PathBuf,
    },

    #[clap(name = "invalid-sig-stress")]
    InvalidSigStress {
        #[clap(long = "fullnode-url")]
        fullnode_url: Option<String>,
        #[clap(long = "starting-tx-digest")]
        starting_tx_digest: String,
    },
}

trait OptionDebug<T> {
    fn opt_debug(&self, def_str: &str) -> String;
}
trait OptionDisplay<T> {
    fn opt_display(&self, def_str: &str) -> String;
}

impl<T> OptionDebug<T> for Option<T>
where
    T: std::fmt::Debug,
{
    fn opt_debug(&self, def_str: &str) -> String {
        match self {
            None => def_str.to_string(),
            Some(t) => format!("{:?}", t),
        }
    }
}

impl<T> OptionDisplay<T> for Option<T>
where
    T: std::fmt::Display,
{
    fn opt_display(&self, def_str: &str) -> String {
        match self {
            None => def_str.to_string(),
            Some(t) => format!("{}", t),
        }
    }
}

struct OwnerOutput(Owner);

// grep/awk-friendly output for Owner
impl std::fmt::Display for OwnerOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Owner::AddressOwner(address) => {
                write!(f, "address({})", address)
            }
            Owner::ObjectOwner(address) => {
                write!(f, "object({})", address)
            }
            Owner::Immutable => {
                write!(f, "immutable")
            }
            Owner::Shared { .. } => {
                write!(f, "shared")
            }
        }
    }
}

impl ToolCommand {
    #[allow(clippy::format_in_format_args)]
    pub async fn execute(self) -> Result<(), anyhow::Error> {
        match self {
            ToolCommand::FetchObject {
                id,
                validator,
                genesis,
                version,
                history,
                verbosity,
                concise_no_header,
            } => {
                let output = get_object(id, version, validator, genesis, history).await?;

                match verbosity {
                    Verbosity::Grouped => {
                        println!("{}", GroupedObjectOutput(output));
                    }
                    Verbosity::Verbose => {
                        println!("{}", VerboseObjectOutput(output));
                    }
                    Verbosity::Concise => {
                        if !concise_no_header {
                            println!("{}", ConciseObjectOutput::header());
                        }
                        println!("{}", ConciseObjectOutput(output));
                    }
                }
            }
            ToolCommand::FetchTransaction {
                genesis,
                digest,
                show_input_tx,
            } => {
                print!(
                    "{}",
                    get_transaction_block(digest, genesis, show_input_tx).await?
                );
            }
            ToolCommand::DbTool { db_path, cmd } => {
                let path = PathBuf::from(db_path);
                match cmd {
                    Some(c) => execute_db_tool_command(path, c)?,
                    None => print_db_all_tables(path)?,
                }
            }
            ToolCommand::DumpValidators { genesis, concise } => {
                let genesis = Genesis::load(genesis).unwrap();
                if !concise {
                    println!("{:#?}", genesis.validator_set_for_tooling());
                } else {
                    for (i, val_info) in genesis.validator_set_for_tooling().iter().enumerate() {
                        let metadata = val_info.verified_metadata();
                        println!(
                            "#{:<2} {:<20} {:?<66} {:?} {}",
                            i,
                            metadata.name,
                            metadata.protocol_pubkey,
                            metadata.net_address,
                            anemo::PeerId(metadata.network_pubkey.0.to_bytes()),
                        )
                    }
                }
            }
            ToolCommand::DumpGenesis { genesis } => {
                let genesis = Genesis::load(genesis)?;
                println!("{:#?}", genesis);
            }
            ToolCommand::FetchCheckpoint {
                genesis,
                sequence_number,
            } => {
                let clients = make_clients(genesis)?;

                for (name, (_, client)) in clients {
                    let resp = client
                        .handle_checkpoint(CheckpointRequest {
                            sequence_number,
                            request_content: true,
                        })
                        .await
                        .unwrap();
                    let CheckpointResponse {
                        checkpoint,
                        contents,
                    } = resp;
                    println!("Validator: {:?}\n", name.concise());
                    println!("Checkpoint: {:?}\n", checkpoint);
                    println!("Content: {:?}\n", contents);
                }
            }
            ToolCommand::Anemo { args } => {
                let config = crate::make_anemo_config();
                anemo_cli::run(config, args).await
            }
            ToolCommand::RestoreFromDBCheckpoint {
                config_path,
                db_checkpoint_path,
            } => {
                let config = sui_config::NodeConfig::load(config_path)?;
                restore_from_db_checkpoint(&config, &db_checkpoint_path).await?;
            }
            ToolCommand::InvalidSigStress { fullnode_url, starting_tx_digest} => {
                let fullnode_url = fullnode_url.unwrap_or_else(|| "https://fullnode.testnet.sui.io:443".to_string());
                let client = SuiClientBuilder::default()
                    .build(fullnode_url)
                    .await
                    .unwrap();
                let read = client.read_api().clone();
                let quorum_driver = client.quorum_driver();
                let sui_system_state = client.governance_api().get_latest_sui_system_state().await?;
                let committee = sui_system_state.get_sui_committee_for_benchmarking();
                // println!("Committee: {:?}", committee);
                let committee_store =
                    Arc::new(CommitteeStore::new_for_testing(&committee.committee));
                let _ = committee_store
                    .insert_new_committee(&committee.committee)
                    .unwrap();
                let registry = prometheus::Registry::new();
                let metrics = SafeClientMetricsBase::new(&registry);
                let metrics2 = AuthAggMetrics::new(&registry);
                let agg = Arc::new(AuthorityAggregator::new_from_committee(
                    committee,
                    &committee_store,
                    metrics,
                    metrics2,
                )?);

                let (tx, mut rx) = mpsc::channel(10_000);
                let cursor = Some(TransactionDigest::from_str(&starting_tx_digest).unwrap());
                tokio::spawn(query_function(
                    read.clone(),
                    "mint_test_token_usdt",
                    cursor,
                    tx.clone(),
                ));
                tokio::spawn(query_function(
                    read.clone(),
                    "mint_test_token_usdc",
                    cursor,
                    tx.clone(),
                ));
                tokio::spawn(query_function(
                    read.clone(),
                    "mint_test_token_eth",
                    cursor,
                    tx.clone(),
                ));
                tokio::spawn(query_function(
                    read.clone(),
                    "mint_test_token_btc",
                    cursor,
                    tx.clone(),
                ));
                tokio::spawn(query_function(
                    read.clone(),
                    "mint_test_token_dai",
                    cursor,
                    tx.clone(),
                ));
                tokio::spawn(query_function(
                    read.clone(),
                    "mint_test_token_sol",
                    cursor,
                    tx.clone(),
                ));
                tokio::spawn(query_function(
                    read.clone(),
                    "mint_test_token_btc",
                    cursor,
                    tx.clone(),
                ));

                loop {
                    let tx_digests = rx.recv().await.unwrap();
                    let tasks = tx_digests.iter().map(|tx_digest| {
                        println!("Fetching transaction: {:?}", tx_digest);
                        let agg_clone = agg.clone();
                        async move { 
                            let tx = agg_clone.fetch_transaction(*tx_digest, None).await;
                            if tx.is_err() {
                                println!("Error fetching transaction: {:?}", tx_digest);
                                return;
                            }
                            let tx = tx.unwrap();
                            let _ = quorum_driver.execute_transaction_block(tx, SuiTransactionBlockResponseOptions::default(), None).await;
                            println!("Done executing transaction: {:?}", tx_digest);
                        }
                    }).collect::<Vec<_>>();
                    let _ = futures::future::join_all(tasks).await;
                }
            }
        };
        Ok(())
    }
}


async fn query_function(read: ReadApi, function_name: &str, mut cursor: Option<TransactionDigest>, tx: mpsc::Sender<Vec<TransactionDigest>>) {
    loop {
        let page = read.query_transaction_blocks(
            SuiTransactionBlockResponseQuery::new_with_filter(
                sui_types::query::TransactionFilter::MoveFunction { 
                    package: ObjectID::from_hex_literal("0xe158e6df182971bb6c85eb9de9fbfb460b68163d19afc45873c8672b5cc521b2").unwrap(),
                    module: Some("TOKEN".to_string()),
                    function: Some(function_name.to_string()),
                },
            ),
            cursor,
            Some(100),
            false,
        ).await;
        if page.is_err() {
            println!("Error fetcing transactions: {:?}", page);
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            continue;
        }
        let page = page.unwrap();
        cursor = page.next_cursor;
        println!("function {} query result size: {}, new cursor: {:?}", function_name, page.data.len(), cursor);
        tx.send(page.data.iter().map(|resp| resp.digest).collect()).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    }
}