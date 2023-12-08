use async_trait::async_trait;
use move_core_types::account_address::AccountAddress;
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use sui_rest_api::Client;
use sui_types::{
    base_types::SequenceNumber,
    committee::Committee,
    crypto::AuthorityQuorumSignInfo,
    digests::TransactionDigest,
    message_envelope::Envelope,
    messages_checkpoint::{CertifiedCheckpointSummary, CheckpointSummary, EndOfEpochData},
};

use sui_config::genesis::Genesis;

use sui_json::SuiJsonValue;
use sui_package_resolver::{Package, PackageStore, Resolver, Result};
use sui_sdk::SuiClientBuilder;

use clap::{Parser, Subcommand};
use std::{fs, io::Write, path::PathBuf};
use std::{io::Read, sync::Arc};

use std::str::FromStr;

/// A light client for the Sui blockchain
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<SCommands>,
}

struct RemotePackageStore {
    client: Client,
}

impl RemotePackageStore {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl PackageStore for RemotePackageStore {
    /// Latest version of the object at `id`.
    async fn version(&self, id: AccountAddress) -> Result<SequenceNumber> {
        Ok(self.client.get_object(id.into()).await.unwrap().version())
    }
    /// Read package contents. Fails if `id` is not an object, not a package, or is malformed in
    /// some way.
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        // TODO(SECURITY):  here we also need to authenticate the transaction that wrote this objects,
        //                  and ensure it wrote this object by hash.
        let object = self.client.get_object(id.into()).await.unwrap();
        println!("Object TID: {:?}", object.previous_transaction);
        let package = Package::read(&object).unwrap();
        Ok(Arc::new(package))
    }
}

#[derive(Subcommand, Debug)]
enum SCommands {
    /// Sync all end-of-epoch checkpoints
    Sync {},

    /// Checks a specific transaction using the light client
    Check {
        /// Transaction hash
        #[arg(short, long, value_name = "TID")]
        tid: String,
    },
}

// The config file for the light client inclding the root of trust genesis digest
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct Config {
    /// Full node url
    full_node_url: String,

    /// Checkpoint summary directory
    checkpoint_summary_dir: PathBuf,

    //  Genesis file name
    genesis_filename: PathBuf,
}

// The list of checkpoints at the end of each epoch
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct CheckpointsList {
    // List of end of epoch checkpoints
    checkpoints: Vec<u64>,
}

fn read_checkpoint_list(config: &Config) -> CheckpointsList {
    let mut checkpoints_path = config.checkpoint_summary_dir.clone();
    checkpoints_path.push("checkpoints.yaml");
    // Read the resulting file and parse the yaml checkpoint list
    let reader = fs::File::open(checkpoints_path.clone()).unwrap_or_else(|_| {
        panic!(
            "Unable to load checkpoints from {}",
            checkpoints_path.display()
        )
    });
    serde_yaml::from_reader(reader).unwrap()
}

fn read_checkpoint(
    config: &Config,
    seq: u64,
) -> Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>> {
    // Read the resulting file and parse the yaml checkpoint list
    let mut checkpoint_path = config.checkpoint_summary_dir.clone();
    checkpoint_path.push(format!("{}.yaml", seq));
    let mut reader = fs::File::open(checkpoint_path.clone()).unwrap_or_else(|_| {
        panic!(
            "Unable to load checkpoint from {}",
            checkpoint_path.display()
        )
    });
    let metadata = fs::metadata(&checkpoint_path).expect("unable to read metadata");
    let mut buffer = vec![0; metadata.len() as usize];
    reader.read_exact(&mut buffer).expect("buffer overflow");
    bcs::from_bytes(&buffer).unwrap_or_else(|_| panic!("Ckp {} exists", seq))
}

fn write_checkpoint(
    config: &Config,
    summary: &Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>>,
) {
    // Write the checkpoint summary to a file
    let mut checkpoint_path = config.checkpoint_summary_dir.clone();
    checkpoint_path.push(format!("{}.yaml", summary.sequence_number));
    let mut writer = fs::File::create(checkpoint_path.clone()).unwrap_or_else(|_| {
        panic!(
            "Unable to create checkpoint file {}",
            checkpoint_path.display()
        )
    });
    let bytes = bcs::to_bytes(&summary).unwrap();
    writer.write_all(&bytes).unwrap();
}

fn write_checkpoint_list(config: &Config, checkpoints_list: &CheckpointsList) {
    // Write the checkpoint list to a file
    let mut checkpoints_path = config.checkpoint_summary_dir.clone();
    checkpoints_path.push("checkpoints.yaml");
    let mut writer = fs::File::create(checkpoints_path.clone()).unwrap_or_else(|_| {
        panic!(
            "Unable to create checkpoint file {}",
            checkpoints_path.display()
        )
    });
    let bytes = serde_yaml::to_vec(&checkpoints_list).unwrap();
    writer.write_all(&bytes).unwrap();
}

async fn download_checkpoint_summary(config: &Config, seq: u64) -> CertifiedCheckpointSummary {
    // Download the checkpoint from the server
    let client = Client::new(config.full_node_url.as_str());
    client.get_checkpoint_summary(seq).await.unwrap()
}

/// Run binary search to for each end of epoch checkpoint that is missing
/// between the latest on the list and the latest checkpoint.
async fn pre_sync_checkpoints_to_latest(config: &Config) {
    // Get the local checlpoint list
    let mut checkpoints_list: CheckpointsList = read_checkpoint_list(config);
    let latest_in_list = checkpoints_list.checkpoints.last().unwrap();

    // Download the latest in list checkpoint
    let summary = download_checkpoint_summary(config, *latest_in_list).await;
    let mut last_epoch = Some(summary.epoch());
    let mut last_checkpoint_seq = Some(summary.sequence_number);

    // Download the very latest checkpoint
    let client = Client::new(config.full_node_url.as_str());
    let latest = client.get_latest_checkpoint().await.unwrap();

    // Binary search to find missing checkpoints
    while last_epoch.unwrap() + 1 < latest.epoch() {
        let mut start = last_checkpoint_seq.unwrap();
        let mut end = latest.sequence_number;

        let taget_epoch = last_epoch.unwrap() + 1;
        // Print target
        println!("Target Epoch: {}", taget_epoch);
        let mut found_summary = None;

        while start < end {
            let mid = (start + end) / 2;
            let summary = download_checkpoint_summary(config, mid).await;

            // print summary epoch and seq
            println!(
                "Epoch: {} Seq: {}: {}",
                summary.epoch(),
                summary.sequence_number,
                summary.end_of_epoch_data.is_some()
            );

            if summary.epoch() == taget_epoch && summary.end_of_epoch_data.is_some() {
                found_summary = Some(summary);
                break;
            }

            if summary.epoch() <= taget_epoch {
                start = mid + 1;
            } else {
                end = mid;
            }
        }

        if let Some(summary) = found_summary {
            // Note: Do not write summary to file, since we must only persist
            //       checkpoints that have been verified by the previous committee

            // Add to the list
            checkpoints_list.checkpoints.push(summary.sequence_number);
            write_checkpoint_list(config, &checkpoints_list);

            // Update
            last_epoch = Some(summary.epoch());
            last_checkpoint_seq = Some(summary.sequence_number);
        }
    }
}

async fn check_and_sync_checkpoints(config: &Config) {
    pre_sync_checkpoints_to_latest(config).await;

    // Get the local checlpoint list
    let checkpoints_list: CheckpointsList = read_checkpoint_list(config);

    // Load the genesis committee
    let mut genesis_path = config.checkpoint_summary_dir.clone();
    genesis_path.push(&config.genesis_filename);
    let genesis_committee = Genesis::load(&genesis_path).unwrap().committee().unwrap();

    // Check the signatures of all checkpoints
    // And download any missing ones

    let mut prev_committee = genesis_committee;
    for ckp_id in &checkpoints_list.checkpoints {
        // check if there is a file with this name ckp_id.yaml in the checkpoint_summary_dir
        let mut checkpoint_path = config.checkpoint_summary_dir.clone();
        checkpoint_path.push(format!("{}.yaml", ckp_id));

        // If file exists read the file otherwise download it from the server
        let summary = if checkpoint_path.exists() {
            read_checkpoint(config, *ckp_id)
        } else {
            // Download the checkpoint from the server
            download_checkpoint_summary(config, *ckp_id).await
        };

        summary.clone().verify(&prev_committee).unwrap();

        // Pirnt the id of the checkpoint and the epoch number
        println!(
            "Epoch: {} Checkpoint ID: {}",
            summary.epoch(),
            summary.digest()
        );

        // Exract the new committee information
        if let Some(EndOfEpochData {
            next_epoch_committee,
            ..
        }) = &summary.end_of_epoch_data
        {
            let next_committee = next_epoch_committee.iter().cloned().collect();
            let committee = Committee::new(summary.epoch().saturating_add(1), next_committee);
            bcs::to_bytes(&committee).unwrap();
            prev_committee = committee;
        } else {
            panic!("Expected all checkpoints to be endf-of-epoch checkpoints");
        }

        // Write the checkpoint summary to a file
        write_checkpoint(config, &summary);
    }
}

async fn check_transaction_tid(config: &Config, tid: String) {
    let sui_mainnet: Arc<sui_sdk::SuiClient> = Arc::new(
        SuiClientBuilder::default()
            .build("http://ord-mnt-rpcbig-06.mainnet.sui.io:9000")
            .await
            .unwrap(),
    );
    let read_api = sui_mainnet.read_api();

    // Lookup the transaction id and get the checkpoint sequence number
    let options = SuiTransactionBlockResponseOptions::new();
    let seq = read_api
        .get_transaction_with_options(
            TransactionDigest::from_str(tid.clone().as_str()).unwrap(),
            options,
        )
        .await
        .unwrap()
        .checkpoint
        .unwrap();

    // Download the full checkpoint for this sequence number
    let client = Client::new(config.full_node_url.as_str());
    let full_check_point = client.get_full_checkpoint(seq).await.unwrap();
    let summary = &full_check_point.checkpoint_summary;

    // Check the validity of the checkpoint summary

    // Load the list of stored checkpoints
    let checkpoints_list: CheckpointsList = read_checkpoint_list(config);
    // find the stored checkpoint before the seq checkpoint
    let prev_ckp_id = checkpoints_list
        .checkpoints
        .iter()
        .filter(|ckp_id| **ckp_id < seq)
        .last()
        .unwrap();

    // Read it from the store
    let prev_ckp = read_checkpoint(config, *prev_ckp_id);

    // Get the committee from the previous checkpoint
    let prev_committee = prev_ckp
        .end_of_epoch_data
        .as_ref()
        .unwrap()
        .next_epoch_committee
        .iter()
        .cloned()
        .collect();

    // Make a commitee object using this
    let committee = Committee::new(prev_ckp.epoch().saturating_add(1), prev_committee);

    // Verify the checkpoint summary using the committee
    summary
        .clone()
        .verify(&committee)
        .expect("The signatures on the downloaded checkpoint summary are not valid");

    // Check the validty of the checkpoint contents

    let contents = &full_check_point.checkpoint_contents;
    assert!(contents.digest() == &summary.content_digest, "The content digest in the checkpoint summary does not match the digest of the checkpoint contents");

    // Check the validity of the transaction

    let found: &Vec<_> = &full_check_point
        .checkpoint_contents
        .enumerate_transactions(summary)
        .filter(|(_, t)| t.transaction.to_string() == tid)
        .collect();

    println!("Valid: {}", !found.is_empty());
    assert!(
        found.len() == 1,
        "Transaction not found in checkpoint contents"
    );
    let exec_digests = found.first().unwrap();

    println!(
        "Executed TID: {} Effects: {}",
        exec_digests.1.transaction, exec_digests.1.effects
    );

    let matching_tx = full_check_point
        .transactions
        .iter()
        // Note that we get the digest of the effects to ensure this is
        // indeed the correct effects that are authenticated in the contents.
        .find(|tx| &tx.effects.execution_digests() == exec_digests.1)
        .unwrap();

    for event in matching_tx.events.as_ref().unwrap().data.iter() {
        let client = Client::new(config.full_node_url.as_str());
        let remote_package_store = RemotePackageStore::new(client);
        let resolver = Resolver::new(remote_package_store);

        let type_layout = resolver
            .type_layout(event.type_.clone().into())
            .await
            .unwrap();

        let json_val = SuiJsonValue::from_bcs_bytes(Some(&type_layout), &event.contents).unwrap();

        println!(
            "Event:\n - Package: {}\n - Module: {}\n - Sender: {}\n{}",
            event.package_id,
            event.transaction_module,
            event.sender,
            serde_json::to_string_pretty(&json_val.to_json_value()).unwrap()
        );
    }
}

#[tokio::main]
pub async fn main() {
    // Command line arguments and config loading
    let args = Args::parse();

    let path = args
        .config
        .unwrap_or_else(|| panic!("Need a config file path"));
    let reader = fs::File::open(path.clone())
        .unwrap_or_else(|_| panic!("Unable to load config from {}", path.display()));
    let config: Config = serde_yaml::from_reader(reader).unwrap();

    // Print config parameters
    println!(
        "Checkpoint Dir: {}",
        config.checkpoint_summary_dir.display()
    );

    match args.command {
        Some(SCommands::Check { tid }) => {
            check_transaction_tid(&config, tid).await;
        }
        Some(SCommands::Sync {}) => {
            check_and_sync_checkpoints(&config).await;
        }
        _ => {}
    }
}
