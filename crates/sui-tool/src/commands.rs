// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    db_tool::{execute_db_tool_command, print_db_all_tables, DbToolCommand},
    get_object, get_transaction_block, make_clients, restore_from_db_checkpoint,
    state_sync_from_archive, verify_archive, verify_archive_by_checksum, ConciseObjectOutput,
    GroupedObjectOutput, VerboseObjectOutput,
};
use anyhow::Result;
use std::path::PathBuf;
use sui_config::genesis::Genesis;
use sui_core::authority_client::AuthorityAPI;
use sui_replay::{execute_replay_command, ReplayToolCommand};

use sui_types::{base_types::*, object::Owner};

use clap::*;
use fastcrypto::encoding::Encoding;
use sui_config::Config;
use sui_core::authority_aggregator::AuthorityAggregatorBuilder;
use sui_storage::object_store::ObjectStoreConfig;
use sui_types::messages_checkpoint::{
    CheckpointRequest, CheckpointResponse, CheckpointSequenceNumber,
};
use sui_types::transaction::{SenderSignedData, Transaction};

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

        // At least one of genesis or fullnode_rpc_url must be provided
        #[clap(long = "genesis")]
        genesis: Option<PathBuf>,

        // At least one of genesis or fullnode_rpc_url must be provided
        // RPC address to provide the up-to-date committee info
        #[clap(long = "fullnode-rpc-url")]
        fullnode_rpc_url: Option<String>,

        /// Concise mode groups responses by results.
        /// prints tabular output suitable for processing with unix tools. For
        /// instance, to quickly check that all validators agree on the history of an object:
        /// ```text
        /// $ sui-tool fetch-object --id 0x260efde76ebccf57f4c5e951157f5c361cde822c \
        ///      --genesis $HOME/.sui/sui_config/genesis.blob \
        ///      --verbosity concise --concise-no-header
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
        // At least one of genesis or fullnode_rpc_url must be provided
        #[clap(long = "genesis")]
        genesis: Option<PathBuf>,

        // At least one of genesis or fullnode_rpc_url must be provided
        // RPC address to provide the up-to-date committee info
        #[clap(long = "fullnode-rpc-url")]
        fullnode_rpc_url: Option<String>,

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

    /// Tool to sync the node from archive store
    #[clap(name = "sync-from-archive")]
    SyncFromArchive {
        #[clap(long = "genesis")]
        genesis: PathBuf,
        #[clap(long = "db-path")]
        db_path: PathBuf,
        #[clap(flatten)]
        object_store_config: ObjectStoreConfig,
        #[clap(default_value_t = 5)]
        download_concurrency: usize,
    },

    /// Tool to verify the archive store
    #[clap(name = "verify-archive")]
    VerifyArchive {
        #[clap(long = "genesis")]
        genesis: PathBuf,
        #[clap(flatten)]
        object_store_config: ObjectStoreConfig,
        #[clap(default_value_t = 5)]
        download_concurrency: usize,
    },

    /// Tool to verify the archive store by comparing file checksums
    #[clap(name = "verify-archive-from-checksums")]
    VerifyArchiveByChecksum {
        #[clap(flatten)]
        object_store_config: ObjectStoreConfig,
        #[clap(default_value_t = 5)]
        download_concurrency: usize,
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
        // At least one of genesis or fullnode_rpc_url must be provided
        #[clap(long = "genesis")]
        genesis: Option<PathBuf>,

        // At least one of genesis or fullnode_rpc_url must be provided
        // RPC address to provide the up-to-date committee info
        #[clap(long = "fullnode-rpc-url")]
        fullnode_rpc_url: Option<String>,

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

    #[clap(name = "replay")]
    Replay {
        #[clap(long = "rpc")]
        rpc_url: Option<String>,
        #[clap(long = "safety-checks")]
        safety_checks: bool,
        #[clap(long = "authority")]
        use_authority: bool,
        #[clap(long = "cfg-path", short)]
        cfg_path: Option<PathBuf>,
        #[clap(subcommand)]
        cmd: ReplayToolCommand,
    },

    /// Ask all validators to sign a transaction through AuthorityAggregator.
    #[clap(name = "sign-transaction")]
    SignTransaction {
        #[clap(long = "genesis")]
        genesis: PathBuf,

        #[clap(
            long,
            help = "The Base64-encoding of the bcs bytes of SenderSignedData"
        )]
        sender_signed_data: String,
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
                fullnode_rpc_url,
                verbosity,
                concise_no_header,
            } => {
                let output = get_object(id, version, validator, genesis, fullnode_rpc_url).await?;

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
                fullnode_rpc_url,
            } => {
                print!(
                    "{}",
                    get_transaction_block(digest, genesis, show_input_tx, fullnode_rpc_url).await?
                );
            }
            ToolCommand::DbTool { db_path, cmd } => {
                let path = PathBuf::from(db_path);
                match cmd {
                    Some(c) => execute_db_tool_command(path, c).await?,
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
                            "#{:<2} {:<20} {:?} {:?} {}",
                            i,
                            metadata.name,
                            metadata.sui_pubkey_bytes().concise(),
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
                fullnode_rpc_url,
            } => {
                let clients = make_clients(genesis, fullnode_rpc_url).await?;

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
            ToolCommand::Replay {
                rpc_url,
                safety_checks,
                cmd,
                use_authority,
                cfg_path,
            } => {
                execute_replay_command(rpc_url, safety_checks, use_authority, cfg_path, cmd)
                    .await?;
            }
            ToolCommand::SyncFromArchive {
                genesis,
                db_path,
                object_store_config,
                download_concurrency,
            } => {
                state_sync_from_archive(
                    &db_path,
                    &genesis,
                    object_store_config,
                    download_concurrency,
                )
                .await?;
            }
            ToolCommand::VerifyArchive {
                genesis,
                object_store_config,
                download_concurrency,
            } => {
                verify_archive(&genesis, object_store_config, download_concurrency, true).await?;
            }
            ToolCommand::VerifyArchiveByChecksum {
                object_store_config,
                download_concurrency,
            } => {
                verify_archive_by_checksum(object_store_config, download_concurrency).await?;
            }
            ToolCommand::SignTransaction {
                genesis,
                sender_signed_data,
            } => {
                let genesis = Genesis::load(genesis)?;
                let sender_signed_data = bcs::from_bytes::<SenderSignedData>(
                    &fastcrypto::encoding::Base64::decode(sender_signed_data.as_str()).unwrap(),
                )
                .unwrap();
                let transaction = Transaction::new(sender_signed_data);
                let (agg, _) = AuthorityAggregatorBuilder::from_genesis(&genesis)
                    .build()
                    .unwrap();
                let result = agg.process_transaction(transaction).await;
                println!("{:?}", result);
            }
        };
        Ok(())
    }
}
