// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    check_completed_snapshot,
    db_tool::{execute_db_tool_command, print_db_all_tables, DbToolCommand},
    download_db_snapshot, download_formal_snapshot, dump_checkpoints_from_archive,
    get_latest_available_epoch, get_object, get_transaction_block, make_clients,
    restore_from_db_checkpoint, verify_archive, verify_archive_by_checksum, ConciseObjectOutput,
    GroupedObjectOutput, SnapshotVerifyMode, VerboseObjectOutput,
};
use anyhow::Result;
use futures::{future::join_all, StreamExt};
use std::path::PathBuf;
use std::{collections::BTreeMap, env, sync::Arc};
use sui_config::genesis::Genesis;
use sui_core::authority_client::AuthorityAPI;
use sui_protocol_config::Chain;
use sui_replay::{execute_replay_command, ReplayToolCommand};
use sui_sdk::{rpc_types::SuiTransactionBlockResponseOptions, SuiClient, SuiClientBuilder};
use telemetry_subscribers::TracingHandle;

use sui_types::{
    base_types::*, crypto::AuthorityPublicKeyBytes, messages_grpc::TransactionInfoRequest,
};

use clap::*;
use fastcrypto::encoding::Encoding;
use sui_archival::{read_manifest_as_json, write_manifest_from_json};
use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_config::Config;
use sui_core::authority_aggregator::AuthorityAggregatorBuilder;
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
pub enum ToolCommand {
    /// Inspect if a specific object is or all gas objects owned by an address are locked by validators
    #[command(name = "locked-object")]
    LockedObject {
        /// Either id or address must be provided
        /// The object to check
        #[arg(long, help = "The object ID to fetch")]
        id: Option<ObjectID>,
        /// Either id or address must be provided
        /// If provided, check all gas objects owned by this account
        #[arg(long = "address")]
        address: Option<SuiAddress>,
        /// RPC address to provide the up-to-date committee info
        #[arg(long = "fullnode-rpc-url")]
        fullnode_rpc_url: String,
        /// Should attempt to rescue the object if it's locked but not fully locked
        #[arg(long = "rescue")]
        rescue: bool,
    },

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

        // RPC address to provide the up-to-date committee info
        #[arg(long = "fullnode-rpc-url")]
        fullnode_rpc_url: String,

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
        // RPC address to provide the up-to-date committee info
        #[arg(long = "fullnode-rpc-url")]
        fullnode_rpc_url: String,

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

    /// Tool to print the archive manifest
    #[command(name = "print-archive-manifest")]
    PrintArchiveManifest {
        #[command(flatten)]
        object_store_config: ObjectStoreConfig,
    },
    /// Tool to update the archive manifest
    #[command(name = "update-archive-manifest")]
    UpdateArchiveManifest {
        #[command(flatten)]
        object_store_config: ObjectStoreConfig,
        #[arg(long = "archive-path")]
        archive_json_path: PathBuf,
    },
    /// Tool to verify the archive store by comparing file checksums
    #[command(name = "verify-archive-from-checksums")]
    VerifyArchiveByChecksum {
        #[command(flatten)]
        object_store_config: ObjectStoreConfig,
        #[arg(default_value_t = 5)]
        download_concurrency: usize,
    },

    /// Tool to print archive contents in checkpoint range
    #[command(name = "dump-archive")]
    DumpArchiveByChecksum {
        #[command(flatten)]
        object_store_config: ObjectStoreConfig,
        #[arg(default_value_t = 0)]
        start: u64,
        end: u64,
        #[arg(default_value_t = 80)]
        max_content_length: usize,
    },

    /// Download all packages to the local filesystem from a GraphQL service. Each package gets its
    /// own sub-directory, named for its ID on chain and version containing two metadata files
    /// (linkage.json and origins.json), a file containing the overall object and a file for every
    /// module it contains. Each module file is named for its module name, with a .mv suffix, and
    /// contains Move bytecode (suitable for passing into a disassembler).
    #[command(name = "dump-packages")]
    DumpPackages {
        /// Connection information for a GraphQL service.
        #[clap(long, short)]
        rpc_url: String,

        /// Path to a non-existent directory that can be created and filled with package information.
        #[clap(long, short)]
        output_dir: PathBuf,

        /// Only fetch packages that were created before this checkpoint (given by its sequence
        /// number).
        #[clap(long)]
        before_checkpoint: Option<u64>,

        /// If false (default), log level will be overridden to "off", and output will be reduced to
        /// necessary status information.
        #[clap(short, long = "verbose")]
        verbose: bool,
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
        // RPC address to provide the up-to-date committee info
        #[arg(long = "fullnode-rpc-url")]
        fullnode_rpc_url: String,

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

    #[clap(
        name = "download-db-snapshot",
        about = "Downloads the legacy database snapshot via cloud object store, outputs to local disk"
    )]
    DownloadDBSnapshot {
        #[clap(long = "epoch", conflicts_with = "latest")]
        epoch: Option<u64>,
        #[clap(
            long = "path",
            help = "the path to write the downloaded snapshot files"
        )]
        path: PathBuf,
        /// skip downloading indexes dir
        #[clap(long = "skip-indexes")]
        skip_indexes: bool,
        /// Number of parallel downloads to perform. Defaults to a reasonable
        /// value based on number of available logical cores.
        #[clap(long = "num-parallel-downloads")]
        num_parallel_downloads: Option<usize>,
        /// Network to download snapshot for. Defaults to "mainnet".
        /// If `--snapshot-bucket` or `--archive-bucket` is not specified,
        /// the value of this flag is used to construct default bucket names.
        #[clap(long = "network", default_value = "mainnet")]
        network: Chain,
        /// Snapshot bucket name. If not specified, defaults are
        /// based on value of `--network` flag.
        #[clap(long = "snapshot-bucket", conflicts_with = "no_sign_request")]
        snapshot_bucket: Option<String>,
        /// Snapshot bucket type
        #[clap(
            long = "snapshot-bucket-type",
            conflicts_with = "no_sign_request",
            help = "Required if --no-sign-request is not set"
        )]
        snapshot_bucket_type: Option<ObjectStoreType>,
        /// Path to snapshot directory on local filesystem.
        /// Only applicable if `--snapshot-bucket-type` is "file".
        #[clap(
            long = "snapshot-path",
            help = "only used for testing, when --snapshot-bucket-type=FILE"
        )]
        snapshot_path: Option<PathBuf>,
        /// If true, no authentication is needed for snapshot restores
        #[clap(
            long = "no-sign-request",
            conflicts_with_all = &["snapshot_bucket", "snapshot_bucket_type"],
            help = "if set, no authentication is needed for snapshot restore"
        )]
        no_sign_request: bool,
        /// Download snapshot of the latest available epoch.
        /// If `--epoch` is specified, then this flag gets ignored.
        #[clap(
            long = "latest",
            conflicts_with = "epoch",
            help = "defaults to latest available snapshot in chosen bucket"
        )]
        latest: bool,
        /// If false (default), log level will be overridden to "off",
        /// and output will be reduced to necessary status information.
        #[clap(long = "verbose")]
        verbose: bool,
    },

    // Restore from formal (slim, DB agnostic) snapshot. Note that this is only supported
    /// for protocol versions supporting `commit_root_state_digest`. For mainnet, this is
    /// epoch 20+, and for testnet this is epoch 12+
    #[clap(
        name = "download-formal-snapshot",
        about = "Downloads formal database snapshot via cloud object store, outputs to local disk"
    )]
    DownloadFormalSnapshot {
        #[clap(long = "epoch", conflicts_with = "latest")]
        epoch: Option<u64>,
        #[clap(long = "genesis")]
        genesis: PathBuf,
        #[clap(long = "path")]
        path: PathBuf,
        /// Number of parallel downloads to perform. Defaults to a reasonable
        /// value based on number of available logical cores.
        #[clap(long = "num-parallel-downloads")]
        num_parallel_downloads: Option<usize>,
        /// Verification mode to employ.
        #[clap(long = "verify", default_value = "normal")]
        verify: Option<SnapshotVerifyMode>,
        /// Network to download snapshot for. Defaults to "mainnet".
        /// If `--snapshot-bucket` or `--archive-bucket` is not specified,
        /// the value of this flag is used to construct default bucket names.
        #[clap(long = "network", default_value = "mainnet")]
        network: Chain,
        /// Snapshot bucket name. If not specified, defaults are
        /// based on value of `--network` flag.
        #[clap(long = "snapshot-bucket", conflicts_with = "no_sign_request")]
        snapshot_bucket: Option<String>,
        /// Snapshot bucket type
        #[clap(
            long = "snapshot-bucket-type",
            conflicts_with = "no_sign_request",
            help = "Required if --no-sign-request is not set"
        )]
        snapshot_bucket_type: Option<ObjectStoreType>,
        /// Path to snapshot directory on local filesystem.
        /// Only applicable if `--snapshot-bucket-type` is "file".
        #[clap(long = "snapshot-path")]
        snapshot_path: Option<PathBuf>,
        /// If true, no authentication is needed for snapshot restores
        #[clap(
            long = "no-sign-request",
            conflicts_with_all = &["snapshot_bucket", "snapshot_bucket_type"],
            help = "if set, no authentication is needed for snapshot restore"
        )]
        no_sign_request: bool,
        /// Download snapshot of the latest available epoch.
        /// If `--epoch` is specified, then this flag gets ignored.
        #[clap(
            long = "latest",
            conflicts_with = "epoch",
            help = "defaults to latest available snapshot in chosen bucket"
        )]
        latest: bool,
        /// If false (default), log level will be overridden to "off",
        /// and output will be reduced to necessary status information.
        #[clap(long = "verbose")]
        verbose: bool,

        /// If provided, all checkpoint summaries from genesis to the end of the target epoch
        /// will be downloaded and (if --verify is provided) full checkpoint chain verification
        /// will be performed. If omitted, only end of epoch checkpoint summaries will be
        /// downloaded, and (if --verify is provided) will be verified via committee signature.
        #[clap(long = "all-checkpoints")]
        all_checkpoints: bool,
    },

    #[clap(name = "replay")]
    Replay {
        #[arg(long = "rpc")]
        rpc_url: Option<String>,
        #[arg(long = "safety-checks")]
        safety_checks: bool,
        #[arg(long = "authority")]
        use_authority: bool,
        #[arg(
            long = "cfg-path",
            short,
            help = "Path to the network config file. This should be specified when rpc_url is not present. \
            If not specified we will use the default network config file at ~/.sui-replay/network-config.yaml"
        )]
        cfg_path: Option<PathBuf>,
        #[arg(
            long,
            help = "The name of the chain to replay from, could be one of: mainnet, testnet, devnet.\
            When rpc_url is not specified, this is used to load the corresponding config from the network config file.\
            If not specified, mainnet will be used by default"
        )]
        chain: Option<String>,
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

async fn check_locked_object(
    sui_client: &Arc<SuiClient>,
    committee: Arc<BTreeMap<AuthorityPublicKeyBytes, u64>>,
    id: ObjectID,
    rescue: bool,
) -> anyhow::Result<()> {
    let clients = Arc::new(make_clients(sui_client).await?);
    let output = get_object(id, None, None, clients.clone()).await?;
    let output = GroupedObjectOutput::new(output, committee);
    if output.fully_locked {
        println!("Object {} is fully locked.", id);
        return Ok(());
    }
    let top_record = output.voting_power.first().unwrap();
    let top_record_stake = top_record.1;
    let top_record = top_record.0.clone().unwrap();
    if top_record.4.is_none() {
        println!(
            "Object {} does not seem to be locked by majority of validators (unlocked stake: {})",
            id, top_record_stake
        );
        return Ok(());
    }

    let tx_digest = top_record.2;
    if !rescue {
        println!("Object {} is rescueable, top tx: {:?}", id, tx_digest);
        return Ok(());
    }
    println!("Object {} is rescueable, trying tx {}", id, tx_digest);
    let validator = output
        .grouped_results
        .get(&Some(top_record))
        .unwrap()
        .first()
        .unwrap();
    let client = &clients.get(validator).unwrap().1;
    let tx = client
        .handle_transaction_info_request(TransactionInfoRequest {
            transaction_digest: tx_digest,
        })
        .await?
        .transaction;
    let res = sui_client
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::new(tx),
            SuiTransactionBlockResponseOptions::full_content(),
            None,
        )
        .await;
    match res {
        Ok(_) => {
            println!("Transaction executed successfully ({:?})", tx_digest);
        }
        Err(e) => {
            println!("Failed to execute transaction ({:?}): {:?}", tx_digest, e);
        }
    }
    Ok(())
}

impl ToolCommand {
    #[allow(clippy::format_in_format_args)]
    pub async fn execute(self, tracing_handle: TracingHandle) -> Result<(), anyhow::Error> {
        match self {
            ToolCommand::LockedObject {
                id,
                fullnode_rpc_url,
                rescue,
                address,
            } => {
                let sui_client =
                    Arc::new(SuiClientBuilder::default().build(fullnode_rpc_url).await?);
                let committee = Arc::new(
                    sui_client
                        .governance_api()
                        .get_committee_info(None)
                        .await?
                        .validators
                        .into_iter()
                        .collect::<BTreeMap<_, _>>(),
                );
                let object_ids = match id {
                    Some(id) => vec![id],
                    None => {
                        let address = address.expect("Either id or address must be provided");
                        sui_client
                            .coin_read_api()
                            .get_coins_stream(address, None)
                            .map(|c| c.coin_object_id)
                            .collect()
                            .await
                    }
                };
                for ids in object_ids.chunks(30) {
                    let mut tasks = vec![];
                    for id in ids {
                        tasks.push(check_locked_object(
                            &sui_client,
                            committee.clone(),
                            *id,
                            rescue,
                        ))
                    }
                    join_all(tasks)
                        .await
                        .into_iter()
                        .collect::<Result<Vec<_>, _>>()?;
                }
            }
            ToolCommand::FetchObject {
                id,
                validator,
                version,
                fullnode_rpc_url,
                verbosity,
                concise_no_header,
            } => {
                let sui_client =
                    Arc::new(SuiClientBuilder::default().build(fullnode_rpc_url).await?);
                let clients = Arc::new(make_clients(&sui_client).await?);
                let output = get_object(id, version, validator, clients).await?;

                match verbosity {
                    Verbosity::Grouped => {
                        let committee = Arc::new(
                            sui_client
                                .governance_api()
                                .get_committee_info(None)
                                .await?
                                .validators
                                .into_iter()
                                .collect::<BTreeMap<_, _>>(),
                        );
                        println!("{}", GroupedObjectOutput::new(output, committee));
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
                digest,
                show_input_tx,
                fullnode_rpc_url,
            } => {
                print!(
                    "{}",
                    get_transaction_block(digest, show_input_tx, fullnode_rpc_url).await?
                );
            }
            ToolCommand::DbTool { db_path, cmd } => {
                let path = PathBuf::from(db_path);
                match cmd {
                    Some(c) => execute_db_tool_command(path, c).await?,
                    None => print_db_all_tables(path)?,
                }
            }
            ToolCommand::DumpPackages {
                rpc_url,
                output_dir,
                before_checkpoint,
                verbose,
            } => {
                if !verbose {
                    tracing_handle
                        .update_log("off")
                        .expect("Failed to update log level");
                }

                sui_package_dump::dump(rpc_url, output_dir, before_checkpoint).await?;
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
                sequence_number,
                fullnode_rpc_url,
            } => {
                let sui_client =
                    Arc::new(SuiClientBuilder::default().build(fullnode_rpc_url).await?);
                let clients = make_clients(&sui_client).await?;

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
            ToolCommand::DownloadFormalSnapshot {
                epoch,
                genesis,
                path,
                num_parallel_downloads,
                verify,
                network,
                snapshot_bucket,
                snapshot_bucket_type,
                snapshot_path,
                no_sign_request,
                latest,
                verbose,
                all_checkpoints,
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
                let snapshot_bucket =
                    snapshot_bucket.or_else(|| match (network, no_sign_request) {
                        (Chain::Mainnet, false) => Some(
                            env::var("MAINNET_FORMAL_SIGNED_BUCKET")
                                .unwrap_or("mysten-mainnet-formal".to_string()),
                        ),
                        (Chain::Mainnet, true) => env::var("MAINNET_FORMAL_UNSIGNED_BUCKET").ok(),
                        (Chain::Testnet, true) => env::var("TESTNET_FORMAL_UNSIGNED_BUCKET").ok(),
                        (Chain::Testnet, _) => Some(
                            env::var("TESTNET_FORMAL_SIGNED_BUCKET")
                                .unwrap_or("mysten-testnet-formal".to_string()),
                        ),
                        (Chain::Unknown, _) => {
                            panic!("Cannot generate default snapshot bucket for unknown network");
                        }
                    });

                let aws_endpoint = env::var("AWS_SNAPSHOT_ENDPOINT").ok().or_else(|| {
                    if no_sign_request {
                        if network == Chain::Mainnet {
                            Some("https://formal-snapshot.mainnet.sui.io".to_string())
                        } else if network == Chain::Testnet {
                            Some("https://formal-snapshot.testnet.sui.io".to_string())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                });

                let snapshot_bucket_type = if no_sign_request {
                    ObjectStoreType::S3
                } else {
                    snapshot_bucket_type
                        .expect("You must set either --snapshot-bucket-type or --no-sign-request")
                };
                let snapshot_store_config = match snapshot_bucket_type {
                    ObjectStoreType::S3 => ObjectStoreConfig {
                        object_store: Some(ObjectStoreType::S3),
                        bucket: snapshot_bucket.filter(|s| !s.is_empty()),
                        aws_access_key_id: env::var("AWS_SNAPSHOT_ACCESS_KEY_ID").ok(),
                        aws_secret_access_key: env::var("AWS_SNAPSHOT_SECRET_ACCESS_KEY").ok(),
                        aws_region: env::var("AWS_SNAPSHOT_REGION").ok(),
                        aws_endpoint: aws_endpoint.filter(|s| !s.is_empty()),
                        aws_virtual_hosted_style_request: env::var(
                            "AWS_SNAPSHOT_VIRTUAL_HOSTED_REQUESTS",
                        )
                        .ok()
                        .and_then(|b| b.parse().ok())
                        .unwrap_or(no_sign_request),
                        object_store_connection_limit: 200,
                        no_sign_request,
                        ..Default::default()
                    },
                    ObjectStoreType::GCS => ObjectStoreConfig {
                        object_store: Some(ObjectStoreType::GCS),
                        bucket: snapshot_bucket,
                        google_service_account: env::var("GCS_SNAPSHOT_SERVICE_ACCOUNT_FILE_PATH")
                            .ok(),
                        object_store_connection_limit: 200,
                        no_sign_request,
                        ..Default::default()
                    },
                    ObjectStoreType::Azure => ObjectStoreConfig {
                        object_store: Some(ObjectStoreType::Azure),
                        bucket: snapshot_bucket,
                        azure_storage_account: env::var("AZURE_SNAPSHOT_STORAGE_ACCOUNT").ok(),
                        azure_storage_access_key: env::var("AZURE_SNAPSHOT_STORAGE_ACCESS_KEY")
                            .ok(),
                        object_store_connection_limit: 200,
                        no_sign_request,
                        ..Default::default()
                    },
                    ObjectStoreType::File => {
                        if snapshot_path.is_some() {
                            ObjectStoreConfig {
                                object_store: Some(ObjectStoreType::File),
                                directory: snapshot_path,
                                ..Default::default()
                            }
                        } else {
                            panic!(
                                "--snapshot-path must be specified for --snapshot-bucket-type=file"
                            );
                        }
                    }
                };

                let archive_bucket = Some(
                    env::var("FORMAL_SNAPSHOT_ARCHIVE_BUCKET").unwrap_or_else(|_| match network {
                        Chain::Mainnet => "mysten-mainnet-archives".to_string(),
                        Chain::Testnet => "mysten-testnet-archives".to_string(),
                        Chain::Unknown => {
                            panic!("Cannot generate default archive bucket for unknown network");
                        }
                    }),
                );

                let mut custom_archive_enabled = false;
                if let Ok(custom_archive_check) = env::var("CUSTOM_ARCHIVE_BUCKET") {
                    if custom_archive_check == "true" {
                        custom_archive_enabled = true;
                    }
                }
                let archive_store_config = if custom_archive_enabled {
                    let aws_region = Some(
                        env::var("FORMAL_SNAPSHOT_ARCHIVE_REGION")
                            .unwrap_or("us-west-2".to_string()),
                    );

                    let archive_bucket_type = env::var("FORMAL_SNAPSHOT_ARCHIVE_BUCKET_TYPE").expect("If setting `CUSTOM_ARCHIVE_BUCKET=true` Must set FORMAL_SNAPSHOT_ARCHIVE_BUCKET_TYPE, and credentials");
                    match archive_bucket_type.to_ascii_lowercase().as_str()
                    {
                        "s3" => ObjectStoreConfig {
                            object_store: Some(ObjectStoreType::S3),
                            bucket: archive_bucket.filter(|s| !s.is_empty()),
                            aws_access_key_id: env::var("AWS_ARCHIVE_ACCESS_KEY_ID").ok(),
                            aws_secret_access_key: env::var("AWS_ARCHIVE_SECRET_ACCESS_KEY").ok(),
                            aws_region,
                            aws_endpoint: env::var("AWS_ARCHIVE_ENDPOINT").ok(),
                            aws_virtual_hosted_style_request: env::var(
                                "AWS_ARCHIVE_VIRTUAL_HOSTED_REQUESTS",
                            )
                            .ok()
                            .and_then(|b| b.parse().ok())
                            .unwrap_or(false),
                            object_store_connection_limit: 50,
                            no_sign_request: false,
                            ..Default::default()
                        },
                        "gcs" => ObjectStoreConfig {
                            object_store: Some(ObjectStoreType::GCS),
                            bucket: archive_bucket,
                            google_service_account: env::var(
                                "GCS_ARCHIVE_SERVICE_ACCOUNT_FILE_PATH",
                            )
                            .ok(),
                            object_store_connection_limit: 50,
                            no_sign_request: false,
                            ..Default::default()
                        },
                        "azure" => ObjectStoreConfig {
                            object_store: Some(ObjectStoreType::Azure),
                            bucket: archive_bucket,
                            azure_storage_account: env::var("AZURE_ARCHIVE_STORAGE_ACCOUNT").ok(),
                            azure_storage_access_key: env::var("AZURE_ARCHIVE_STORAGE_ACCESS_KEY")
                                .ok(),
                            object_store_connection_limit: 50,
                            no_sign_request: false,
                            ..Default::default()
                        },
                        _ => panic!("If setting `CUSTOM_ARCHIVE_BUCKET=true` must set FORMAL_SNAPSHOT_ARCHIVE_BUCKET_TYPE to one of 'gcs', 'azure', or 's3' "),
                    }
                } else {
                    // if not explicitly overridden, just default to the permissionless archive store
                    ObjectStoreConfig {
                        object_store: Some(ObjectStoreType::S3),
                        bucket: archive_bucket.filter(|s| !s.is_empty()),
                        aws_region: Some("us-west-2".to_string()),
                        aws_endpoint: env::var("AWS_ARCHIVE_ENDPOINT").ok(),
                        aws_virtual_hosted_style_request: env::var(
                            "AWS_ARCHIVE_VIRTUAL_HOSTED_REQUESTS",
                        )
                        .ok()
                        .and_then(|b| b.parse().ok())
                        .unwrap_or(false),
                        object_store_connection_limit: 200,
                        no_sign_request: true,
                        ..Default::default()
                    }
                };
                let latest_available_epoch =
                    latest.then_some(get_latest_available_epoch(&snapshot_store_config).await?);
                let epoch_to_download = epoch.or(latest_available_epoch).expect(
                    "Either pass epoch with --epoch <epoch_num> or use latest with --latest",
                );

                if let Err(e) =
                    check_completed_snapshot(&snapshot_store_config, epoch_to_download).await
                {
                    panic!(
                        "Aborting snapshot restore: {}, snapshot may not be uploaded yet",
                        e
                    );
                }

                let verify = verify.unwrap_or_default();
                download_formal_snapshot(
                    &path,
                    epoch_to_download,
                    &genesis,
                    snapshot_store_config,
                    archive_store_config,
                    num_parallel_downloads,
                    network,
                    verify,
                    all_checkpoints,
                )
                .await?;
            }
            ToolCommand::DownloadDBSnapshot {
                epoch,
                path,
                skip_indexes,
                num_parallel_downloads,
                network,
                snapshot_bucket,
                snapshot_bucket_type,
                snapshot_path,
                no_sign_request,
                latest,
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
                let snapshot_bucket =
                    snapshot_bucket.or_else(|| match (network, no_sign_request) {
                        (Chain::Mainnet, false) => Some(
                            env::var("MAINNET_DB_SIGNED_BUCKET")
                                .unwrap_or("mysten-mainnet-snapshots".to_string()),
                        ),
                        (Chain::Mainnet, true) => env::var("MAINNET_DB_UNSIGNED_BUCKET").ok(),
                        (Chain::Testnet, true) => env::var("TESTNET_DB_UNSIGNED_BUCKET").ok(),
                        (Chain::Testnet, _) => Some(
                            env::var("TESTNET_DB_SIGNED_BUCKET")
                                .unwrap_or("mysten-testnet-snapshots".to_string()),
                        ),
                        (Chain::Unknown, _) => {
                            panic!("Cannot generate default snapshot bucket for unknown network");
                        }
                    });

                let aws_endpoint = env::var("AWS_SNAPSHOT_ENDPOINT").ok();
                let snapshot_bucket_type = if no_sign_request {
                    ObjectStoreType::S3
                } else {
                    snapshot_bucket_type
                        .expect("You must set either --snapshot-bucket-type or --no-sign-request")
                };
                let snapshot_store_config = if no_sign_request {
                    let aws_endpoint = env::var("AWS_SNAPSHOT_ENDPOINT").ok().or_else(|| {
                        if network == Chain::Mainnet {
                            Some("https://db-snapshot.mainnet.sui.io".to_string())
                        } else if network == Chain::Testnet {
                            Some("https://db-snapshot.testnet.sui.io".to_string())
                        } else {
                            None
                        }
                    });
                    ObjectStoreConfig {
                        object_store: Some(ObjectStoreType::S3),
                        aws_endpoint: aws_endpoint.filter(|s| !s.is_empty()),
                        aws_virtual_hosted_style_request: env::var(
                            "AWS_SNAPSHOT_VIRTUAL_HOSTED_REQUESTS",
                        )
                        .ok()
                        .and_then(|b| b.parse().ok())
                        .unwrap_or(no_sign_request),
                        object_store_connection_limit: 200,
                        no_sign_request,
                        ..Default::default()
                    }
                } else {
                    match snapshot_bucket_type {
                        ObjectStoreType::S3 => ObjectStoreConfig {
                            object_store: Some(ObjectStoreType::S3),
                            bucket: snapshot_bucket.filter(|s| !s.is_empty()),
                            aws_access_key_id: env::var("AWS_SNAPSHOT_ACCESS_KEY_ID").ok(),
                            aws_secret_access_key: env::var("AWS_SNAPSHOT_SECRET_ACCESS_KEY").ok(),
                            aws_region: env::var("AWS_SNAPSHOT_REGION").ok(),
                            aws_endpoint: aws_endpoint.filter(|s| !s.is_empty()),
                            aws_virtual_hosted_style_request: env::var(
                                "AWS_SNAPSHOT_VIRTUAL_HOSTED_REQUESTS",
                            )
                            .ok()
                            .and_then(|b| b.parse().ok())
                            .unwrap_or(no_sign_request),
                            object_store_connection_limit: 200,
                            no_sign_request,
                            ..Default::default()
                        },
                        ObjectStoreType::GCS => ObjectStoreConfig {
                            object_store: Some(ObjectStoreType::GCS),
                            bucket: snapshot_bucket,
                            google_service_account: env::var(
                                "GCS_SNAPSHOT_SERVICE_ACCOUNT_FILE_PATH",
                            )
                            .ok(),
                            google_project_id: env::var("GCS_SNAPSHOT_SERVICE_ACCOUNT_PROJECT_ID")
                                .ok(),
                            object_store_connection_limit: 200,
                            no_sign_request,
                            ..Default::default()
                        },
                        ObjectStoreType::Azure => ObjectStoreConfig {
                            object_store: Some(ObjectStoreType::Azure),
                            bucket: snapshot_bucket,
                            azure_storage_account: env::var("AZURE_SNAPSHOT_STORAGE_ACCOUNT").ok(),
                            azure_storage_access_key: env::var("AZURE_SNAPSHOT_STORAGE_ACCESS_KEY")
                                .ok(),
                            object_store_connection_limit: 200,
                            no_sign_request,
                            ..Default::default()
                        },
                        ObjectStoreType::File => {
                            if snapshot_path.is_some() {
                                ObjectStoreConfig {
                                    object_store: Some(ObjectStoreType::File),
                                    directory: snapshot_path,
                                    ..Default::default()
                                }
                            } else {
                                panic!(
                                "--snapshot-path must be specified for --snapshot-bucket-type=file"
                            );
                            }
                        }
                    }
                };

                let latest_available_epoch =
                    latest.then_some(get_latest_available_epoch(&snapshot_store_config).await?);
                let epoch_to_download = epoch.or(latest_available_epoch).expect(
                    "Either pass epoch with --epoch <epoch_num> or use latest with --latest",
                );

                if let Err(e) =
                    check_completed_snapshot(&snapshot_store_config, epoch_to_download).await
                {
                    panic!(
                        "Aborting snapshot restore: {}, snapshot may not be uploaded yet",
                        e
                    );
                }
                download_db_snapshot(
                    &path,
                    epoch_to_download,
                    snapshot_store_config,
                    skip_indexes,
                    num_parallel_downloads,
                )
                .await?;
            }
            ToolCommand::Replay {
                rpc_url,
                safety_checks,
                cmd,
                use_authority,
                cfg_path,
                chain,
            } => {
                execute_replay_command(rpc_url, safety_checks, use_authority, cfg_path, chain, cmd)
                    .await?;
            }
            ToolCommand::VerifyArchive {
                genesis,
                object_store_config,
                download_concurrency,
            } => {
                verify_archive(&genesis, object_store_config, download_concurrency, true).await?;
            }
            ToolCommand::PrintArchiveManifest {
                object_store_config,
            } => {
                println!("{}", read_manifest_as_json(object_store_config).await?);
            }
            ToolCommand::UpdateArchiveManifest {
                object_store_config,
                archive_json_path,
            } => {
                write_manifest_from_json(object_store_config, archive_json_path).await?;
            }
            ToolCommand::VerifyArchiveByChecksum {
                object_store_config,
                download_concurrency,
            } => {
                verify_archive_by_checksum(object_store_config, download_concurrency).await?;
            }
            ToolCommand::DumpArchiveByChecksum {
                object_store_config,
                start,
                end,
                max_content_length,
            } => {
                dump_checkpoints_from_archive(object_store_config, start, end, max_content_length)
                    .await?;
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
                let (agg, _) =
                    AuthorityAggregatorBuilder::from_genesis(&genesis).build_network_clients();
                let result = agg.process_transaction(transaction, None).await;
                println!("{:?}", result);
            }
        };
        Ok(())
    }
}
