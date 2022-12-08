// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::db_tool::{execute_db_tool_command, print_db_all_tables, DbToolCommand};
use anyhow::Result;
use futures::future::join_all;
use std::cmp::min;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use sui_config::genesis::Genesis;
use sui_network::default_mysten_network_config;

use sui_core::authority_client::{
    AuthorityAPI, NetworkAuthorityClient, NetworkAuthorityClientMetrics,
};
use sui_types::{base_types::*, batch::*, messages::*, object::Owner};

use anyhow::anyhow;
use futures::stream::StreamExt;

use clap::*;
use sui_core::authority::MAX_ITEMS_LIMIT;
use sui_types::messages_checkpoint::{
    CheckpointRequest, CheckpointResponse, CheckpointSequenceNumber,
};
use sui_types::object::ObjectFormatOptions;

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

        /// Concise mode prints tabular output suitable for processing with unix tools. For
        /// instance, to quickly check that all validators agree on the history of an object:
        ///
        ///     $ sui-tool fetch-object --id 0x260efde76ebccf57f4c5e951157f5c361cde822c \
        ///         --genesis $HOME/.sui/sui_config/genesis.blob \
        ///         --history --concise   --no-header \
        ///         | sort -k2 -k1 \
        ///         | uniq -f1 -c
        ///
        /// (Prints full history in concise mode, suppresses header, sorts by version and then
        /// validator name, uniqs and counts lines ignoring validator name.)
        #[clap(long = "concise", help = "show concise output")]
        concise: bool,

        #[clap(long = "no-header", help = "don't show header in concise output")]
        no_header: bool,
    },

    #[clap(name = "fetch-transaction")]
    FetchTransaction {
        #[clap(long = "genesis")]
        genesis: PathBuf,

        #[clap(long, help = "The object ID to fetch")]
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

fn make_clients(genesis: &Genesis) -> Result<BTreeMap<AuthorityName, NetworkAuthorityClient>> {
    let net_config = default_mysten_network_config();

    let mut authority_clients = BTreeMap::new();

    for validator in genesis.validator_set() {
        let channel = net_config
            .connect_lazy(&validator.network_address)
            .map_err(|err| anyhow!(err.to_string()))?;
        let client = NetworkAuthorityClient::new(
            channel,
            Arc::new(NetworkAuthorityClientMetrics::new_for_tests()),
        );
        let public_key_bytes = validator.protocol_key();
        authority_clients.insert(public_key_bytes, client);
    }

    Ok(authority_clients)
}

type ObjectVersionResponses = Vec<(Option<SequenceNumber>, Result<ObjectInfoResponse>)>;
struct ObjectData {
    requested_id: ObjectID,
    responses: Vec<(AuthorityName, ObjectVersionResponses)>,
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

struct ConciseObjectOutput(ObjectData);

impl ConciseObjectOutput {
    fn header() -> String {
        format!(
            "{:<66} {:<8} {:<66} {:<45} {}",
            "validator", "version", "digest", "parent_cert", "owner"
        )
    }
}

impl std::fmt::Display for ConciseObjectOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (name, versions) in &self.0.responses {
            for (version, resp) in versions {
                write!(
                    f,
                    "{:<66} {:<8}",
                    format!("{:?}", name),
                    version.map(|s| s.value()).opt_debug("-")
                )?;
                match resp {
                    Err(_) => writeln!(
                        f,
                        "{:<66} {:<45} {:<51}",
                        "object-fetch-failed", "no-cert-available", "no-owner-available"
                    )?,
                    Ok(resp) => {
                        let objref = resp
                            .requested_object_reference
                            .map(|(_, _, digest)| digest)
                            .opt_debug("objref-not-available");
                        let cert = resp
                            .parent_certificate
                            .as_ref()
                            .map(|c| *c.digest())
                            .opt_debug("<genesis>");
                        let owner = resp
                            .object_and_lock
                            .as_ref()
                            .map(|o| OwnerOutput(o.object.owner))
                            .opt_display("no-owner-available");
                        write!(f, " {:<66} {:<45} {:<51}", objref, cert, owner)?;
                    }
                }
                writeln!(f)?;
            }
        }
        Ok(())
    }
}

struct VerboseObjectOutput(ObjectData);

impl std::fmt::Display for VerboseObjectOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Object: {}", self.0.requested_id)?;

        for (name, versions) in &self.0.responses {
            writeln!(f, "validator: {:?}", name)?;

            for (version, resp) in versions {
                writeln!(
                    f,
                    "-- version: {}",
                    version.opt_debug("<version not available>")
                )?;

                match resp {
                    Err(e) => writeln!(f, "Error fetching object: {}", e)?,
                    Ok(resp) => {
                        let objref = resp.requested_object_reference.opt_debug("<no object>");
                        writeln!(f, "  -- ref: {}", objref)?;

                        write!(f, "  -- cert:")?;
                        match &resp.parent_certificate {
                            None => writeln!(f, " <genesis>")?,
                            Some(cert) => {
                                let cert = format!("{}", cert);
                                let cert = textwrap::indent(&cert, "     | ");
                                write!(f, "\n{}", cert)?;
                            }
                        }

                        if let Some(ObjectResponse {
                            lock,
                            object,
                            layout,
                        }) = &resp.object_and_lock
                        {
                            if object.is_package() {
                                writeln!(f, "  -- object: <Move Package>")?;
                            } else if let Some(layout) = layout {
                                writeln!(
                                    f,
                                    "  -- object: Move Object: {}",
                                    object
                                        .data
                                        .try_as_move()
                                        .unwrap()
                                        .to_move_struct(layout)
                                        .unwrap()
                                )?;
                            }
                            writeln!(f, "  -- owner: {}", object.owner)?;
                            writeln!(f, "  -- locked by: {}", lock.opt_debug("<not locked>"))?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

async fn get_object(
    client: &NetworkAuthorityClient,
    id: ObjectID,
    start_version: Option<u64>,
    full_history: bool,
) -> Vec<(Option<SequenceNumber>, Result<ObjectInfoResponse>)> {
    let mut ret = Vec::new();
    let mut version = start_version;

    loop {
        let resp = client
            .handle_object_info_request(ObjectInfoRequest {
                object_id: id,
                request_kind: match version {
                    None => ObjectInfoRequestKind::LatestObjectInfo(Some(
                        ObjectFormatOptions::default(),
                    )),
                    Some(v) => ObjectInfoRequestKind::PastObjectInfoDebug(
                        SequenceNumber::from_u64(v),
                        Some(ObjectFormatOptions::default()),
                    ),
                },
            })
            .await
            .map_err(anyhow::Error::from);

        let resp_version = resp
            .as_ref()
            .ok()
            .and_then(|r| r.requested_object_reference)
            .map(|(_, v, _)| v.value());
        ret.push((resp_version.map(SequenceNumber::from), resp));

        version = match (version, resp_version) {
            (Some(v), _) | (None, Some(v)) => {
                if v == 0 || !full_history {
                    break;
                } else {
                    Some(v - 1)
                }
            }
            _ => break,
        };
    }

    ret
}

async fn handle_batch(client: &dyn AuthorityAPI, req: &BatchInfoRequest) {
    let mut streamx = Box::pin(client.handle_batch_stream(req.clone()).await.unwrap());

    while let Some(item) = streamx.next().await {
        match item {
            Ok(BatchInfoResponseItem(UpdateItem::Batch(signed_batch))) => {
                println!("batch: {:?}", signed_batch);
            }

            Ok(BatchInfoResponseItem(UpdateItem::Transaction((seq, digests)))) => {
                println!("item: {:?}, {:?}", seq, digests);
            }

            // Return any errors.
            Err(err) => {
                println!("error: {}", err);
            }
        }
    }
}

impl ToolCommand {
    pub async fn execute(self) -> Result<(), anyhow::Error> {
        match self {
            ToolCommand::BatchStream {
                seq,
                validator,
                genesis,
                len,
            } => {
                let genesis = Genesis::load(genesis)?;
                let clients = make_clients(&genesis)?;

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

                for (name, c) in clients.iter() {
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
                            handle_batch(*c, &request).await;
                        }
                    } else {
                        let req = BatchInfoRequest {
                            start: seq,
                            length: len,
                        };
                        handle_batch(*c, &req).await;
                    }
                }
            }
            ToolCommand::FetchObject {
                id,
                validator,
                genesis,
                version,
                history,
                concise,
                no_header,
            } => {
                let genesis = Genesis::load(genesis)?;
                let clients = make_clients(&genesis)?;

                let responses = join_all(
                    clients
                        .iter()
                        .filter(|(name, _)| {
                            if let Some(v) = validator {
                                v == **name
                            } else {
                                true
                            }
                        })
                        .map(|(name, client)| async {
                            let object_versions = get_object(client, id, version, history).await;
                            (*name, object_versions)
                        }),
                )
                .await;

                let output = ObjectData {
                    requested_id: id,
                    responses,
                };

                if concise {
                    if !no_header {
                        println!("{}", ConciseObjectOutput::header());
                    }
                    print!("{}", ConciseObjectOutput(output));
                } else {
                    println!("{}", VerboseObjectOutput(output));
                }
            }
            ToolCommand::FetchTransaction { genesis, digest } => {
                let genesis = Genesis::load(genesis)?;
                let clients = make_clients(&genesis)?;

                let responses = join_all(clients.iter().map(|(name, client)| async {
                    let result = client
                        .handle_transaction_info_request(TransactionInfoRequest {
                            transaction_digest: digest,
                        })
                        .await;
                    (*name, result)
                }))
                .await;
                println!("{:#?}", responses);
            }
            ToolCommand::DbTool { db_path, cmd } => {
                let path = PathBuf::from(db_path);
                match cmd {
                    Some(c) => execute_db_tool_command(path, c)?,
                    None => print_db_all_tables(path)?,
                }
            }
            ToolCommand::DumpValidators { genesis } => {
                let genesis = Genesis::load(genesis)?;
                println!("{:#?}", genesis.validator_set());
            }
            ToolCommand::DumpGenesis { genesis } => {
                let genesis = Genesis::load(genesis)?;
                println!("{:#?}", genesis);
            }
            ToolCommand::FetchAuthenticatedCheckpoint {
                genesis,
                sequence_number,
            } => {
                let genesis = Genesis::load(genesis)?;
                let clients = make_clients(&genesis)?;
                let committee = genesis.committee()?;

                for (name, client) in clients {
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
                        _ => unreachable!(),
                    }
                }
            }
        };
        Ok(())
    }
}
