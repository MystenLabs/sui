// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    db_tool::{execute_db_tool_command, print_db_all_tables, DbToolCommand},
    get_transaction,
};
use anyhow::Result;
use std::cmp::min;
use std::path::PathBuf;
use sui_config::genesis::Genesis;

use sui_core::authority_client::AuthorityAPI;
use sui_types::{base_types::*, messages::*};

use crate::{
    get_object, handle_batch, make_clients, ConciseObjectOutput, GrouppedObjectOutput,
    VerboseObjectOutput,
};
use clap::*;
use sui_core::authority::MAX_ITEMS_LIMIT;
use sui_types::messages_checkpoint::{
    CheckpointRequest, CheckpointResponse, CheckpointSequenceNumber,
};

#[derive(Parser, Clone, ArgEnum)]
pub enum Verbosity {
    Groupped,
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
        ///     ```
        ///     cli-tool fetch-object --id 0x260efde76ebccf57f4c5e951157f5c361cde822c \
        ///         --genesis $HOME/.sui/sui_config/genesis.blob \
        ///         --history --verbosity concise --concise-no-header
        ///     ```
        ///
        #[clap(
            arg_enum,
            long = "verbosity",
            default_value = "groupped",
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

        #[clap(long, help = "The transaction ID to fetch")]
        digest: TransactionDigest,
    },
    /// Tool to read validator & gateway db.
    #[clap(name = "db-tool")]
    DbTool {
        /// Path of the DB to read
        #[clap(long = "db-path")]
        db_path: String,
        #[clap(subcommand)]
        cmd: Option<DbToolCommand>,
    },

    /// Pull down the batch stream for a validator(s).
    /// Note that this command currently operates sequentially, so it will block on the first
    /// validator indefinitely. Therefore you should generally use this with a --validator=
    /// argument.
    #[clap(name = "batch-stream")]
    BatchStream {
        #[clap(long, help = "SequenceNumber to start at")]
        seq: Option<u64>,

        #[clap(long, help = "Number of items to request", default_value_t = 1000)]
        len: u64,

        #[clap(
            long,
            help = "Validator to fetch from - if not specified, all validators are queried"
        )]
        validator: Option<AuthorityName>,

        #[clap(long = "genesis")]
        genesis: PathBuf,
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
    FetchAuthenticatedCheckpoint {
        #[clap(long = "genesis")]
        genesis: PathBuf,
        #[clap(
            long,
            help = "Fetch authenticated checkpoint at a specific sequence number"
        )]
        sequence_number: Option<CheckpointSequenceNumber>,
    },
}

impl ToolCommand {
    #[allow(clippy::format_in_format_args)]
    pub async fn execute(self) -> Result<(), anyhow::Error> {
        match self {
            ToolCommand::BatchStream {
                seq,
                validator,
                genesis,
                len,
            } => {
                let clients = make_clients(genesis)?;

                let clients: Vec<_> = clients
                    .iter()
                    .filter(|(name, _)| {
                        if let Some(v) = validator {
                            v == **name
                        } else {
                            true
                        }
                    })
                    .collect();

                for (name, (_v, c)) in clients.iter() {
                    println!("validator batch stream: {:?}", name);
                    if let Some(seq) = seq {
                        let requests =
                            (seq..(seq + len))
                                .step_by(MAX_ITEMS_LIMIT as usize)
                                .map(|start| BatchInfoRequest {
                                    start: Some(start),
                                    length: min(MAX_ITEMS_LIMIT, seq + len - start),
                                });
                        for request in requests {
                            handle_batch(c, &request).await;
                        }
                    } else {
                        let req = BatchInfoRequest {
                            start: seq,
                            length: len,
                        };
                        handle_batch(c, &req).await;
                    }
                }
            }
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
                    Verbosity::Groupped => {
                        println!("{}", GrouppedObjectOutput(output));
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
            ToolCommand::FetchTransaction { genesis, digest } => {
                print!("{}", get_transaction(digest, genesis).await?);
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
                    println!("{:#?}", genesis.validator_set());
                } else {
                    for (i, val_info) in genesis.validator_set().iter().enumerate() {
                        println!(
                            "#{:<2} {:<20} {:?<66} {:?}",
                            i,
                            val_info.name(),
                            val_info.protocol_key(),
                            val_info.network_address()
                        )
                    }
                }
            }
            ToolCommand::DumpGenesis { genesis } => {
                let genesis = Genesis::load(genesis)?;
                println!("{:#?}", genesis);
            }
            ToolCommand::FetchAuthenticatedCheckpoint {
                genesis,
                sequence_number,
            } => {
                let clients = make_clients(genesis.clone())?;
                let genesis = Genesis::load(genesis)?;
                let committee = genesis.committee()?;

                for (name, (_val, client)) in clients {
                    let resp = client
                        .handle_checkpoint(CheckpointRequest::authenticated(sequence_number, true))
                        .await
                        .unwrap();
                    println!("Validator: {:?}\n", name);
                    match resp {
                        CheckpointResponse::AuthenticatedCheckpoint {
                            checkpoint,
                            contents,
                        } => {
                            println!("Checkpoint: {:?}\n", checkpoint);
                            println!("Content: {:?}\n", contents);
                            if let Some(c) = checkpoint {
                                c.verify(&committee, contents.as_ref())?;
                            }
                        }
                    }
                }
            }
        };
        Ok(())
    }
}
