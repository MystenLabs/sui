// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    db_tool::{execute_db_tool_command, print_db_all_tables, DbToolCommand},
    download_db_snapshot, download_formal_snapshot, get_object, get_transaction_block,
    make_clients, restore_from_db_checkpoint, state_sync_from_archive, verify_archive,
    verify_archive_by_checksum, ConciseObjectOutput, GroupedObjectOutput, VerboseObjectOutput,
};
use anyhow::{anyhow, Result};
use std::env;
use std::path::PathBuf;
use sui_config::genesis::Genesis;
use sui_core::authority_client::AuthorityAPI;
use sui_protocol_config::Chain;
use sui_replay::{execute_replay_command, ReplayToolCommand};
use telemetry_subscribers::TracingHandle;

use sui_types::{base_types::*, object::Owner};

use clap::*;
use fastcrypto::encoding::Encoding;
use sui_config::Config;
use sui_core::authority_aggregator::AuthorityAggregatorBuilder;
use sui_storage::object_store::{ObjectStoreConfig, ObjectStoreType};
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
#[command(
    name = "sui-tool",
    about = "Debugging utilities for sui",
    rename_all = "kebab-case",
    author,
    version
)]
pub enum ToolCommand {
    /// Fetch the same object from all validators
    #[command(name = "fetch-object")]
    FetchObject {
        #[arg(long, help = "The object ID to fetch")]
        id: ObjectID,

        #[arg(long, help = "Fetch object at a specific sequence")]
        version: Option<u64>,

        #[arg(
            long,
            help = "Validator to fetch from - if not specified, all validators are queried"
        )]
        validator: Option<AuthorityName>,

        // At least one of genesis or fullnode_rpc_url must be provided
        #[arg(long = "genesis")]
        genesis: Option<PathBuf>,

        // At least one of genesis or fullnode_rpc_url must be provided
        // RPC address to provide the up-to-date committee info
        #[arg(long = "fullnode-rpc-url")]
        fullnode_rpc_url: Option<String>,

        /// Concise mode groups responses by results.
        /// prints tabular output suitable for processing with unix tools. For
        /// instance, to quickly check that all validators agree on the history of an object:
        /// ```text
        /// $ sui-tool fetch-object --id 0x260efde76ebccf57f4c5e951157f5c361cde822c \
        ///      --genesis $HOME/.sui/sui_config/genesis.blob \
        ///      --verbosity concise --concise-no-header
        /// ```
        #[arg(
            value_enum,
            long = "verbosity",
            default_value = "grouped",
            ignore_case = true
        )]
        verbosity: Verbosity,

        #[arg(
            long = "concise-no-header",
            help = "don't show header in concise output"
        )]
        concise_no_header: bool,
    },

    /// Fetch the effects association with transaction `digest`
    #[command(name = "fetch-transaction")]
    FetchTransaction {
        // At least one of genesis or fullnode_rpc_url must be provided
        #[arg(long = "genesis")]
        genesis: Option<PathBuf>,

        // At least one of genesis or fullnode_rpc_url must be provided
        // RPC address to provide the up-to-date committee info
        #[arg(long = "fullnode-rpc-url")]
        fullnode_rpc_url: Option<String>,

        #[arg(long, help = "The transaction ID to fetch")]
        digest: TransactionDigest,

        /// If true, show the input transaction as well as the effects
        #[arg(long = "show-tx")]
        show_input_tx: bool,
    },

    /// Tool to read validator & node db.
    #[command(name = "db-tool")]
    DbTool {
        /// Path of the DB to read
        #[arg(long = "db-path")]
        db_path: String,
        #[command(subcommand)]
        cmd: Option<DbToolCommand>,
    },

    /// Tool to sync the node from archive store
    #[command(name = "sync-from-archive")]
    SyncFromArchive {
        #[arg(long = "genesis")]
        genesis: PathBuf,
        #[arg(long = "db-path")]
        db_path: PathBuf,
        #[command(flatten)]
        object_store_config: ObjectStoreConfig,
        #[arg(default_value_t = 5)]
        download_concurrency: usize,
    },

    /// Tool to verify the archive store
    #[command(name = "verify-archive")]
    VerifyArchive {
        #[arg(long = "genesis")]
        genesis: PathBuf,
        #[command(flatten)]
        object_store_config: ObjectStoreConfig,
        #[arg(default_value_t = 5)]
        download_concurrency: usize,
    },

    /// Tool to verify the archive store by comparing file checksums
    #[command(name = "verify-archive-from-checksums")]
    VerifyArchiveByChecksum {
        #[command(flatten)]
        object_store_config: ObjectStoreConfig,
        #[arg(default_value_t = 5)]
        download_concurrency: usize,
    },

    #[command(name = "dump-validators")]
    DumpValidators {
        #[arg(long = "genesis")]
        genesis: PathBuf,

        #[arg(
            long = "concise",
            help = "show concise output - name, protocol key and network address"
        )]
        concise: bool,
    },

    #[command(name = "dump-genesis")]
    DumpGenesis {
        #[arg(long = "genesis")]
        genesis: PathBuf,
    },

    /// Fetch authenticated checkpoint information at a specific sequence number.
    /// If sequence number is not specified, get the latest authenticated checkpoint.
    #[command(name = "fetch-checkpoint")]
    FetchCheckpoint {
        // At least one of genesis or fullnode_rpc_url must be provided
        #[arg(long = "genesis")]
        genesis: Option<PathBuf>,

        // At least one of genesis or fullnode_rpc_url must be provided
        // RPC address to provide the up-to-date committee info
        #[arg(long = "fullnode-rpc-url")]
        fullnode_rpc_url: Option<String>,

        #[arg(long, help = "Fetch checkpoint at a specific sequence number")]
        sequence_number: Option<CheckpointSequenceNumber>,
    },

    #[command(name = "anemo")]
    Anemo {
        #[command(next_help_heading = "foo", flatten)]
        args: anemo_cli::Args,
    },

    #[command(name = "restore-db")]
    RestoreFromDBCheckpoint {
        #[arg(long = "config-path")]
        config_path: PathBuf,
        #[arg(long = "db-checkpoint-path")]
        db_checkpoint_path: PathBuf,
    },

    #[clap(name = "download-db-snapshot")]
    DownloadDBSnapshot {
        #[clap(long = "epoch")]
        epoch: u64,
        #[clap(long = "genesis")]
        genesis: PathBuf,
        #[clap(long = "path", default_value = "/tmp")]
        path: PathBuf,
        /// skip downloading checkpoints dir. Overridden to `true` if `--formal` flag specified
        #[clap(long = "skip-checkpoints")]
        skip_checkpoints: bool,
        /// skip downloading indexes dir. Overridden to `true` if `--formal` flag specified,
        /// as index staging is not yet supported for formal snapshots.
        #[clap(long = "skip-indexes")]
        skip_indexes: bool,
        /// Number of parallel downloads to perform. Defaults to a reasonable
        /// value based on number of available logical cores.
        #[clap(long = "num-parallel-downloads")]
        num_parallel_downloads: Option<usize>,
        /// If true, restore from formal (slim, DB agnostic) snapshot. Note that this is only supported
        /// for protocol versions supporting `commit_root_state_digest`. For mainnet, this is
        /// epoch 20+, and for testnet this is epoch 12+
        #[clap(long = "formal")]
        formal: bool,
        /// If true, perform snapshot and checkpoint summary verification. Only
        /// applicable if `--formal` flag is specified. Defaults to true.
        #[clap(long = "verify")]
        verify: Option<bool>,
        /// Network to download snapshot for. Defaults to "mainnet".
        /// If `--snapshot-bucket` or `--archive-bucket` is not specified,
        /// the value of this flag is used to construct default bucket names.
        #[clap(long = "network", default_value = "mainnet")]
        network: Chain,
        /// Snapshot bucket name. If not specified, defaults are
        /// based on value of `--network` and `--formal` flags.
        #[clap(long = "snapshot-bucket")]
        snapshot_bucket: Option<String>,
        /// Snapshot bucket type. Defaults to "gcs" if `--formal`
        /// flag specified, otherwise "s3".
        #[clap(long = "snapshot-bucket-type")]
        snapshot_bucket_type: Option<ObjectStoreType>,
        /// Path to snapshot directory on local filesystem.
        /// Only applicable if `--snapshot-bucket-type` is "file".
        #[clap(long = "snapshot-path")]
        snapshot_path: Option<PathBuf>,
        /// Archival bucket name. If not specified, defaults are
        /// based on value of `--network` and `--formal` flags.
        #[clap(long = "archive-bucket")]
        archive_bucket: Option<String>,
        #[clap(long = "archive-bucket-type", default_value = "s3")]
        archive_bucket_type: ObjectStoreType,
        /// If false (default), log level will be overridden to "off",
        /// and output will be reduced to necessary status information.
        #[clap(long = "formal")]
        verbose: bool,
    },

    #[clap(name = "replay")]
    Replay {
        #[arg(long = "rpc")]
        rpc_url: Option<String>,
        #[arg(long = "safety-checks")]
        safety_checks: bool,
        #[arg(long = "authority")]
        use_authority: bool,
        #[arg(long = "cfg-path", short)]
        cfg_path: Option<PathBuf>,
        #[command(subcommand)]
        cmd: ReplayToolCommand,
    },

    /// Ask all validators to sign a transaction through AuthorityAggregator.
    #[command(name = "sign-transaction")]
    SignTransaction {
        #[arg(long = "genesis")]
        genesis: PathBuf,

        #[arg(
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
    pub async fn execute(self, tracing_handle: TracingHandle) -> Result<(), anyhow::Error> {
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
            ToolCommand::DownloadDBSnapshot {
                epoch,
                genesis,
                path,
                skip_checkpoints,
                skip_indexes,
                num_parallel_downloads,
                formal,
                verify,
                network,
                snapshot_bucket,
                snapshot_bucket_type,
                snapshot_path,
                archive_bucket,
                archive_bucket_type,
                verbose,
            } => {
                if !verbose {
                    tracing_handle
                        .update_log("off")
                        .expect("Failed to update log level");
                }
                let num_parallel_downloads = num_parallel_downloads.unwrap_or_else(|| {
                    num_cpus::get()
                        .checked_sub(1)
                        .expect("Failed to get number of CPUs")
                });
                let snapshot_bucket = snapshot_bucket.unwrap_or_else(|| match (formal, network) {
                    (true, Chain::Mainnet) => "mysten-mainnet-formal".to_string(),
                    (false, Chain::Mainnet) => "mysten-mainnet-snapshots".to_string(),
                    (true, Chain::Testnet) => "mysten-testnet-formal".to_string(),
                    (false, Chain::Testnet) => "mysten-testnet-snapshots".to_string(),
                    (_, Chain::Unknown) => {
                        panic!("Cannot generate default snapshot bucket for unknown network");
                    }
                });
                let archive_bucket = archive_bucket.unwrap_or_else(|| match network {
                    Chain::Mainnet => "mysten-mainnet-archives".to_string(),
                    Chain::Testnet => "mysten-testnet-archives".to_string(),
                    Chain::Unknown => {
                        panic!("Cannot generate default archive bucket for unknown network");
                    }
                });
                let snapshot_bucket_type = snapshot_bucket_type.unwrap_or({
                    if formal {
                        ObjectStoreType::GCS
                    } else {
                        ObjectStoreType::S3
                    }
                });

                // index staging is not yet supported for formal snapshots
                let skip_indexes = skip_indexes || formal;
                // Checkpoint db does not exist in formal snapshots and
                // is not reconstructed during formal snapshot restore
                let skip_checkpoints = skip_checkpoints || formal;

                let snapshot_store_config = match snapshot_bucket_type {
                    ObjectStoreType::S3 => {
                        ObjectStoreConfig {
                            object_store: Some(ObjectStoreType::S3),
                            bucket: Some(snapshot_bucket),
                            aws_access_key_id: Some(env::var(
                                "AWS_SNAPSHOT_ACCESS_KEY_ID",
                            ).map_err(|_| anyhow!("Please provide AWS_SNAPSHOT_ACCESS_KEY_ID as env variable"))?),
                            aws_secret_access_key: Some(env::var(
                                "AWS_SNAPSHOT_SECRET_ACCESS_KEY",
                            ).map_err(|_| anyhow!("Please provide AWS_SNAPSHOT_SECRET_ACCESS_KEY as env variable"))?),
                            aws_region: Some(env::var(
                                "AWS_SNAPSHOT_REGION",
                            ).map_err(|_| anyhow!("Please provide AWS_SNAPSHOT_REGION as env variable"))?),
                            object_store_connection_limit: 200,
                            ..Default::default()
                        }
                    },
                    ObjectStoreType::GCS => {
                        ObjectStoreConfig {
                            object_store: Some(ObjectStoreType::GCS),
                            bucket: Some(snapshot_bucket),
                            google_service_account: Some(env::var(
                                "GCS_SNAPSHOT_SERVICE_ACCOUNT_FILE_PATH",
                            ).map_err(|_| anyhow!("Please provide GCS_SNAPSHOT_SERVICE_ACCOUNT_FILE_PATH as env variable"))?),
                            object_store_connection_limit: 200,
                            ..Default::default()
                        }
                    },
                    ObjectStoreType::Azure => {
                        ObjectStoreConfig {
                            object_store: Some(ObjectStoreType::Azure),
                            bucket: Some(snapshot_bucket),
                            azure_storage_account: Some(env::var(
                                "AZURE_SNAPSHOT_STORAGE_ACCOUNT",
                            ).map_err(|_| anyhow!("Please provide AZURE_SNAPSHOT_STORAGE_ACCOUNT as env variable"))?),
                            azure_storage_access_key: Some(env::var(
                                "AZURE_SNAPSHOT_STORAGE_ACCESS_KEY",
                            ).map_err(|_| anyhow!("Please provide AZURE_SNAPSHOT_STORAGE_ACCESS_KEY as env variable"))?),
                            object_store_connection_limit: 200,
                            ..Default::default()
                        }
                    },
                    ObjectStoreType::File => {
                        if snapshot_path.is_some() {
                            ObjectStoreConfig {
                                object_store: Some(ObjectStoreType::File),
                                directory: snapshot_path,
                                ..Default::default()
                            }
                        } else {
                            panic!("--snapshot-path must be specified for --snapshot-bucket-type=file");
                        }
                    }
                };

                let archive_store_config = match archive_bucket_type {
                    ObjectStoreType::S3 => {
                        ObjectStoreConfig {
                            object_store: Some(ObjectStoreType::S3),
                            bucket: Some(archive_bucket),
                            aws_access_key_id: Some(env::var(
                                "AWS_ARCHIVE_ACCESS_KEY_ID",
                            ).map_err(|_| anyhow!("Please provide AWS_ARCHIVE_ACCESS_KEY_ID as env variable"))?),
                            aws_secret_access_key: Some(env::var(
                                "AWS_ARCHIVE_SECRET_ACCESS_KEY",
                            ).map_err(|_| anyhow!("Please provide AWS_ARCHIVE_SECRET_ACCESS_KEY as env variable"))?),
                            aws_region: Some(env::var(
                                "AWS_ARCHIVE_REGION",
                            ).map_err(|_| anyhow!("Please provide AWS_ARCHIVE_REGION as env variable"))?),
                            object_store_connection_limit: 200,
                            ..Default::default()
                        }
                    },
                    ObjectStoreType::GCS => {
                        ObjectStoreConfig {
                            object_store: Some(ObjectStoreType::GCS),
                            bucket: Some(archive_bucket),
                            google_service_account: Some(env::var(
                                "GCS_ARCHIVE_SERVICE_ACCOUNT_FILE_PATH",
                            ).map_err(|_| anyhow!("Please provide GCS_ARCHIVE_SERVICE_ACCOUNT_FILE_PATH as env variable"))?),
                            object_store_connection_limit: 200,
                            ..Default::default()
                        }
                    },
                    ObjectStoreType::Azure => {
                        ObjectStoreConfig {
                            object_store: Some(ObjectStoreType::Azure),
                            bucket: Some(archive_bucket),
                            azure_storage_account: Some(env::var(
                                "AZURE_ARCHIVE_STORAGE_ACCOUNT",
                            ).map_err(|_| anyhow!("Please provide AZURE_ARCHIVE_STORAGE_ACCOUNT as env variable"))?),
                            azure_storage_access_key: Some(env::var(
                                "AZURE_ARCHIVE_STORAGE_ACCESS_KEY",
                            ).map_err(|_| anyhow!("Please provide AZURE_ARCHIVE_STORAGE_ACCESS_KEY as env variable"))?),
                            object_store_connection_limit: 200,
                            ..Default::default()
                        }
                    },
                    ObjectStoreType::File => panic!("Download from local filesystem is not supported")
                };

                if formal {
                    let verify = verify.unwrap_or(true);
                    download_formal_snapshot(
                        &path,
                        epoch,
                        &genesis,
                        snapshot_store_config,
                        archive_store_config,
                        num_parallel_downloads,
                        network,
                        verify,
                    )
                    .await?;
                } else {
                    download_db_snapshot(
                        &path,
                        epoch,
                        &genesis,
                        snapshot_store_config,
                        archive_store_config,
                        skip_checkpoints,
                        skip_indexes,
                        num_parallel_downloads,
                    )
                    .await?;
                }
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
