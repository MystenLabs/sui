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
use sui_network::default_mysten_network_config;
use sui_protocol_config::Chain;
use sui_sdk::SuiClientBuilder;
use sui_types::accumulator::Accumulator;
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::messages_grpc::LayoutGenerationOption;
use sui_types::multiaddr::Multiaddr;
use sui_types::{base_types::*, object::Owner};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use ::object_store::ObjectMeta;
use anyhow::anyhow;
use eyre::ContextCompat;
use fastcrypto::hash::MultisetHash;
use futures::{StreamExt, TryStreamExt};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use prometheus::Registry;
use sui_archival::reader::{ArchiveReader, ArchiveReaderMetrics};
use sui_archival::{verify_archive_with_checksums, verify_archive_with_genesis_config};
use sui_config::node::ArchiveReaderConfig;
use sui_core::authority::authority_store_tables::AuthorityPerpetualTables;
use sui_core::authority::AuthorityStore;
use sui_core::checkpoints::CheckpointStore;
use sui_core::db_checkpoint_handler::SUCCESS_MARKER;
use sui_core::epoch::committee_store::CommitteeStore;
use sui_core::storage::RocksDbStore;
use sui_snapshot::reader::StateSnapshotReaderV1;
use sui_snapshot::setup_db_state;
use sui_storage::object_store::util::{copy_file, get_path};
use sui_storage::object_store::{ObjectStoreConfig, ObjectStoreType};
use sui_storage::verify_checkpoint_range;
use sui_types::messages_checkpoint::{CheckpointCommitment, ECMHLiveObjectSetDigest};
use sui_types::messages_grpc::{
    ObjectInfoRequest, ObjectInfoRequestKind, ObjectInfoResponse, TransactionInfoRequest,
    TransactionStatus,
};

use sui_types::storage::{ReadStore, SharedInMemoryStore};
use tracing::info;
use typed_store::rocks::MetricConf;

pub mod commands;
pub mod db_tool;
pub mod pkg_dump;

// This functions requires at least one of genesis or fullnode_rpc to be `Some`.
async fn make_clients(
    genesis: Option<PathBuf>,
    fullnode_rpc: Option<String>,
) -> Result<BTreeMap<AuthorityName, (Multiaddr, NetworkAuthorityClient)>> {
    let mut net_config = default_mysten_network_config();
    net_config.connect_timeout = Some(Duration::from_secs(5));
    let mut authority_clients = BTreeMap::new();

    if let Some(fullnode_rpc) = fullnode_rpc {
        let sui_client = SuiClientBuilder::default().build(fullnode_rpc).await?;
        let active_validators = sui_client
            .governance_api()
            .get_latest_sui_system_state()
            .await?
            .active_validators;

        for validator in active_validators {
            let net_addr = Multiaddr::try_from(validator.net_address).unwrap();
            let channel = net_config
                .connect_lazy(&net_addr)
                .map_err(|err| anyhow!(err.to_string()))?;
            let client = NetworkAuthorityClient::new(channel);
            let public_key_bytes =
                AuthorityPublicKeyBytes::from_bytes(&validator.protocol_pubkey_bytes)?;
            authority_clients.insert(public_key_bytes, (net_addr.clone(), client));
        }
    } else {
        if genesis.is_none() {
            return Err(anyhow!("Either genesis or fullnode_rpc must be specified"));
        }
        let genesis = Genesis::load(genesis.unwrap())?;
        for validator in genesis.validator_set_for_tooling() {
            let metadata = validator.verified_metadata();
            let channel = net_config
                .connect_lazy(&metadata.net_address)
                .map_err(|err| anyhow!(err.to_string()))?;
            let client = NetworkAuthorityClient::new(channel);
            let public_key_bytes = metadata.sui_pubkey_bytes();
            authority_clients.insert(public_key_bytes, (metadata.net_address.clone(), client));
        }
    }

    Ok(authority_clients)
}

type ObjectVersionResponses = Vec<(Option<SequenceNumber>, Result<ObjectInfoResponse>, f64)>;
pub struct ObjectData {
    requested_id: ObjectID,
    responses: Vec<(AuthorityName, Multiaddr, ObjectVersionResponses)>,
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

pub struct GroupedObjectOutput(pub ObjectData);

#[allow(clippy::format_in_format_args)]
impl std::fmt::Display for GroupedObjectOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let responses = self
            .0
            .responses
            .iter()
            .flat_map(|(name, multiaddr, resp)| {
                resp.iter().map(|(seq_num, r, timespent)| {
                    (
                        *name,
                        multiaddr.clone(),
                        seq_num,
                        r,
                        timespent,
                        r.as_ref().err(),
                    )
                })
            })
            .sorted_by(|a, b| {
                Ord::cmp(&b.2, &a.2)
                    .then_with(|| Ord::cmp(&format!("{:?}", &b.5), &format!("{:?}", &a.5)))
            })
            .group_by(|(_, _, seq_num, _r, _ts, _)| **seq_num);
        for (seq_num, group) in &responses {
            writeln!(f, "seq num: {}", seq_num.opt_debug("latest-seq-num"))?;
            let cur_version_resp = group.group_by(|(_, _, _, r, _, _)| match r {
                Ok(result) => {
                    let parent_tx_digest = result.object.previous_transaction;
                    let obj_digest = result.object.compute_object_reference().2;
                    let lock = result
                        .lock_for_debugging
                        .as_ref()
                        .map(|lock| *lock.digest());
                    let owner = result.object.owner;
                    Some((parent_tx_digest, obj_digest, lock, owner))
                }
                Err(_) => None,
            });
            for (result, group) in &cur_version_resp {
                match result {
                    Some((parent_tx_digest, obj_digest, lock, owner)) => {
                        let lock = lock.opt_debug("no-known-lock");
                        writeln!(f, "obj ref: {obj_digest}")?;
                        writeln!(f, "parent tx: {parent_tx_digest}")?;
                        writeln!(f, "owner: {owner}")?;
                        writeln!(f, "lock: {lock}")?;
                        for (i, (name, multiaddr, _, _, timespent, _)) in group.enumerate() {
                            writeln!(
                                f,
                                "        {:<4} {:<20} {:<56} ({:.3}s)",
                                i,
                                name.concise(),
                                format!("{}", multiaddr),
                                timespent
                            )?;
                        }
                    }
                    None => {
                        writeln!(f, "ERROR")?;
                        for (i, (name, multiaddr, _, resp, timespent, _)) in group.enumerate() {
                            writeln!(
                                f,
                                "        {:<4} {:<20} {:<56} ({:.3}s) {:?}",
                                i,
                                name.concise(),
                                format!("{}", multiaddr),
                                timespent,
                                resp
                            )?;
                        }
                    }
                };
                writeln!(f, "{:<100}\n", "-".repeat(100))?;
            }
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
        for (name, _multi_addr, versions) in &self.0.responses {
            for (version, resp, _time_elapsed) in versions {
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
                        let owner = resp.object.owner;
                        write!(f, " {:<66} {:<45} {:<51}", obj_digest, parent, owner)?;
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

        for (name, multiaddr, versions) in &self.0.responses {
            writeln!(f, "validator: {:?}, addr: {:?}", name.concise(), multiaddr)?;

            for (version, resp, timespent) in versions {
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
        }
        Ok(())
    }
}

pub async fn get_object(
    obj_id: ObjectID,
    version: Option<u64>,
    validator: Option<AuthorityName>,
    genesis: Option<PathBuf>,
    fullnode_rpc: Option<String>,
) -> Result<ObjectData> {
    let clients = make_clients(genesis, fullnode_rpc).await?;

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
                let object_versions = get_object_impl(client, obj_id, version).await;
                (*name, address.clone(), object_versions)
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
    genesis: Option<PathBuf>,
    show_input_tx: bool,
    fullnode_rpc: Option<String>,
) -> Result<String> {
    let clients = make_clients(genesis, fullnode_rpc).await?;
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
        .group_by(|(_, _err, r)| {
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

// Keep the return type a vector in case we need support for lamport versions in the near future
async fn get_object_impl(
    client: &NetworkAuthorityClient,
    id: ObjectID,
    version: Option<u64>,
) -> Vec<(Option<SequenceNumber>, Result<ObjectInfoResponse>, f64)> {
    let mut ret = Vec::new();

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
    ret.push((resp_version.map(SequenceNumber::from), resp, elapsed));

    ret
}

pub(crate) fn make_anemo_config() -> anemo_cli::Config {
    use narwhal_types::*;
    use sui_network::discovery::*;
    use sui_network::state_sync::*;

    // TODO: implement `ServiceInfo` generation in anemo-build and use here.
    anemo_cli::Config::new()
        // Narwhal primary-to-primary
        .add_service(
            "PrimaryToPrimary",
            anemo_cli::ServiceInfo::new()
                .add_method(
                    "SendCertificate",
                    anemo_cli::ron_method!(
                        PrimaryToPrimaryClient,
                        send_certificate,
                        SendCertificateRequest
                    ),
                )
                .add_method(
                    "RequestVote",
                    anemo_cli::ron_method!(
                        PrimaryToPrimaryClient,
                        request_vote,
                        RequestVoteRequest
                    ),
                )
                .add_method(
                    "FetchCertificates",
                    anemo_cli::ron_method!(
                        PrimaryToPrimaryClient,
                        fetch_certificates,
                        FetchCertificatesRequest
                    ),
                ),
        )
        // Narwhal worker-to-worker
        .add_service(
            "WorkerToWorker",
            anemo_cli::ServiceInfo::new()
                .add_method(
                    "ReportBatch",
                    anemo_cli::ron_method!(WorkerToWorkerClient, report_batch, WorkerBatchMessage),
                )
                .add_method(
                    "RequestBatches",
                    anemo_cli::ron_method!(
                        WorkerToWorkerClient,
                        request_batches,
                        RequestBatchesRequest
                    ),
                ),
        )
        // Sui discovery
        .add_service(
            "Discovery",
            anemo_cli::ServiceInfo::new().add_method(
                "GetKnownPeers",
                anemo_cli::ron_method!(DiscoveryClient, get_known_peers, ()),
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
) -> JoinHandle<Result<(), anyhow::Error>> {
    tokio::spawn(async move {
        info!("Starting summary sync");
        let store = AuthorityStore::open(
            perpetual_db,
            &genesis,
            &committee_store,
            usize::MAX,
            false,
            &Registry::default(),
        )
        .await?;
        let state_sync_store = RocksDbStore::new(store, committee_store, checkpoint_store.clone());
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

        let last_checkpoint = manifest.next_checkpoint_after_epoch(epoch) - 1;
        let sync_progress_bar = m.add(
            ProgressBar::new(last_checkpoint).with_style(
                ProgressStyle::with_template("[{elapsed_precise}] {wide_bar} {pos}/{len}({msg})")
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

        let sync_range = s_start..last_checkpoint + 1;
        archive_reader
            .read_summaries(
                state_sync_store.clone(),
                sync_range.clone(),
                sync_checkpoint_counter,
                // rather than blocking on verify, sync all summaries first, then verify later
                false,
            )
            .await?;
        sync_progress_bar.finish_with_message("Checkpoint summary sync is complete");

        // verify checkpoint summaries
        if verify {
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
            let verify_progress_bar = m.add(
                ProgressBar::new(last_checkpoint).with_style(
                    ProgressStyle::with_template(
                        "[{elapsed_precise}] {wide_bar} {pos}/{len}({msg})",
                    )
                    .unwrap(),
                ),
            );
            let cloned_verify_progress_bar = verify_progress_bar.clone();
            let verify_checkpoint_counter = Arc::new(AtomicU64::new(0));
            let cloned_verify_counter = verify_checkpoint_counter.clone();
            let v_instant = Instant::now();

            tokio::spawn(async move {
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

            let verify_range = v_start..last_checkpoint + 1;
            verify_checkpoint_range(
                verify_range,
                state_sync_store,
                verify_checkpoint_counter,
                num_parallel_downloads,
            )
            .await;
            verify_progress_bar.finish_with_message("Checkpoint summary verification is complete");
        }

        let checkpoint = checkpoint_store
            .get_checkpoint_by_sequence_number(last_checkpoint)?
            .ok_or(anyhow!("Failed to read last checkpoint"))?;

        checkpoint_store.update_highest_verified_checkpoint(&checkpoint)?;
        checkpoint_store.update_highest_synced_checkpoint(&checkpoint)?;
        checkpoint_store.update_highest_executed_checkpoint(&checkpoint)?;
        checkpoint_store.update_highest_pruned_checkpoint(&checkpoint)?;
        Ok::<(), anyhow::Error>(())
    })
}

pub async fn download_formal_snapshot(
    path: &Path,
    epoch: EpochId,
    genesis: &Path,
    snapshot_store_config: ObjectStoreConfig,
    archive_store_config: ObjectStoreConfig,
    num_parallel_downloads: usize,
    network: Chain,
    verify: bool,
) -> Result<(), anyhow::Error> {
    eprintln!(
        "Beginning formal snapshot restore to end of epoch {}, network: {:?}",
        epoch, network,
    );
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
    let checkpoint_store = Arc::new(CheckpointStore::open_tables_read_write(
        path.join("checkpoints"),
        MetricConf::default(),
        None,
        None,
    ));

    let m = MultiProgress::new();
    let summaries_handle = start_summary_sync(
        perpetual_db.clone(),
        committee_store.clone(),
        checkpoint_store.clone(),
        m.clone(),
        genesis.clone(),
        archive_store_config.clone(),
        epoch,
        num_parallel_downloads,
        verify,
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
            usize::MAX,
            NonZeroUsize::new(num_parallel_downloads).unwrap(),
            m,
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
    while let Some(partial_acc) = receiver.recv().await {
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
    if verify {
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
                    local root state hash {} after restoring from formal snapshot",
                    epoch, consensus_digest.digest, local_digest.digest,
                );
                eprintln!("Formal snapshot state verification completed successfully!");
            }
        };
    } else {
        eprintln!(
            "WARNING: Skipping snapshot verification! \
            This is highly discouraged unless you fully trust the source of this snapshot and its contents.
            If this was unintentional, rerun with `--verify` set to `true`"
        );
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
        root_accumulator,
        perpetual_db,
        checkpoint_store,
        committee_store,
    )
    .await?;

    let new_path = path.parent().unwrap().join("live");
    if new_path.exists() {
        fs::remove_dir_all(new_path.clone())?;
    }
    fs::rename(&path, &new_path)?;
    fs::remove_dir_all(snapshot_dir.clone())?;
    info!(
        "Successfully restored state from snapshot at end of epoch {}",
        epoch
    );

    Ok(())
}

pub async fn download_db_snapshot(
    path: &Path,
    epoch: u64,
    genesis: &Path,
    snapshot_store_config: ObjectStoreConfig,
    archive_store_config: ObjectStoreConfig,
    skip_checkpoints: bool,
    skip_indexes: bool,
    num_parallel_downloads: usize,
) -> Result<(), anyhow::Error> {
    // TODO: Enable downloading db snapshots with no sign requests
    let remote_store = snapshot_store_config.make()?;
    let entries = remote_store.list_with_delimiter(None).await?;
    let epoch_path = format!("epoch_{}", epoch);
    let epoch_dir = entries
        .common_prefixes
        .iter()
        .find(|entry| {
            entry
                .filename()
                .map(|filename| filename == epoch_path)
                .unwrap_or(false)
        })
        .ok_or(anyhow!("Epoch dir doesn't exist on the remote store"))?;
    let success_marker = epoch_dir.child(SUCCESS_MARKER);
    let _get_result = remote_store.get(&success_marker).await?;
    let store_entries = remote_store
        .list_with_delimiter(Some(&get_path(&format!("{}/store", epoch_path))))
        .await?;
    let perpetual_dir = store_entries
        .common_prefixes
        .iter()
        .find(|entry| {
            entry
                .filename()
                .map(|filename| filename == "perpetual")
                .unwrap_or(false)
        })
        .ok_or(anyhow!(
            "Perpetual dir doesn't exist under the remote epoch dir"
        ))?;
    let entries = remote_store
        .list_with_delimiter(Some(&get_path(&epoch_path)))
        .await?;
    let committee_dir = entries
        .common_prefixes
        .iter()
        .find(|entry| {
            entry
                .filename()
                .map(|filename| filename == "epochs")
                .unwrap_or(false)
        })
        .ok_or(anyhow!(
            "Epochs dir doesn't exist under the remote epoch dir"
        ))?;
    let mut files: Vec<ObjectMeta> = vec![];
    files.extend(
        remote_store
            .list_with_delimiter(Some(committee_dir))
            .await?
            .objects,
    );
    files.extend(
        remote_store
            .list_with_delimiter(Some(perpetual_dir))
            .await?
            .objects,
    );
    if !skip_checkpoints {
        let checkpoints_dir = entries
            .common_prefixes
            .iter()
            .find(|entry| {
                entry
                    .filename()
                    .map(|filename| filename == "checkpoints")
                    .unwrap_or(false)
            })
            .ok_or(anyhow!(
                "Checkpoints dir doesn't exist under the remote epoch dir"
            ))?;
        files.extend(
            remote_store
                .list_with_delimiter(Some(checkpoints_dir))
                .await?
                .objects,
        );
    }
    if !skip_indexes {
        let indexes_dir = entries
            .common_prefixes
            .iter()
            .find(|entry| {
                entry
                    .filename()
                    .map(|filename| filename == "indexes")
                    .unwrap_or(false)
            })
            .ok_or(anyhow!(
                "Indexes dir doesn't exist under the remote epoch dir"
            ))?;
        files.extend(
            remote_store
                .list_with_delimiter(Some(indexes_dir))
                .await?
                .objects,
        );
    }
    let total_bytes: usize = files.iter().map(|f| f.size).sum();
    info!(
        "Total bytes to download: {}MiB",
        total_bytes as f64 / (1024 * 1024) as f64
    );
    let local_store = ObjectStoreConfig {
        object_store: Some(ObjectStoreType::File),
        directory: Some(path.to_path_buf()),
        ..Default::default()
    }
    .make()?;
    let m = MultiProgress::new();
    let path = path.to_path_buf();
    let genesis = genesis.to_path_buf();
    let perpetual_db = Arc::new(AuthorityPerpetualTables::open(
        &path.join(format!("epoch_{}", epoch)).join("store"),
        None,
    ));
    let summaries_handle = if skip_checkpoints {
        let perpetual_db = Arc::new(AuthorityPerpetualTables::open(&path.join("store"), None));
        let genesis = Genesis::load(genesis).unwrap();
        let genesis_committee = genesis.committee()?;
        let committee_store = Arc::new(CommitteeStore::new(
            path.join("epochs"),
            &genesis_committee,
            None,
        ));
        let checkpoint_store = Arc::new(CheckpointStore::open_tables_read_write(
            path.join("checkpoints"),
            MetricConf::default(),
            None,
            None,
        ));
        Some(start_summary_sync(
            perpetual_db,
            committee_store,
            checkpoint_store,
            m.clone(),
            genesis,
            archive_store_config,
            epoch,
            num_parallel_downloads,
            false, // verify
        ))
    } else {
        None
    };
    let snapshot_handle = tokio::spawn(async move {
        let progress_bar = m.add(
            ProgressBar::new(files.len() as u64).with_style(
                ProgressStyle::with_template(
                    "[{elapsed_precise}] {wide_bar} {pos} out of {len} files done\n({msg})",
                )
                .unwrap(),
            ),
        );
        let cloned_progress_bar = progress_bar.clone();
        let mut instant = Instant::now();
        let downloaded_bytes = AtomicUsize::new(0);
        let file_counter = Arc::new(AtomicUsize::new(0));
        futures::stream::iter(files.iter())
            .map(|file| {
                let local_store = local_store.clone();
                let remote_store = remote_store.clone();
                let counter_cloned = file_counter.clone();
                async move {
                    counter_cloned.fetch_add(1, Ordering::Relaxed);
                    copy_file(&file.location, &file.location, &remote_store, &local_store).await?;
                    Ok::<(::object_store::path::Path, usize), anyhow::Error>((
                        file.location.clone(),
                        file.size,
                    ))
                }
            })
            .boxed()
            .buffer_unordered(num_parallel_downloads)
            .try_for_each(|(path, bytes)| {
                file_counter.fetch_sub(1, Ordering::Relaxed);
                downloaded_bytes.fetch_add(bytes, Ordering::Relaxed);
                cloned_progress_bar.inc(1);
                cloned_progress_bar.set_message(format!(
                    "Download speed: {} MiB/s, file: {}, #downloads_in_progress: {}",
                    downloaded_bytes.load(Ordering::Relaxed) as f64
                        / (1024 * 1024) as f64
                        / instant.elapsed().as_secs_f64(),
                    path,
                    file_counter.load(Ordering::Relaxed)
                ));
                instant = Instant::now();
                downloaded_bytes.store(0, Ordering::Relaxed);
                futures::future::ready(Ok(()))
            })
            .await?;
        progress_bar.finish_with_message("Snapshot file download is complete");
        Ok::<(), anyhow::Error>(())
    });

    let mut tasks: Vec<_> = vec![Box::pin(snapshot_handle)];
    if let Some(summary_handle) = summaries_handle {
        tasks.push(Box::pin(summary_handle));
    }
    join_all(tasks)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .for_each(|result| result.expect("Task failed"));
    if skip_checkpoints {
        let checkpoint_store = Arc::new(CheckpointStore::open_tables_read_write(
            path.join("checkpoints"),
            MetricConf::default(),
            None,
            None,
        ));
        let last_checkpoint = checkpoint_store
            .get_highest_verified_checkpoint()?
            .expect("Expected nonempty checkpoint store");
        perpetual_db.set_highest_pruned_checkpoint_without_wb(last_checkpoint.sequence_number)?;
    }

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
                .get_full_checkpoint_contents_by_sequence_number(key.sequence_number)?
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

pub async fn state_sync_from_archive(
    path: &Path,
    genesis: &Path,
    remote_store_config: ObjectStoreConfig,
    concurrency: usize,
) -> Result<()> {
    let genesis = Genesis::load(genesis).unwrap();
    let genesis_committee = genesis.committee()?;

    let checkpoint_store = Arc::new(CheckpointStore::open_tables_read_write(
        path.join("checkpoints"),
        MetricConf::default(),
        None,
        None,
    ));
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

    let perpetual_db = Arc::new(AuthorityPerpetualTables::open(&path.join("store"), None));

    let committee_store = Arc::new(CommitteeStore::new(
        path.join("epochs"),
        &genesis_committee,
        None,
    ));

    let store = AuthorityStore::open(
        perpetual_db,
        &genesis,
        &committee_store,
        usize::MAX,
        false,
        &Registry::default(),
    )
    .await?;

    let latest_checkpoint = checkpoint_store
        .get_highest_synced_checkpoint()?
        .map(|c| c.sequence_number)
        .unwrap_or(0);
    let state_sync_store = RocksDbStore::new(store, committee_store, checkpoint_store.clone());
    let archive_reader_config = ArchiveReaderConfig {
        remote_store_config,
        download_concurrency: NonZeroUsize::new(concurrency).unwrap(),
        use_for_pruning_watermark: false,
    };
    let metrics = ArchiveReaderMetrics::new(&Registry::default());
    let archive_reader = ArchiveReader::new(archive_reader_config, &metrics)?;
    archive_reader.sync_manifest_once().await?;
    let latest_checkpoint_in_archive = archive_reader.latest_available_checkpoint().await?;
    info!(
        "Latest available checkpoint in archive store: {}",
        latest_checkpoint_in_archive
    );
    info!("Highest synced checkpoint in db: {latest_checkpoint}");
    if latest_checkpoint_in_archive <= latest_checkpoint {
        return Ok(());
    }
    let progress_bar = ProgressBar::new(latest_checkpoint_in_archive).with_style(
        ProgressStyle::with_template("[{elapsed_precise}] {wide_bar} {pos}/{len}({msg})").unwrap(),
    );
    let txn_counter = Arc::new(AtomicU64::new(0));
    let checkpoint_counter = Arc::new(AtomicU64::new(0));
    let cloned_progress_bar = progress_bar.clone();
    let cloned_checkpoint_store = checkpoint_store.clone();
    let cloned_counter = txn_counter.clone();
    let instant = Instant::now();
    tokio::spawn(async move {
        loop {
            let curr_latest_checkpoint = cloned_checkpoint_store
                .get_highest_synced_checkpoint()
                .unwrap()
                .map(|c| c.sequence_number)
                .unwrap_or(0);
            let total_checkpoints_per_sec = (curr_latest_checkpoint - latest_checkpoint) as f64
                / instant.elapsed().as_secs_f64();
            let total_txns_per_sec =
                cloned_counter.load(Ordering::Relaxed) as f64 / instant.elapsed().as_secs_f64();
            cloned_progress_bar.set_position(curr_latest_checkpoint);
            cloned_progress_bar.set_message(format!(
                "checkpoints/s: {}, txns/s: {}",
                total_checkpoints_per_sec, total_txns_per_sec
            ));
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    let start = latest_checkpoint
        .checked_add(1)
        .context("Checkpoint overflow")
        .map_err(|_| anyhow!("Failed to increment checkpoint"))?;
    info!("Starting syncing checkpoints from checkpoint seq num: {start}");
    archive_reader
        .read(
            state_sync_store,
            start..u64::MAX,
            txn_counter,
            checkpoint_counter,
            true,
        )
        .await?;
    let end = checkpoint_store
        .get_highest_synced_checkpoint()?
        .map(|c| c.sequence_number)
        .unwrap_or(0);
    progress_bar.finish_and_clear();
    info!("Highest synced checkpoint after sync: {end}");
    Ok(())
}
