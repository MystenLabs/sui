// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use fastcrypto::traits::ToFromBytes;
use futures::future::join_all;
use futures::future::AbortHandle;
use itertools::Itertools;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::num::NonZeroUsize;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::{fs, io};
use sui_config::{genesis::Genesis, NodeConfig};
use sui_core::authority_client::{AuthorityAPI, NetworkAuthorityClient};
use sui_core::execution_cache::build_execution_cache_from_env;
use sui_network::default_mysten_network_config;
use sui_protocol_config::Chain;
use sui_sdk::SuiClient;
use sui_sdk::SuiClientBuilder;
use sui_storage::object_store::http::HttpDownloaderBuilder;
use sui_storage::object_store::util::Manifest;
use sui_storage::object_store::util::PerEpochManifest;
use sui_storage::object_store::util::MANIFEST_FILENAME;
use sui_types::accumulator::Accumulator;
use sui_types::committee::QUORUM_THRESHOLD;
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::messages_grpc::LayoutGenerationOption;
use sui_types::multiaddr::Multiaddr;
use sui_types::{base_types::*, object::Owner};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use anyhow::anyhow;
use clap::ValueEnum;
use eyre::ContextCompat;
use fastcrypto::hash::MultisetHash;
use futures::{StreamExt, TryStreamExt};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use prometheus::Registry;
use serde::{Deserialize, Serialize};
use sui_archival::reader::{ArchiveReader, ArchiveReaderMetrics};
use sui_archival::{verify_archive_with_checksums, verify_archive_with_genesis_config};
use sui_config::node::ArchiveReaderConfig;
use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_core::authority::authority_store_tables::AuthorityPerpetualTables;
use sui_core::authority::AuthorityStore;
use sui_core::checkpoints::CheckpointStore;
use sui_core::epoch::committee_store::CommitteeStore;
use sui_core::storage::RocksDbStore;
use sui_snapshot::reader::StateSnapshotReaderV1;
use sui_snapshot::setup_db_state;
use sui_storage::object_store::util::{copy_file, exists, get_path};
use sui_storage::object_store::ObjectStoreGetExt;
use sui_storage::verify_checkpoint_range;
use sui_types::messages_checkpoint::{CheckpointCommitment, ECMHLiveObjectSetDigest};
use sui_types::messages_grpc::{
    ObjectInfoRequest, ObjectInfoRequestKind, ObjectInfoResponse, TransactionInfoRequest,
    TransactionStatus,
};

use sui_types::storage::{ReadStore, SharedInMemoryStore};
use tracing::info;

pub mod commands;
pub mod db_tool;

#[derive(
    Clone, Serialize, Deserialize, Debug, PartialEq, Copy, PartialOrd, Ord, Eq, ValueEnum, Default,
)]
pub enum SnapshotVerifyMode {
    /// verification of both db state and downloaded checkpoints are skipped.
    /// This is the fastest mode, but is unsafe, and thus should only be used
    /// if you fully trust the source for both the snapshot and the checkpoint
    /// archive.
    None,
    /// verify snapshot state during download, but no post-restore db verification.
    /// Checkpoint verification is performed.
    #[default]
    Normal,
    /// In ADDITION to the behavior of `--verify normal`, verify db state post-restore
    /// against the end of epoch state root commitment.
    Strict,
}

// This functions requires at least one of genesis or fullnode_rpc to be `Some`.
async fn make_clients(
    sui_client: &Arc<SuiClient>,
) -> Result<BTreeMap<AuthorityName, (Multiaddr, NetworkAuthorityClient)>> {
    let mut net_config = default_mysten_network_config();
    net_config.connect_timeout = Some(Duration::from_secs(5));
    let mut authority_clients = BTreeMap::new();

    let active_validators = sui_client
        .governance_api()
        .get_latest_sui_system_state()
        .await?
        .active_validators;

    for validator in active_validators {
        let net_addr = Multiaddr::try_from(validator.net_address).unwrap();
        // TODO: Enable TLS on this interface with below config, once support is rolled out to validators.
        // let tls_config = sui_tls::create_rustls_client_config(
        //     sui_types::crypto::NetworkPublicKey::from_bytes(&validator.network_pubkey_bytes)?,
        //     sui_tls::SUI_VALIDATOR_SERVER_NAME.to_string(),
        //     None,
        // );
        let channel = net_config
            .connect_lazy(&net_addr, None)
            .map_err(|err| anyhow!(err.to_string()))?;
        let client = NetworkAuthorityClient::new(channel);
        let public_key_bytes =
            AuthorityPublicKeyBytes::from_bytes(&validator.protocol_pubkey_bytes)?;
        authority_clients.insert(public_key_bytes, (net_addr.clone(), client));
    }

    Ok(authority_clients)
}

type ObjectVersionResponses = (Option<SequenceNumber>, Result<ObjectInfoResponse>, f64);
pub struct ObjectData {
    requested_id: ObjectID,
    responses: Vec<(AuthorityName, Multiaddr, ObjectVersionResponses)>,
}

trait OptionDebug<T> {
    fn opt_debug(&self, def_str: &str) -> String;
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

#[allow(clippy::type_complexity)]
pub struct GroupedObjectOutput {
    pub grouped_results: BTreeMap<
        Option<(
            Option<SequenceNumber>,
            ObjectDigest,
            TransactionDigest,
            Owner,
            Option<TransactionDigest>,
        )>,
        Vec<AuthorityName>,
    >,
    pub voting_power: Vec<(
        Option<(
            Option<SequenceNumber>,
            ObjectDigest,
            TransactionDigest,
            Owner,
            Option<TransactionDigest>,
        )>,
        u64,
    )>,
    pub available_voting_power: u64,
    pub fully_locked: bool,
}

impl GroupedObjectOutput {
    pub fn new(
        object_data: ObjectData,
        committee: Arc<BTreeMap<AuthorityPublicKeyBytes, u64>>,
    ) -> Self {
        let mut grouped_results = BTreeMap::new();
        let mut voting_power = BTreeMap::new();
        let mut available_voting_power = 0;
        for (name, _, (version, resp, _elapsed)) in &object_data.responses {
            let stake = committee.get(name).unwrap();
            let key = match resp {
                Ok(r) => {
                    let obj_digest = r.object.compute_object_reference().2;
                    let parent_tx_digest = r.object.previous_transaction;
                    let owner = r.object.owner.clone();
                    let lock = r.lock_for_debugging.as_ref().map(|lock| *lock.digest());
                    if lock.is_none() {
                        available_voting_power += stake;
                    }
                    Some((*version, obj_digest, parent_tx_digest, owner, lock))
                }
                Err(_) => None,
            };
            let entry = grouped_results.entry(key.clone()).or_insert_with(Vec::new);
            entry.push(*name);
            let entry: &mut u64 = voting_power.entry(key).or_default();
            *entry += stake;
        }
        let voting_power = voting_power
            .into_iter()
            .sorted_by(|(_, v1), (_, v2)| Ord::cmp(v2, v1))
            .collect::<Vec<_>>();
        let mut fully_locked = false;
        if !voting_power.is_empty()
            && voting_power.first().unwrap().1 + available_voting_power < QUORUM_THRESHOLD
        {
            fully_locked = true;
        }
        Self {
            grouped_results,
            voting_power,
            available_voting_power,
            fully_locked,
        }
    }
}

#[allow(clippy::format_in_format_args)]
impl std::fmt::Display for GroupedObjectOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "available stake: {}", self.available_voting_power)?;
        writeln!(f, "fully locked: {}", self.fully_locked)?;
        writeln!(f, "{:<100}\n", "-".repeat(100))?;
        for (key, stake) in &self.voting_power {
            let val = self.grouped_results.get(key).unwrap();
            writeln!(f, "total stake: {stake}")?;
            match key {
                Some((_version, obj_digest, parent_tx_digest, owner, lock)) => {
                    let lock = lock.opt_debug("no-known-lock");
                    writeln!(f, "obj ref: {obj_digest}")?;
                    writeln!(f, "parent tx: {parent_tx_digest}")?;
                    writeln!(f, "owner: {owner}")?;
                    writeln!(f, "lock: {lock}")?;
                    for (i, name) in val.iter().enumerate() {
                        writeln!(f, "        {:<4} {:<20}", i, name.concise(),)?;
                    }
                }
                None => {
                    writeln!(f, "ERROR")?;
                    for (i, name) in val.iter().enumerate() {
                        writeln!(f, "        {:<4} {:<20}", i, name.concise(),)?;
                    }
                }
            };
            writeln!(f, "{:<100}\n", "-".repeat(100))?;
        }
        Ok(())
    }
}

struct ConciseObjectOutput(ObjectData);

impl ConciseObjectOutput {
    fn header() -> String {
        format!(
            "{:<20} {:<8} {:<66} {:<45} {}",
            "validator", "version", "digest", "parent_cert", "owner"
        )
    }
}

impl std::fmt::Display for ConciseObjectOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (name, _multi_addr, (version, resp, _time_elapsed)) in &self.0.responses {
            write!(
                f,
                "{:<20} {:<8}",
                format!("{:?}", name.concise()),
                version.map(|s| s.value()).opt_debug("-")
            )?;
            match resp {
                Err(_) => writeln!(
                    f,
                    "{:<66} {:<45} {:<51}",
                    "object-fetch-failed", "no-cert-available", "no-owner-available"
                )?,
                Ok(resp) => {
                    let obj_digest = resp.object.compute_object_reference().2;
                    let parent = resp.object.previous_transaction;
                    let owner = resp.object.owner.clone();
                    write!(f, " {:<66} {:<45} {:<51}", obj_digest, parent, owner)?;
                }
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

struct VerboseObjectOutput(ObjectData);

impl std::fmt::Display for VerboseObjectOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Object: {}", self.0.requested_id)?;

        for (name, multiaddr, (version, resp, timespent)) in &self.0.responses {
            writeln!(f, "validator: {:?}, addr: {:?}", name.concise(), multiaddr)?;
            writeln!(
                f,
                "-- version: {} ({:.3}s)",
                version.opt_debug("<version not available>"),
                timespent,
            )?;

            match resp {
                Err(e) => writeln!(f, "Error fetching object: {}", e)?,
                Ok(resp) => {
                    writeln!(
                        f,
                        "  -- object digest: {}",
                        resp.object.compute_object_reference().2
                    )?;
                    if resp.object.is_package() {
                        writeln!(f, "  -- object: <Move Package>")?;
                    } else if let Some(layout) = &resp.layout {
                        writeln!(
                            f,
                            "  -- object: Move Object: {}",
                            resp.object
                                .data
                                .try_as_move()
                                .unwrap()
                                .to_move_struct(layout)
                                .unwrap()
                        )?;
                    }
                    writeln!(f, "  -- owner: {}", resp.object.owner)?;
                    writeln!(
                        f,
                        "  -- locked by: {}",
                        resp.lock_for_debugging.opt_debug("<not locked>")
                    )?;
                }
            }
        }
        Ok(())
    }
}

pub async fn get_object(
    obj_id: ObjectID,
    version: Option<u64>,
    validator: Option<AuthorityName>,
    clients: Arc<BTreeMap<AuthorityName, (Multiaddr, NetworkAuthorityClient)>>,
) -> Result<ObjectData> {
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
            .map(|(name, (address, client))| async {
                let object_version = get_object_impl(client, obj_id, version).await;
                (*name, address.clone(), object_version)
            }),
    )
    .await;

    Ok(ObjectData {
        requested_id: obj_id,
        responses,
    })
}

pub async fn get_transaction_block(
    tx_digest: TransactionDigest,
    show_input_tx: bool,
    fullnode_rpc: String,
) -> Result<String> {
    let sui_client = Arc::new(SuiClientBuilder::default().build(fullnode_rpc).await?);
    let clients = make_clients(&sui_client).await?;
    let timer = Instant::now();
    let responses = join_all(clients.iter().map(|(name, (address, client))| async {
        let result = client
            .handle_transaction_info_request(TransactionInfoRequest {
                transaction_digest: tx_digest,
            })
            .await;
        (
            *name,
            address.clone(),
            result,
            timer.elapsed().as_secs_f64(),
        )
    }))
    .await;

    // Grab one validator that return Some(TransactionInfoResponse)
    let validator_aware_of_tx = responses.iter().find(|r| r.2.is_ok());

    let responses = responses
        .iter()
        .map(|r| {
            let key =
                r.2.as_ref()
                    .map(|ok_result| match &ok_result.status {
                        TransactionStatus::Signed(_) => None,
                        TransactionStatus::Executed(_, effects, _) => Some(effects.digest()),
                    })
                    .ok();
            let err = r.2.as_ref().err();
            (key, err, r)
        })
        .sorted_by(|(k1, err1, _), (k2, err2, _)| {
            Ord::cmp(k1, k2).then_with(|| Ord::cmp(err1, err2))
        })
        .chunk_by(|(_, _err, r)| {
            r.2.as_ref().map(|ok_result| match &ok_result.status {
                TransactionStatus::Signed(_) => None,
                TransactionStatus::Executed(_, effects, _) => Some((
                    ok_result.transaction.transaction_data(),
                    effects.data(),
                    effects.digest(),
                )),
            })
        });
    let mut s = String::new();
    for (i, (key, group)) in responses.into_iter().enumerate() {
        match key {
            Ok(Some((tx, effects, effects_digest))) => {
                writeln!(
                    &mut s,
                    "#{:<2} tx_digest: {:<68?} effects_digest: {:?}",
                    i, tx_digest, effects_digest,
                )?;
                writeln!(&mut s, "{:#?}", effects)?;
                if show_input_tx {
                    writeln!(&mut s, "{:#?}", tx)?;
                }
            }
            Ok(None) => {
                writeln!(
                    &mut s,
                    "#{:<2} tx_digest: {:<68?} Signed but not executed",
                    i, tx_digest
                )?;
                if show_input_tx {
                    // In this case, we expect at least one validator knows about this tx
                    let validator_aware_of_tx = validator_aware_of_tx.unwrap();
                    let client = &clients.get(&validator_aware_of_tx.0).unwrap().1;
                    let tx_info = client.handle_transaction_info_request(TransactionInfoRequest {
                        transaction_digest: tx_digest,
                    }).await.unwrap_or_else(|e| panic!("Validator {:?} should have known about tx_digest: {:?}, got error: {:?}", validator_aware_of_tx.0, tx_digest, e));
                    writeln!(&mut s, "{:#?}", tx_info)?;
                }
            }
            other => {
                writeln!(&mut s, "#{:<2} {:#?}", i, other)?;
            }
        }
        for (j, (_, _, res)) in group.enumerate() {
            writeln!(
                &mut s,
                "        {:<4} {:<20} {:<56} ({:.3}s)",
                j,
                res.0.concise(),
                format!("{}", res.1),
                res.3
            )?;
        }
        writeln!(&mut s, "{:<100}\n", "-".repeat(100))?;
    }
    Ok(s)
}

async fn get_object_impl(
    client: &NetworkAuthorityClient,
    id: ObjectID,
    version: Option<u64>,
) -> (Option<SequenceNumber>, Result<ObjectInfoResponse>, f64) {
    let start = Instant::now();
    let resp = client
        .handle_object_info_request(ObjectInfoRequest {
            object_id: id,
            generate_layout: LayoutGenerationOption::Generate,
            request_kind: match version {
                None => ObjectInfoRequestKind::LatestObjectInfo,
                Some(v) => ObjectInfoRequestKind::PastObjectInfoDebug(SequenceNumber::from_u64(v)),
            },
        })
        .await
        .map_err(anyhow::Error::from);
    let elapsed = start.elapsed().as_secs_f64();

    let resp_version = resp.as_ref().ok().map(|r| r.object.version().value());
    (resp_version.map(SequenceNumber::from), resp, elapsed)
}

pub(crate) fn make_anemo_config() -> anemo_cli::Config {
    use sui_network::discovery::*;
    use sui_network::state_sync::*;

    // TODO: implement `ServiceInfo` generation in anemo-build and use here.
    anemo_cli::Config::new()
        // Sui discovery
        .add_service(
            "Discovery",
            anemo_cli::ServiceInfo::new().add_method(
                "GetKnownPeersV2",
                anemo_cli::ron_method!(DiscoveryClient, get_known_peers_v2, ()),
            ),
        )
        // Sui state sync
        .add_service(
            "StateSync",
            anemo_cli::ServiceInfo::new()
                .add_method(
                    "PushCheckpointSummary",
                    anemo_cli::ron_method!(
                        StateSyncClient,
                        push_checkpoint_summary,
                        sui_types::messages_checkpoint::CertifiedCheckpointSummary
                    ),
                )
                .add_method(
                    "GetCheckpointSummary",
                    anemo_cli::ron_method!(
                        StateSyncClient,
                        get_checkpoint_summary,
                        GetCheckpointSummaryRequest
                    ),
                )
                .add_method(
                    "GetCheckpointContents",
                    anemo_cli::ron_method!(
                        StateSyncClient,
                        get_checkpoint_contents,
                        sui_types::messages_checkpoint::CheckpointContentsDigest
                    ),
                )
                .add_method(
                    "GetCheckpointAvailability",
                    anemo_cli::ron_method!(StateSyncClient, get_checkpoint_availability, ()),
                ),
        )
}

fn copy_dir_all(
    src: impl AsRef<Path>,
    dst: impl AsRef<Path>,
    skip: Vec<PathBuf>,
) -> io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if skip.contains(&entry.path()) {
            continue;
        }
        if ty.is_dir() {
            copy_dir_all(
                entry.path(),
                dst.as_ref().join(entry.file_name()),
                skip.clone(),
            )?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

pub async fn restore_from_db_checkpoint(
    config: &NodeConfig,
    db_checkpoint_path: &Path,
) -> Result<(), anyhow::Error> {
    copy_dir_all(db_checkpoint_path, config.db_path(), vec![])?;
    Ok(())
}

fn start_summary_sync(
    perpetual_db: Arc<AuthorityPerpetualTables>,
    committee_store: Arc<CommitteeStore>,
    checkpoint_store: Arc<CheckpointStore>,
    m: MultiProgress,
    genesis: Genesis,
    archive_store_config: ObjectStoreConfig,
    epoch: u64,
    num_parallel_downloads: usize,
    verify: bool,
    all_checkpoints: bool,
) -> JoinHandle<Result<(), anyhow::Error>> {
    tokio::spawn(async move {
        info!("Starting summary sync");
        let store = AuthorityStore::open_no_genesis(perpetual_db, false, &Registry::default())?;
        let cache_traits = build_execution_cache_from_env(&Registry::default(), &store);
        let state_sync_store =
            RocksDbStore::new(cache_traits, committee_store, checkpoint_store.clone());
        // Only insert the genesis checkpoint if the DB is empty and doesn't have it already
        if checkpoint_store
            .get_checkpoint_by_digest(genesis.checkpoint().digest())
            .unwrap()
            .is_none()
        {
            checkpoint_store.insert_checkpoint_contents(genesis.checkpoint_contents().clone())?;
            checkpoint_store.insert_verified_checkpoint(&genesis.checkpoint())?;
            checkpoint_store.update_highest_synced_checkpoint(&genesis.checkpoint())?;
        }
        // set up download of checkpoint summaries
        let config = ArchiveReaderConfig {
            remote_store_config: archive_store_config,
            download_concurrency: NonZeroUsize::new(num_parallel_downloads).unwrap(),
            use_for_pruning_watermark: false,
        };
        let metrics = ArchiveReaderMetrics::new(&Registry::default());
        let archive_reader = ArchiveReader::new(config, &metrics)?;
        archive_reader.sync_manifest_once().await?;
        let manifest = archive_reader.get_manifest().await?;

        let end_of_epoch_checkpoint_seq_nums = (0..=epoch)
            .map(|e| manifest.next_checkpoint_after_epoch(e) - 1)
            .collect::<Vec<_>>();
        let last_checkpoint = end_of_epoch_checkpoint_seq_nums
            .last()
            .expect("Expected at least one checkpoint");

        let num_to_sync = if all_checkpoints {
            *last_checkpoint
        } else {
            end_of_epoch_checkpoint_seq_nums.len() as u64
        };
        let sync_progress_bar = m.add(
            ProgressBar::new(num_to_sync).with_style(
                ProgressStyle::with_template("[{elapsed_precise}] {wide_bar} {pos}/{len} ({msg})")
                    .unwrap(),
            ),
        );

        let cloned_progress_bar = sync_progress_bar.clone();
        let sync_checkpoint_counter = Arc::new(AtomicU64::new(0));
        let s_instant = Instant::now();

        let cloned_counter = sync_checkpoint_counter.clone();
        let latest_synced = checkpoint_store
            .get_highest_synced_checkpoint()?
            .map(|c| c.sequence_number)
            .unwrap_or(0);
        let s_start = latest_synced
            .checked_add(1)
            .context("Checkpoint overflow")
            .map_err(|_| anyhow!("Failed to increment checkpoint"))?;
        tokio::spawn(async move {
            loop {
                if cloned_progress_bar.is_finished() {
                    break;
                }
                let num_summaries = cloned_counter.load(Ordering::Relaxed);
                let total_checkpoints_per_sec =
                    num_summaries as f64 / s_instant.elapsed().as_secs_f64();
                cloned_progress_bar.set_position(s_start + num_summaries);
                cloned_progress_bar.set_message(format!(
                    "checkpoints synced per sec: {}",
                    total_checkpoints_per_sec
                ));
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });

        if all_checkpoints {
            archive_reader
                .read_summaries_for_range_no_verify(
                    state_sync_store.clone(),
                    s_start..last_checkpoint + 1,
                    sync_checkpoint_counter,
                )
                .await?;
        } else {
            archive_reader
                .read_summaries_for_list_no_verify(
                    state_sync_store.clone(),
                    end_of_epoch_checkpoint_seq_nums.clone(),
                    sync_checkpoint_counter,
                )
                .await?;
        }
        sync_progress_bar.finish_with_message("Checkpoint summary sync is complete");

        let checkpoint = checkpoint_store
            .get_checkpoint_by_sequence_number(*last_checkpoint)?
            .ok_or(anyhow!("Failed to read last checkpoint"))?;
        if verify {
            let verify_progress_bar = m.add(
                ProgressBar::new(num_to_sync).with_style(
                    ProgressStyle::with_template(
                        "[{elapsed_precise}] {wide_bar} {pos}/{len} ({msg})",
                    )
                    .unwrap(),
                ),
            );
            let cloned_verify_progress_bar = verify_progress_bar.clone();
            let verify_checkpoint_counter = Arc::new(AtomicU64::new(0));
            let cloned_verify_counter = verify_checkpoint_counter.clone();
            let v_instant = Instant::now();

            tokio::spawn(async move {
                let v_start = if all_checkpoints { s_start } else { 0 };
                loop {
                    if cloned_verify_progress_bar.is_finished() {
                        break;
                    }
                    let num_summaries = cloned_verify_counter.load(Ordering::Relaxed);
                    let total_checkpoints_per_sec =
                        num_summaries as f64 / v_instant.elapsed().as_secs_f64();
                    cloned_verify_progress_bar.set_position(v_start + num_summaries);
                    cloned_verify_progress_bar.set_message(format!(
                        "checkpoints verified per sec: {}",
                        total_checkpoints_per_sec
                    ));
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            });

            if all_checkpoints {
                // in this case we need to verify all the checkpoints in the range pairwise
                let v_start = s_start;
                // update highest verified to be highest synced. We will move back
                // iff parallel verification succeeds
                let latest_verified = checkpoint_store
                    .get_checkpoint_by_sequence_number(latest_synced)
                    .expect("Failed to get checkpoint")
                    .expect("Expected checkpoint to exist after summary sync");
                checkpoint_store
                    .update_highest_verified_checkpoint(&latest_verified)
                    .expect("Failed to update highest verified checkpoint");

                let verify_range = v_start..last_checkpoint + 1;
                verify_checkpoint_range(
                    verify_range,
                    state_sync_store,
                    verify_checkpoint_counter,
                    num_parallel_downloads,
                )
                .await;
            } else {
                // in this case we only need to verify the end of epoch checkpoints by checking
                // signatures against the corresponding epoch committee.
                for (cp_epoch, epoch_last_cp_seq_num) in
                    end_of_epoch_checkpoint_seq_nums.iter().enumerate()
                {
                    let epoch_last_checkpoint = checkpoint_store
                        .get_checkpoint_by_sequence_number(*epoch_last_cp_seq_num)?
                        .ok_or(anyhow!("Failed to read checkpoint"))?;
                    let committee = state_sync_store.get_committee(cp_epoch as u64).expect(
                        "Expected committee to exist after syncing all end of epoch checkpoints",
                    );
                    epoch_last_checkpoint
                        .verify_authority_signatures(&committee)
                        .expect("Failed to verify checkpoint");
                    verify_checkpoint_counter.fetch_add(1, Ordering::Relaxed);
                }
            }

            verify_progress_bar.finish_with_message("Checkpoint summary verification is complete");
        }

        checkpoint_store.update_highest_verified_checkpoint(&checkpoint)?;
        checkpoint_store.update_highest_synced_checkpoint(&checkpoint)?;
        checkpoint_store.update_highest_executed_checkpoint(&checkpoint)?;
        checkpoint_store.update_highest_pruned_checkpoint(&checkpoint)?;
        Ok::<(), anyhow::Error>(())
    })
}

pub async fn get_latest_available_epoch(
    snapshot_store_config: &ObjectStoreConfig,
) -> Result<u64, anyhow::Error> {
    let remote_object_store = if snapshot_store_config.no_sign_request {
        snapshot_store_config.make_http()?
    } else {
        snapshot_store_config.make().map(Arc::new)?
    };
    let manifest_contents = remote_object_store
        .get_bytes(&get_path(MANIFEST_FILENAME))
        .await?;
    let root_manifest: Manifest = serde_json::from_slice(&manifest_contents)
        .map_err(|err| anyhow!("Error parsing MANIFEST from bytes: {}", err))?;
    let epoch = root_manifest
        .available_epochs
        .iter()
        .max()
        .ok_or(anyhow!("No snapshot found in manifest"))?;
    Ok(*epoch)
}

pub async fn check_completed_snapshot(
    snapshot_store_config: &ObjectStoreConfig,
    epoch: EpochId,
) -> Result<(), anyhow::Error> {
    let success_marker = format!("epoch_{}/_SUCCESS", epoch);
    let remote_object_store = if snapshot_store_config.no_sign_request {
        snapshot_store_config.make_http()?
    } else {
        snapshot_store_config.make().map(Arc::new)?
    };
    if exists(&remote_object_store, &get_path(success_marker.as_str())).await {
        Ok(())
    } else {
        Err(anyhow!(
            "missing success marker at {}/{}",
            snapshot_store_config.bucket.as_ref().unwrap_or(
                &snapshot_store_config
                    .clone()
                    .aws_endpoint
                    .unwrap_or("unknown_bucket".to_string())
            ),
            success_marker
        ))
    }
}

pub async fn download_formal_snapshot(
    path: &Path,
    epoch: EpochId,
    genesis: &Path,
    snapshot_store_config: ObjectStoreConfig,
    archive_store_config: ObjectStoreConfig,
    num_parallel_downloads: usize,
    network: Chain,
    verify: SnapshotVerifyMode,
    all_checkpoints: bool,
) -> Result<(), anyhow::Error> {
    let m = MultiProgress::new();
    m.println(format!(
        "Beginning formal snapshot restore to end of epoch {}, network: {:?}, verification mode: {:?}",
        epoch, network, verify,
    ))?;
    let path = path.join("staging").to_path_buf();
    if path.exists() {
        fs::remove_dir_all(path.clone())?;
    }
    let perpetual_db = Arc::new(AuthorityPerpetualTables::open(&path.join("store"), None));
    let genesis = Genesis::load(genesis).unwrap();
    let genesis_committee = genesis.committee()?;
    let committee_store = Arc::new(CommitteeStore::new(
        path.join("epochs"),
        &genesis_committee,
        None,
    ));
    let checkpoint_store = CheckpointStore::new(&path.join("checkpoints"));

    let summaries_handle = start_summary_sync(
        perpetual_db.clone(),
        committee_store.clone(),
        checkpoint_store.clone(),
        m.clone(),
        genesis.clone(),
        archive_store_config.clone(),
        epoch,
        num_parallel_downloads,
        verify != SnapshotVerifyMode::None,
        all_checkpoints,
    );
    let (_abort_handle, abort_registration) = AbortHandle::new_pair();
    let perpetual_db_clone = perpetual_db.clone();
    let snapshot_dir = path.parent().unwrap().join("snapshot");
    if snapshot_dir.exists() {
        fs::remove_dir_all(snapshot_dir.clone())?;
    }
    let snapshot_dir_clone = snapshot_dir.clone();

    // TODO if verify is false, we should skip generating these and
    // not pass in a channel to the reader
    let (sender, mut receiver) = mpsc::channel(num_parallel_downloads);
    let m_clone = m.clone();

    let snapshot_handle = tokio::spawn(async move {
        let local_store_config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(snapshot_dir_clone.to_path_buf()),
            ..Default::default()
        };
        let mut reader = StateSnapshotReaderV1::new(
            epoch,
            &snapshot_store_config,
            &local_store_config,
            NonZeroUsize::new(num_parallel_downloads).unwrap(),
            m_clone,
            false, // skip_reset_local_store
        )
        .await
        .unwrap_or_else(|err| panic!("Failed to create reader: {}", err));
        reader
            .read(&perpetual_db_clone, abort_registration, Some(sender))
            .await
            .unwrap_or_else(|err| panic!("Failed during read: {}", err));
        Ok::<(), anyhow::Error>(())
    });
    let mut root_accumulator = Accumulator::default();
    let mut num_live_objects = 0;
    while let Some((partial_acc, num_objects)) = receiver.recv().await {
        num_live_objects += num_objects;
        root_accumulator.union(&partial_acc);
    }
    summaries_handle
        .await
        .expect("Task join failed")
        .expect("Summaries task failed");

    let last_checkpoint = checkpoint_store
        .get_highest_verified_checkpoint()?
        .expect("Expected nonempty checkpoint store");

    // Perform snapshot state verification
    if verify != SnapshotVerifyMode::None {
        assert_eq!(
            last_checkpoint.epoch(),
            epoch,
            "Expected highest verified checkpoint ({}) to be for epoch {} but was for epoch {}",
            last_checkpoint.sequence_number,
            epoch,
            last_checkpoint.epoch()
        );
        let commitment = last_checkpoint
            .end_of_epoch_data
            .as_ref()
            .expect("Expected highest verified checkpoint to have end of epoch data")
            .epoch_commitments
            .last()
            .expect(
                "End of epoch has no commitments. This likely means that the epoch \
                you are attempting to restore from does not support end of epoch state \
                digest commitment. If restoring from mainnet, `--epoch` must be > 20, \
                and for testnet, `--epoch` must be > 12.",
            );
        match commitment {
            CheckpointCommitment::ECMHLiveObjectSetDigest(consensus_digest) => {
                let local_digest: ECMHLiveObjectSetDigest = root_accumulator.digest().into();
                assert_eq!(
                    *consensus_digest, local_digest,
                    "End of epoch {} root state digest {} does not match \
                    local root state hash {} computed from snapshot data",
                    epoch, consensus_digest.digest, local_digest.digest,
                );
                let progress_bar = m.add(
                    ProgressBar::new(1).with_style(
                        ProgressStyle::with_template(
                            "[{elapsed_precise}] {wide_bar} Verifying snapshot contents against root state hash ({msg})",
                        )
                        .unwrap(),
                    ),
                );
                progress_bar.finish_with_message("Verification complete");
            }
        };
    } else {
        m.println(
            "WARNING: Skipping snapshot verification! \
            This is highly discouraged unless you fully trust the source of this snapshot and its contents.
            If this was unintentional, rerun with `--verify` set to `normal` or `strict`.",
        )?;
    }

    snapshot_handle
        .await
        .expect("Task join failed")
        .expect("Snapshot restore task failed");

    // TODO we should ensure this map is being updated for all end of epoch
    // checkpoints during summary sync. This happens in `insert_{verified|certified}_checkpoint`
    // in checkpoint store, but not in the corresponding functions in ObjectStore trait
    checkpoint_store.insert_epoch_last_checkpoint(epoch, &last_checkpoint)?;

    setup_db_state(
        epoch,
        root_accumulator.clone(),
        perpetual_db.clone(),
        checkpoint_store,
        committee_store,
        network,
        verify == SnapshotVerifyMode::Strict,
        num_live_objects,
        m,
    )
    .await?;

    let new_path = path.parent().unwrap().join("live");
    if new_path.exists() {
        fs::remove_dir_all(new_path.clone())?;
    }
    fs::rename(&path, &new_path)?;
    fs::remove_dir_all(snapshot_dir.clone())?;
    println!(
        "Successfully restored state from snapshot at end of epoch {}",
        epoch
    );

    Ok(())
}

pub async fn download_db_snapshot(
    path: &Path,
    epoch: u64,
    snapshot_store_config: ObjectStoreConfig,
    skip_indexes: bool,
    num_parallel_downloads: usize,
) -> Result<(), anyhow::Error> {
    let remote_store = if snapshot_store_config.no_sign_request {
        snapshot_store_config.make_http()?
    } else {
        snapshot_store_config.make().map(Arc::new)?
    };

    // We rely on the top level MANIFEST file which contains all valid epochs
    let manifest_contents = remote_store.get_bytes(&get_path(MANIFEST_FILENAME)).await?;
    let root_manifest: Manifest = serde_json::from_slice(&manifest_contents)
        .map_err(|err| anyhow!("Error parsing MANIFEST from bytes: {}", err))?;

    if !root_manifest.epoch_exists(epoch) {
        return Err(anyhow!(
            "Epoch dir {} doesn't exist on the remote store",
            epoch
        ));
    }

    let epoch_path = format!("epoch_{}", epoch);
    let epoch_dir = get_path(&epoch_path);

    let manifest_file = epoch_dir.child(MANIFEST_FILENAME);
    let epoch_manifest_contents =
        String::from_utf8(remote_store.get_bytes(&manifest_file).await?.to_vec())
            .map_err(|err| anyhow!("Error parsing {}/MANIFEST from bytes: {}", epoch_path, err))?;

    let epoch_manifest =
        PerEpochManifest::deserialize_from_newline_delimited(&epoch_manifest_contents);

    let mut files: Vec<String> = vec![];
    files.extend(epoch_manifest.filter_by_prefix("store/perpetual").lines);
    files.extend(epoch_manifest.filter_by_prefix("epochs").lines);
    files.extend(epoch_manifest.filter_by_prefix("checkpoints").lines);
    if !skip_indexes {
        files.extend(epoch_manifest.filter_by_prefix("indexes").lines)
    }
    let local_store = ObjectStoreConfig {
        object_store: Some(ObjectStoreType::File),
        directory: Some(path.to_path_buf()),
        ..Default::default()
    }
    .make()?;
    let m = MultiProgress::new();
    let path = path.to_path_buf();
    let snapshot_handle = tokio::spawn(async move {
        let progress_bar = m.add(
            ProgressBar::new(files.len() as u64).with_style(
                ProgressStyle::with_template(
                    "[{elapsed_precise}] {wide_bar} {pos} out of {len} files done ({msg})",
                )
                .unwrap(),
            ),
        );
        let cloned_progress_bar = progress_bar.clone();
        let file_counter = Arc::new(AtomicUsize::new(0));
        futures::stream::iter(files.iter())
            .map(|file| {
                let local_store = local_store.clone();
                let remote_store = remote_store.clone();
                let counter_cloned = file_counter.clone();
                async move {
                    counter_cloned.fetch_add(1, Ordering::Relaxed);
                    let file_path = get_path(format!("epoch_{}/{}", epoch, file).as_str());
                    copy_file(&file_path, &file_path, &remote_store, &local_store).await?;
                    Ok::<::object_store::path::Path, anyhow::Error>(file_path.clone())
                }
            })
            .boxed()
            .buffer_unordered(num_parallel_downloads)
            .try_for_each(|path| {
                file_counter.fetch_sub(1, Ordering::Relaxed);
                cloned_progress_bar.inc(1);
                cloned_progress_bar.set_message(format!(
                    "Downloading file: {}, #downloads_in_progress: {}",
                    path,
                    file_counter.load(Ordering::Relaxed)
                ));
                futures::future::ready(Ok(()))
            })
            .await?;
        progress_bar.finish_with_message("Snapshot file download is complete");
        Ok::<(), anyhow::Error>(())
    });

    let tasks: Vec<_> = vec![Box::pin(snapshot_handle)];
    join_all(tasks)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .for_each(|result| result.expect("Task failed"));

    let store_dir = path.join("store");
    if store_dir.exists() {
        fs::remove_dir_all(&store_dir)?;
    }
    let epochs_dir = path.join("epochs");
    if epochs_dir.exists() {
        fs::remove_dir_all(&epochs_dir)?;
    }
    Ok(())
}

pub async fn verify_archive(
    genesis: &Path,
    remote_store_config: ObjectStoreConfig,
    concurrency: usize,
    interactive: bool,
) -> Result<()> {
    verify_archive_with_genesis_config(genesis, remote_store_config, concurrency, interactive, 10)
        .await
}

pub async fn dump_checkpoints_from_archive(
    remote_store_config: ObjectStoreConfig,
    start_checkpoint: u64,
    end_checkpoint: u64,
    max_content_length: usize,
) -> Result<()> {
    let metrics = ArchiveReaderMetrics::new(&Registry::default());
    let config = ArchiveReaderConfig {
        remote_store_config,
        download_concurrency: NonZeroUsize::new(1).unwrap(),
        use_for_pruning_watermark: false,
    };
    let store = SharedInMemoryStore::default();
    let archive_reader = ArchiveReader::new(config, &metrics)?;
    archive_reader.sync_manifest_once().await?;
    let checkpoint_counter = Arc::new(AtomicU64::new(0));
    let txn_counter = Arc::new(AtomicU64::new(0));
    archive_reader
        .read(
            store.clone(),
            Range {
                start: start_checkpoint,
                end: end_checkpoint,
            },
            txn_counter,
            checkpoint_counter,
            false,
        )
        .await?;
    for key in store
        .inner()
        .checkpoints()
        .values()
        .sorted_by(|a, b| a.sequence_number().cmp(&b.sequence_number))
    {
        let mut content = serde_json::to_string(
            &store
                .get_full_checkpoint_contents_by_sequence_number(key.sequence_number)
                .unwrap(),
        )?;
        content.truncate(max_content_length);
        info!(
            "{}:{}:{:?}",
            key.sequence_number, key.content_digest, content
        );
    }
    Ok(())
}

pub async fn verify_archive_by_checksum(
    remote_store_config: ObjectStoreConfig,
    concurrency: usize,
) -> Result<()> {
    verify_archive_with_checksums(remote_store_config, concurrency).await
}
