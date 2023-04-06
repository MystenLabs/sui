// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    db_tool::{execute_db_tool_command, print_db_all_tables, DbToolCommand},
    fetch_causal_history, get_object, get_transaction_block, make_clients, replay_transactions,
    restore_from_db_checkpoint, CausalHistory, ConciseObjectOutput, GroupedObjectOutput,
    VerboseObjectOutput,
};
use anyhow::Result;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use sui_config::genesis::Genesis;
use sui_core::authority_client::AuthorityAPI;

use sui_types::{base_types::*, digests::TransactionEffectsDigest, object::Owner};

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

    #[clap(name = "fetch-transaction")]
    FetchTransaction {
        #[clap(long = "genesis")]
        genesis: PathBuf,

        #[clap(long, help = "Fetch data from a local db")]
        read_from_db: Option<PathBuf>,

        #[clap(long, help = "Where to serialize output to")]
        output_file: Option<PathBuf>,

        #[clap(long, help = "The transaction ID to fetch")]
        digest: TransactionDigest,

        #[clap(
            long,
            help = "Optionally verify that transaction effects match this effects digest"
        )]
        fx_digest: Option<TransactionEffectsDigest>,

        #[clap(long, help = "Fetch entire causal history of transaction")]
        causal_history: bool,
    },

    #[clap(name = "replay-transactions")]
    ReplayTransactions {
        #[clap(long = "transactions")]
        transactions: PathBuf,

        #[clap(long = "working-dir")]
        working_dir: PathBuf,
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
                fx_digest,
                causal_history,
                read_from_db,
                output_file,
            } => {
                if causal_history {
                    let history =
                        fetch_causal_history(digest, fx_digest, genesis, read_from_db).await?;

                    if let Some(output_file) = output_file {
                        // serialize output to disk using bcs
                        let mut file = File::create(output_file)?;
                        bcs::serialize_into(&mut file, &history)?;
                    }
                } else {
                    print!("{}", get_transaction_block(digest, genesis).await?);
                }
            }
            ToolCommand::ReplayTransactions {
                transactions: transactions_path,
                working_dir,
            } => {
                let address_map_path = working_dir.join("addressmap.bcs");

                // load and deserialize transactions_path as a CausalHistory instance using bcs
                fn load_bcs<T: DeserializeOwned>(
                    path: impl AsRef<Path>,
                ) -> Result<T, anyhow::Error> {
                    let bytes = std::fs::read(path)?;
                    Ok(bcs::from_bytes(&bytes)?)
                }

                let address_map =
                    load_bcs::<BTreeMap<SuiAddress, SuiAddress>>(&address_map_path).unwrap();

                let transactions = load_bcs::<CausalHistory>(&transactions_path).unwrap();

                replay_transactions(transactions, address_map, working_dir).await;
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
        };
        Ok(())
    }
}
