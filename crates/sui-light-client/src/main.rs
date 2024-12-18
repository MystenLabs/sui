// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use move_core_types::account_address::AccountAddress;
use sui_json_rpc_types::{SuiObjectDataOptions, SuiTransactionBlockResponseOptions};

use sui_rpc_api::CheckpointData;
use sui_types::{
    base_types::ObjectID,
    committee::Committee,
    crypto::AuthorityQuorumSignInfo,
    digests::TransactionDigest,
    effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents},
    message_envelope::Envelope,
    messages_checkpoint::{CertifiedCheckpointSummary, CheckpointSummary, EndOfEpochData},
    object::{bounded_visitor::BoundedVisitor, Data, Object},
};

use sui_config::genesis::Genesis;

use sui_package_resolver::Result as ResolverResult;
use sui_package_resolver::{Package, PackageStore, Resolver};
use sui_sdk::SuiClientBuilder;

use clap::{Parser, Subcommand};
use std::{collections::HashMap, fs, io::Write, path::PathBuf, str::FromStr, sync::Mutex};
use std::{io::Read, sync::Arc};

use log::info;
use object_store::parse_url;
use object_store::path::Path;
use serde_json::json;
use serde_json::Value;
use url::Url;

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
    config: Config,
    cache: Mutex<HashMap<AccountAddress, Arc<Package>>>,
}

impl RemotePackageStore {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            cache: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl PackageStore for RemotePackageStore {
    /// Read package contents. Fails if `id` is not an object, not a package, or is malformed in
    /// some way.
    async fn fetch(&self, id: AccountAddress) -> ResolverResult<Arc<Package>> {
        // Check if we have it in the cache
        if let Some(package) = self.cache.lock().unwrap().get(&id) {
            info!("Fetch Package: {} cache hit", id);
            return Ok(package.clone());
        }

        info!("Fetch Package: {}", id);

        let object = get_verified_object(&self.config, id.into()).await.unwrap();
        let package = Arc::new(Package::read_from_object(&object).unwrap());

        // Add to the cache
        self.cache.lock().unwrap().insert(id, package.clone());

        Ok(package)
    }
}

#[derive(Subcommand, Debug)]
enum SCommands {
    /// Sync all end-of-epoch checkpoints
    Sync {},

    /// Checks a specific transaction using the light client
    Transaction {
        /// Transaction hash
        #[arg(short, long, value_name = "TID")]
        tid: String,
    },

    /// Checks a specific object using the light client
    Object {
        /// Transaction hash
        #[arg(short, long, value_name = "OID")]
        oid: String,
    },
}

// The config file for the light client including the root of trust genesis digest
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct Config {
    /// Full node url
    full_node_url: String,

    /// Checkpoint summary directory
    checkpoint_summary_dir: PathBuf,

    //  Genesis file name
    genesis_filename: PathBuf,

    /// Object store url
    object_store_url: String,

    /// GraphQL endpoint
    graphql_url: String,
}

async fn query_last_checkpoint_of_epoch(config: &Config, epoch_id: u64) -> anyhow::Result<u64> {
    // GraphQL query to get the last checkpoint of an epoch
    let query = json!({
        "query": "query ($epochID: Int) { epoch(id: $epochID) { checkpoints(last: 1) { nodes { sequenceNumber } } } }",
        "variables": { "epochID": epoch_id }
    });

    // Submit the query by POSTing to the GraphQL endpoint
    let client = reqwest::Client::new();
    let resp = client
        .post(&config.graphql_url)
        .header("Content-Type", "application/json")
        .body(query.to_string())
        .send()
        .await
        .expect("Cannot connect to graphql")
        .text()
        .await
        .expect("Cannot parse response");

    // Parse the JSON response to get the last checkpoint of the epoch
    let v: Value = serde_json::from_str(resp.as_str()).expect("Incorrect JSON response");
    let checkpoint_number = v["data"]["epoch"]["checkpoints"]["nodes"][0]["sequenceNumber"]
        .as_u64()
        .unwrap();

    Ok(checkpoint_number)
}

// The list of checkpoints at the end of each epoch
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct CheckpointsList {
    // List of end of epoch checkpoints
    checkpoints: Vec<u64>,
}

fn read_checkpoint_list(config: &Config) -> anyhow::Result<CheckpointsList> {
    let mut checkpoints_path = config.checkpoint_summary_dir.clone();
    checkpoints_path.push("checkpoints.yaml");
    // Read the resulting file and parse the yaml checkpoint list
    let reader = fs::File::open(checkpoints_path.clone())?;
    Ok(serde_yaml::from_reader(reader)?)
}

fn read_checkpoint(
    config: &Config,
    seq: u64,
) -> anyhow::Result<Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>>> {
    read_checkpoint_general(config, seq, None)
}

fn read_checkpoint_general(
    config: &Config,
    seq: u64,
    path: Option<&str>,
) -> anyhow::Result<Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>>> {
    // Read the resulting file and parse the yaml checkpoint list
    let mut checkpoint_path = config.checkpoint_summary_dir.clone();
    if let Some(path) = path {
        checkpoint_path.push(path);
    }
    checkpoint_path.push(format!("{}.yaml", seq));
    let mut reader = fs::File::open(checkpoint_path.clone())?;
    let metadata = fs::metadata(&checkpoint_path)?;
    let mut buffer = vec![0; metadata.len() as usize];
    reader.read_exact(&mut buffer)?;
    bcs::from_bytes(&buffer).map_err(|_| anyhow!("Unable to parse checkpoint file"))
}

fn write_checkpoint(
    config: &Config,
    summary: &Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>>,
) -> anyhow::Result<()> {
    write_checkpoint_general(config, summary, None)
}

fn write_checkpoint_general(
    config: &Config,
    summary: &Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>>,
    path: Option<&str>,
) -> anyhow::Result<()> {
    // Write the checkpoint summary to a file
    let mut checkpoint_path = config.checkpoint_summary_dir.clone();
    if let Some(path) = path {
        checkpoint_path.push(path);
    }
    checkpoint_path.push(format!("{}.yaml", summary.sequence_number));
    let mut writer = fs::File::create(checkpoint_path.clone())?;
    let bytes =
        bcs::to_bytes(&summary).map_err(|_| anyhow!("Unable to serialize checkpoint summary"))?;
    writer.write_all(&bytes)?;
    Ok(())
}

fn write_checkpoint_list(
    config: &Config,
    checkpoints_list: &CheckpointsList,
) -> anyhow::Result<()> {
    // Write the checkpoint list to a file
    let mut checkpoints_path = config.checkpoint_summary_dir.clone();
    checkpoints_path.push("checkpoints.yaml");
    let mut writer = fs::File::create(checkpoints_path.clone())?;
    let bytes = serde_yaml::to_vec(&checkpoints_list)?;
    writer
        .write_all(&bytes)
        .map_err(|_| anyhow!("Unable to serialize checkpoint list"))
}

async fn download_checkpoint_summary(
    config: &Config,
    checkpoint_number: u64,
) -> anyhow::Result<CertifiedCheckpointSummary> {
    // Download the checkpoint from the server

    let url = Url::parse(&config.object_store_url)?;
    let (dyn_store, _store_path) = parse_url(&url).unwrap();
    let path = Path::from(format!("{}.chk", checkpoint_number));
    let response = dyn_store.get(&path).await?;
    let bytes = response.bytes().await?;
    let (_, blob) = bcs::from_bytes::<(u8, CheckpointData)>(&bytes)?;

    info!("Downloaded checkpoint summary: {}", checkpoint_number);
    Ok(blob.checkpoint_summary)
}

/// Run binary search to for each end of epoch checkpoint that is missing
/// between the latest on the list and the latest checkpoint.
async fn sync_checkpoint_list_to_latest(config: &Config) -> anyhow::Result<()> {
    // Get the local checkpoint list
    let mut checkpoints_list: CheckpointsList = read_checkpoint_list(config)?;
    let latest_in_list = checkpoints_list
        .checkpoints
        .last()
        .ok_or(anyhow!("Empty checkpoint list"))?;

    // Download the latest in list checkpoint
    let summary = download_checkpoint_summary(config, *latest_in_list).await?;
    let mut last_epoch = summary.epoch();

    // Download the very latest checkpoint
    let client = SuiClientBuilder::default()
        .build(config.full_node_url.as_str())
        .await
        .expect("Cannot connect to full node");

    let latest_seq = client
        .read_api()
        .get_latest_checkpoint_sequence_number()
        .await?;
    let latest = download_checkpoint_summary(config, latest_seq).await?;

    // Sequentially record all the missing end of epoch checkpoints numbers
    while last_epoch + 1 < latest.epoch() {
        let target_epoch = last_epoch + 1;
        let target_last_checkpoint_number =
            query_last_checkpoint_of_epoch(config, target_epoch).await?;

        // Add to the list
        checkpoints_list
            .checkpoints
            .push(target_last_checkpoint_number);
        write_checkpoint_list(config, &checkpoints_list)?;

        // Update
        last_epoch = target_epoch;

        println!(
            "Last Epoch: {} Last Checkpoint: {}",
            target_epoch, target_last_checkpoint_number
        );
    }

    Ok(())
}

async fn check_and_sync_checkpoints(config: &Config) -> anyhow::Result<()> {
    sync_checkpoint_list_to_latest(config)
        .await
        .map_err(|e| anyhow!(format!("Cannot refresh list: {e}")))?;

    // Get the local checkpoint list
    let checkpoints_list: CheckpointsList = read_checkpoint_list(config)
        .map_err(|e| anyhow!(format!("Cannot read checkpoint list: {e}")))?;

    // Load the genesis committee
    let mut genesis_path = config.checkpoint_summary_dir.clone();
    genesis_path.push(&config.genesis_filename);
    let genesis_committee = Genesis::load(&genesis_path)?
        .committee()
        .map_err(|e| anyhow!(format!("Cannot load Genesis: {e}")))?;

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
                .map_err(|e| anyhow!(format!("Cannot read checkpoint: {e}")))?
        } else {
            // Download the checkpoint from the server
            let summary = download_checkpoint_summary(config, *ckp_id)
                .await
                .map_err(|e| anyhow!(format!("Cannot download summary: {e}")))?;
            summary.clone().try_into_verified(&prev_committee)?;
            // Write the checkpoint summary to a file
            write_checkpoint(config, &summary)?;
            summary
        };

        // Print the id of the checkpoint and the epoch number
        println!(
            "Epoch: {} Checkpoint ID: {}",
            summary.epoch(),
            summary.digest()
        );

        // Extract the new committee information
        if let Some(EndOfEpochData {
            next_epoch_committee,
            ..
        }) = &summary.end_of_epoch_data
        {
            let next_committee = next_epoch_committee.iter().cloned().collect();
            prev_committee =
                Committee::new(summary.epoch().checked_add(1).unwrap(), next_committee);
        } else {
            return Err(anyhow!(
                "Expected all checkpoints to be end-of-epoch checkpoints"
            ));
        }
    }

    Ok(())
}

async fn get_full_checkpoint(
    config: &Config,
    checkpoint_number: u64,
) -> anyhow::Result<CheckpointData> {
    let url = Url::parse(&config.object_store_url)
        .map_err(|_| anyhow!("Cannot parse object store URL"))?;
    let (dyn_store, _store_path) = parse_url(&url).unwrap();
    let path = Path::from(format!("{}.chk", checkpoint_number));
    info!("Request full checkpoint: {}", path);
    let response = dyn_store
        .get(&path)
        .await
        .map_err(|_| anyhow!("Cannot get full checkpoint from object store"))?;
    let bytes = response.bytes().await?;
    let (_, full_checkpoint) = bcs::from_bytes::<(u8, CheckpointData)>(&bytes)?;
    Ok(full_checkpoint)
}

fn extract_verified_effects_and_events(
    checkpoint: &CheckpointData,
    committee: &Committee,
    tid: TransactionDigest,
) -> anyhow::Result<(TransactionEffects, Option<TransactionEvents>)> {
    let summary = &checkpoint.checkpoint_summary;

    // Verify the checkpoint summary using the committee
    summary.verify_with_contents(committee, Some(&checkpoint.checkpoint_contents))?;

    // Check the validity of the transaction
    let contents = &checkpoint.checkpoint_contents;
    let (matching_tx, _) = checkpoint
        .transactions
        .iter()
        .zip(contents.iter())
        // Note that we get the digest of the effects to ensure this is
        // indeed the correct effects that are authenticated in the contents.
        .find(|(tx, digest)| {
            tx.effects.execution_digests() == **digest && digest.transaction == tid
        })
        .ok_or(anyhow!("Transaction not found in checkpoint contents"))?;

    // Check the events are all correct.
    let events_digest = matching_tx.events.as_ref().map(|events| events.digest());
    anyhow::ensure!(
        events_digest.as_ref() == matching_tx.effects.events_digest(),
        "Events digest does not match"
    );

    // Since we do not check objects we do not return them
    Ok((matching_tx.effects.clone(), matching_tx.events.clone()))
}

async fn get_verified_effects_and_events(
    config: &Config,
    tid: TransactionDigest,
) -> anyhow::Result<(TransactionEffects, Option<TransactionEvents>)> {
    let sui_mainnet: sui_sdk::SuiClient = SuiClientBuilder::default()
        .build(config.full_node_url.as_str())
        .await
        .unwrap();
    let read_api = sui_mainnet.read_api();

    info!("Getting effects and events for TID: {}", tid);
    // Lookup the transaction id and get the checkpoint sequence number
    let options = SuiTransactionBlockResponseOptions::new();
    let seq = read_api
        .get_transaction_with_options(tid, options)
        .await
        .map_err(|e| anyhow!(format!("Cannot get transaction: {e}")))?
        .checkpoint
        .ok_or(anyhow!("Transaction not found"))?;

    // Download the full checkpoint for this sequence number
    let full_check_point = get_full_checkpoint(config, seq)
        .await
        .map_err(|e| anyhow!(format!("Cannot get full checkpoint: {e}")))?;

    // Load the list of stored checkpoints
    let checkpoints_list: CheckpointsList = read_checkpoint_list(config)?;

    // find the stored checkpoint before the seq checkpoint
    let prev_ckp_id = checkpoints_list
        .checkpoints
        .iter()
        .filter(|ckp_id| **ckp_id < seq)
        .last();

    let committee = if let Some(prev_ckp_id) = prev_ckp_id {
        // Read it from the store
        let prev_ckp = read_checkpoint(config, *prev_ckp_id)?;

        // Check we have the right checkpoint
        anyhow::ensure!(
            prev_ckp.epoch().checked_add(1).unwrap() == full_check_point.checkpoint_summary.epoch(),
            "Checkpoint sequence number does not match. Need to Sync."
        );

        // Get the committee from the previous checkpoint
        let current_committee = prev_ckp
            .end_of_epoch_data
            .as_ref()
            .ok_or(anyhow!(
                "Expected all checkpoints to be end-of-epoch checkpoints"
            ))?
            .next_epoch_committee
            .iter()
            .cloned()
            .collect();

        // Make a committee object using this
        Committee::new(prev_ckp.epoch().checked_add(1).unwrap(), current_committee)
    } else {
        // Since we did not find a small committee checkpoint we use the genesis
        let mut genesis_path = config.checkpoint_summary_dir.clone();
        genesis_path.push(&config.genesis_filename);
        Genesis::load(&genesis_path)?
            .committee()
            .map_err(|e| anyhow!(format!("Cannot load Genesis: {e}")))?
    };

    info!("Extracting effects and events for TID: {}", tid);
    extract_verified_effects_and_events(&full_check_point, &committee, tid)
        .map_err(|e| anyhow!(format!("Cannot extract effects and events: {e}")))
}

async fn get_verified_object(config: &Config, id: ObjectID) -> anyhow::Result<Object> {
    let sui_client: Arc<sui_sdk::SuiClient> = Arc::new(
        SuiClientBuilder::default()
            .build(config.full_node_url.as_str())
            .await
            .unwrap(),
    );

    info!("Getting object: {}", id);

    let read_api = sui_client.read_api();
    let object_json = read_api
        .get_object_with_options(id, SuiObjectDataOptions::bcs_lossless())
        .await
        .expect("Cannot get object");
    let object = object_json
        .into_object()
        .expect("Cannot make into object data");
    let object: Object = object.try_into().expect("Cannot reconstruct object");

    // Need to authenticate this object
    let (effects, _) = get_verified_effects_and_events(config, object.previous_transaction)
        .await
        .expect("Cannot get effects and events");

    // check that this object ID, version and hash is in the effects
    let target_object_ref = object.compute_object_reference();
    effects
        .all_changed_objects()
        .iter()
        .find(|object_ref| object_ref.0 == target_object_ref)
        .ok_or(anyhow!("Object not found"))
        .expect("Object not found");

    Ok(object)
}

#[tokio::main]
pub async fn main() {
    env_logger::init();

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

    let remote_package_store = RemotePackageStore::new(config.clone());
    let resolver = Resolver::new(remote_package_store);

    match args.command {
        Some(SCommands::Transaction { tid }) => {
            let (effects, events) = get_verified_effects_and_events(
                &config,
                TransactionDigest::from_str(&tid).unwrap(),
            )
            .await
            .unwrap();

            let exec_digests = effects.execution_digests();
            println!(
                "Executed TID: {} Effects: {}",
                exec_digests.transaction, exec_digests.effects
            );

            if events.is_some() {
                for event in events.as_ref().unwrap().data.iter() {
                    let type_layout = resolver
                        .type_layout(event.type_.clone().into())
                        .await
                        .unwrap();

                    let result = BoundedVisitor::deserialize_value(&event.contents, &type_layout)
                        .expect("Cannot deserialize");

                    println!(
                        "Event:\n - Package: {}\n - Module: {}\n - Sender: {}\n - Type: {}\n{}",
                        event.package_id,
                        event.transaction_module,
                        event.sender,
                        event.type_,
                        serde_json::to_string_pretty(&result).unwrap()
                    );
                }
            } else {
                println!("No events found");
            }
        }
        Some(SCommands::Object { oid }) => {
            let oid = ObjectID::from_str(&oid).unwrap();
            let object = get_verified_object(&config, oid).await.unwrap();

            if let Data::Move(move_object) = &object.data {
                let object_type = move_object.type_().clone();

                let type_layout = resolver
                    .type_layout(object_type.clone().into())
                    .await
                    .unwrap();

                let result =
                    BoundedVisitor::deserialize_value(move_object.contents(), &type_layout)
                        .expect("Cannot deserialize");

                let (oid, version, hash) = object.compute_object_reference();
                println!(
                    "OID: {}\n - Version: {}\n - Hash: {}\n - Owner: {}\n - Type: {}\n{}",
                    oid,
                    version,
                    hash,
                    object.owner,
                    object_type,
                    serde_json::to_string_pretty(&result).unwrap()
                );
            }
        }

        Some(SCommands::Sync {}) => {
            check_and_sync_checkpoints(&config)
                .await
                .expect("Failed to sync checkpoints");
        }
        _ => {}
    }
}

// Make a test namespace
#[cfg(test)]
mod tests {
    use sui_types::messages_checkpoint::FullCheckpointContents;

    use super::*;
    use std::path::{Path, PathBuf};

    async fn read_full_checkpoint(checkpoint_path: &PathBuf) -> anyhow::Result<CheckpointData> {
        let mut reader = fs::File::open(checkpoint_path.clone())?;
        let metadata = fs::metadata(checkpoint_path)?;
        let mut buffer = vec![0; metadata.len() as usize];
        reader.read_exact(&mut buffer)?;
        bcs::from_bytes(&buffer).map_err(|_| anyhow!("Unable to parse checkpoint file"))
    }

    // clippy ignore dead-code
    #[allow(dead_code)]
    async fn write_full_checkpoint(
        checkpoint_path: &Path,
        checkpoint: &CheckpointData,
    ) -> anyhow::Result<()> {
        let mut writer = fs::File::create(checkpoint_path)?;
        let bytes = bcs::to_bytes(&checkpoint)
            .map_err(|_| anyhow!("Unable to serialize checkpoint summary"))?;
        writer.write_all(&bytes)?;
        Ok(())
    }

    async fn read_data() -> (Committee, CheckpointData) {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("example_config/20873329.yaml");

        let mut reader = fs::File::open(d.clone()).unwrap();
        let metadata = fs::metadata(&d).unwrap();
        let mut buffer = vec![0; metadata.len() as usize];
        reader.read_exact(&mut buffer).unwrap();
        let checkpoint: Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>> =
            bcs::from_bytes(&buffer)
                .map_err(|_| anyhow!("Unable to parse checkpoint file"))
                .unwrap();

        let prev_committee = checkpoint
            .end_of_epoch_data
            .as_ref()
            .ok_or(anyhow!(
                "Expected all checkpoints to be end-of-epoch checkpoints"
            ))
            .unwrap()
            .next_epoch_committee
            .iter()
            .cloned()
            .collect();

        // Make a committee object using this
        let committee = Committee::new(checkpoint.epoch().checked_add(1).unwrap(), prev_committee);

        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("example_config/20958462.bcs");

        let full_checkpoint = read_full_checkpoint(&d).await.unwrap();

        (committee, full_checkpoint)
    }

    #[tokio::test]
    async fn test_checkpoint_all_good() {
        let (committee, full_checkpoint) = read_data().await;

        extract_verified_effects_and_events(
            &full_checkpoint,
            &committee,
            TransactionDigest::from_str("8RiKBwuAbtu8zNCtz8SrcfHyEUzto6zi6cMVA9t4WhWk").unwrap(),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_checkpoint_bad_committee() {
        let (mut committee, full_checkpoint) = read_data().await;

        // Change committee
        committee.epoch += 10;

        assert!(extract_verified_effects_and_events(
            &full_checkpoint,
            &committee,
            TransactionDigest::from_str("8RiKBwuAbtu8zNCtz8SrcfHyEUzto6zi6cMVA9t4WhWk").unwrap(),
        )
        .is_err());
    }

    #[tokio::test]
    async fn test_checkpoint_no_transaction() {
        let (committee, full_checkpoint) = read_data().await;

        assert!(extract_verified_effects_and_events(
            &full_checkpoint,
            &committee,
            TransactionDigest::from_str("8RiKBwuAbtu8zNCtz8SrcfHyEUzto6zj6cMVA9t4WhWk").unwrap(),
        )
        .is_err());
    }

    #[tokio::test]
    async fn test_checkpoint_bad_contents() {
        let (committee, mut full_checkpoint) = read_data().await;

        // Change contents
        let random_contents = FullCheckpointContents::random_for_testing();
        full_checkpoint.checkpoint_contents = random_contents.checkpoint_contents();

        assert!(extract_verified_effects_and_events(
            &full_checkpoint,
            &committee,
            TransactionDigest::from_str("8RiKBwuAbtu8zNCtz8SrcfHyEUzto6zj6cMVA9t4WhWk").unwrap(),
        )
        .is_err());
    }

    #[tokio::test]
    async fn test_checkpoint_bad_events() {
        let (committee, mut full_checkpoint) = read_data().await;

        let event = full_checkpoint.transactions[4]
            .events
            .as_ref()
            .unwrap()
            .data[0]
            .clone();

        for t in &mut full_checkpoint.transactions {
            if let Some(events) = &mut t.events {
                events.data.push(event.clone());
            }
        }

        assert!(extract_verified_effects_and_events(
            &full_checkpoint,
            &committee,
            TransactionDigest::from_str("8RiKBwuAbtu8zNCtz8SrcfHyEUzto6zj6cMVA9t4WhWk").unwrap(),
        )
        .is_err());
    }
}
