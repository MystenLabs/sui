// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_store_types::{ObjectContentDigest, StoreData, StoreObject};
use crate::checkpoints::{CheckpointStore, CheckpointWatermark};
use crate::rest_index::RestIndexStore;
use anyhow::anyhow;
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use once_cell::sync::Lazy;
use prometheus::{
    register_int_counter_with_registry, register_int_gauge_with_registry, IntCounter, IntGauge,
    Registry,
};
use rocksdb::LiveFile;
use std::cmp::{max, min};
use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{sync::Arc, time::Duration};
use sui_archival::reader::ArchiveReaderBalancer;
use sui_config::node::AuthorityStorePruningConfig;
use sui_storage::mutex_table::RwLockTable;
use sui_types::base_types::SequenceNumber;
use sui_types::committee::EpochId;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointDigest, CheckpointSequenceNumber,
};
use sui_types::{
    base_types::{ObjectID, VersionNumber},
    storage::ObjectKey,
};
use tokio::sync::oneshot::{self, Sender};
use tokio::time::Instant;
use tracing::{debug, error, info, warn};
use typed_store::{Map, TypedStoreError};

use super::authority_store_tables::AuthorityPerpetualTables;

static PERIODIC_PRUNING_TABLES: Lazy<BTreeSet<String>> = Lazy::new(|| {
    [
        "objects",
        "effects",
        "transactions",
        "events",
        "executed_effects",
        "executed_transactions_to_checkpoint",
    ]
    .into_iter()
    .map(|cf| cf.to_string())
    .collect()
});
pub const EPOCH_DURATION_MS_FOR_TESTING: u64 = 24 * 60 * 60 * 1000;
pub struct AuthorityStorePruner {
    _objects_pruner_cancel_handle: oneshot::Sender<()>,
}

static MIN_PRUNING_TICK_DURATION_MS: u64 = 10 * 1000;

pub struct AuthorityStorePruningMetrics {
    pub last_pruned_checkpoint: IntGauge,
    pub num_pruned_objects: IntCounter,
    pub num_pruned_tombstones: IntCounter,
    pub last_pruned_effects_checkpoint: IntGauge,
    pub num_epochs_to_retain_for_objects: IntGauge,
    pub num_epochs_to_retain_for_checkpoints: IntGauge,
}

impl AuthorityStorePruningMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        let this = Self {
            last_pruned_checkpoint: register_int_gauge_with_registry!(
                "last_pruned_checkpoint",
                "Last pruned checkpoint",
                registry
            )
            .unwrap(),
            num_pruned_objects: register_int_counter_with_registry!(
                "num_pruned_objects",
                "Number of pruned objects",
                registry
            )
            .unwrap(),
            num_pruned_tombstones: register_int_counter_with_registry!(
                "num_pruned_tombstones",
                "Number of pruned tombstones",
                registry
            )
            .unwrap(),
            last_pruned_effects_checkpoint: register_int_gauge_with_registry!(
                "last_pruned_effects_checkpoint",
                "Last pruned effects checkpoint",
                registry
            )
            .unwrap(),
            num_epochs_to_retain_for_objects: register_int_gauge_with_registry!(
                "num_epochs_to_retain_for_objects",
                "Number of epochs to retain for objects",
                registry
            )
            .unwrap(),
            num_epochs_to_retain_for_checkpoints: register_int_gauge_with_registry!(
                "num_epochs_to_retain_for_checkpoints",
                "Number of epochs to retain for checkpoints",
                registry
            )
            .unwrap(),
        };
        Arc::new(this)
    }

    pub fn new_for_test() -> Arc<Self> {
        Self::new(&Registry::new())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PruningMode {
    Objects,
    Checkpoints,
}

impl AuthorityStorePruner {
    /// prunes old versions of objects based on transaction effects
    async fn prune_objects(
        transaction_effects: Vec<TransactionEffects>,
        perpetual_db: &Arc<AuthorityPerpetualTables>,
        objects_lock_table: &Arc<RwLockTable<ObjectContentDigest>>,
        checkpoint_number: CheckpointSequenceNumber,
        metrics: Arc<AuthorityStorePruningMetrics>,
        indirect_objects_threshold: usize,
        enable_pruning_tombstones: bool,
    ) -> anyhow::Result<()> {
        let _scope = monitored_scope("ObjectsLivePruner");
        let mut wb = perpetual_db.objects.batch();

        // Collect objects keys that need to be deleted from `transaction_effects`.
        let mut live_object_keys_to_prune = vec![];
        let mut object_tombstones_to_prune = vec![];
        for effects in &transaction_effects {
            for (object_id, seq_number) in effects.modified_at_versions() {
                live_object_keys_to_prune.push(ObjectKey(object_id, seq_number));
            }

            if enable_pruning_tombstones {
                for deleted_object_key in effects.all_tombstones() {
                    object_tombstones_to_prune
                        .push(ObjectKey(deleted_object_key.0, deleted_object_key.1));
                }
            }
        }

        metrics
            .num_pruned_objects
            .inc_by(live_object_keys_to_prune.len() as u64);
        metrics
            .num_pruned_tombstones
            .inc_by(object_tombstones_to_prune.len() as u64);

        let mut indirect_objects: HashMap<_, i64> = HashMap::new();
        if indirect_objects_threshold > 0 && indirect_objects_threshold < usize::MAX {
            for object in perpetual_db
                .objects
                .multi_get(live_object_keys_to_prune.iter())?
                .into_iter()
                .flatten()
            {
                if let StoreObject::Value(obj) = object.into_inner() {
                    if let StoreData::IndirectObject(indirect_object) = obj.data {
                        *indirect_objects.entry(indirect_object.digest).or_default() -= 1;
                    }
                }
            }
        }

        let mut updates: HashMap<ObjectID, (VersionNumber, VersionNumber)> = HashMap::new();
        for ObjectKey(object_id, seq_number) in live_object_keys_to_prune {
            updates
                .entry(object_id)
                .and_modify(|range| *range = (min(range.0, seq_number), max(range.1, seq_number)))
                .or_insert((seq_number, seq_number));
        }

        for (object_id, (min_version, max_version)) in updates {
            debug!(
                "Pruning object {:?} versions {:?} - {:?}",
                object_id, min_version, max_version
            );
            let start_range = ObjectKey(object_id, min_version);
            let end_range = ObjectKey(object_id, (max_version.value() + 1).into());
            wb.schedule_delete_range(&perpetual_db.objects, &start_range, &end_range)?;
        }

        // When enable_pruning_tombstones is enabled, instead of using range deletes, we need to do a scan of all the keys
        // for the deleted objects and then do point deletes to delete all the existing keys. This is because to improve read
        // performance, we set `ignore_range_deletions` on all read options, and using range delete to delete tombstones
        // may leak object (imagine a tombstone is compacted away, but earlier version is still not). Using point deletes
        // guarantees that all earlier versions are deleted in the database.
        if !object_tombstones_to_prune.is_empty() {
            let mut object_keys_to_delete = vec![];
            for ObjectKey(object_id, seq_number) in object_tombstones_to_prune {
                for result in perpetual_db.objects.safe_iter_with_bounds(
                    Some(ObjectKey(object_id, VersionNumber::MIN)),
                    Some(ObjectKey(object_id, seq_number.next())),
                ) {
                    let (object_key, _) = result?;
                    assert_eq!(object_key.0, object_id);
                    object_keys_to_delete.push(object_key);
                }
            }

            wb.delete_batch(&perpetual_db.objects, object_keys_to_delete)?;
        }

        if !indirect_objects.is_empty() {
            let ref_count_update = indirect_objects
                .iter()
                .map(|(digest, delta)| (digest, delta.to_le_bytes()));
            wb.partial_merge_batch(&perpetual_db.indirect_move_objects, ref_count_update)?;
        }
        perpetual_db.set_highest_pruned_checkpoint(&mut wb, checkpoint_number)?;
        metrics.last_pruned_checkpoint.set(checkpoint_number as i64);

        let _locks = objects_lock_table
            .acquire_locks(indirect_objects.into_keys())
            .await;
        wb.write()?;
        Ok(())
    }

    fn prune_checkpoints(
        perpetual_db: &Arc<AuthorityPerpetualTables>,
        checkpoint_db: &Arc<CheckpointStore>,
        rest_index: Option<&RestIndexStore>,
        checkpoint_number: CheckpointSequenceNumber,
        checkpoints_to_prune: Vec<CheckpointDigest>,
        checkpoint_content_to_prune: Vec<CheckpointContents>,
        effects_to_prune: &Vec<TransactionEffects>,
        metrics: Arc<AuthorityStorePruningMetrics>,
    ) -> anyhow::Result<()> {
        let _scope = monitored_scope("EffectsLivePruner");

        let mut perpetual_batch = perpetual_db.objects.batch();
        let transactions: Vec<_> = checkpoint_content_to_prune
            .iter()
            .flat_map(|content| content.iter().map(|tx| tx.transaction))
            .collect();

        perpetual_batch.delete_batch(&perpetual_db.transactions, transactions.iter())?;
        perpetual_batch.delete_batch(&perpetual_db.executed_effects, transactions.iter())?;
        perpetual_batch.delete_batch(
            &perpetual_db.executed_transactions_to_checkpoint,
            transactions,
        )?;

        let mut effect_digests = vec![];
        for effects in effects_to_prune {
            let effects_digest = effects.digest();
            debug!("Pruning effects {:?}", effects_digest);
            effect_digests.push(effects_digest);

            if let Some(event_digest) = effects.events_digest() {
                if let Some(next_digest) = event_digest.next_lexicographical() {
                    perpetual_batch.schedule_delete_range(
                        &perpetual_db.events,
                        &(*event_digest, 0),
                        &(next_digest, 0),
                    )?;
                }
            }
        }
        perpetual_batch.delete_batch(&perpetual_db.effects, effect_digests)?;

        let mut checkpoints_batch = checkpoint_db.certified_checkpoints.batch();

        let checkpoint_content_digests =
            checkpoint_content_to_prune.iter().map(|ckpt| ckpt.digest());
        checkpoints_batch.delete_batch(
            &checkpoint_db.checkpoint_content,
            checkpoint_content_digests.clone(),
        )?;
        checkpoints_batch.delete_batch(
            &checkpoint_db.checkpoint_sequence_by_contents_digest,
            checkpoint_content_digests,
        )?;

        checkpoints_batch
            .delete_batch(&checkpoint_db.checkpoint_by_digest, checkpoints_to_prune)?;

        checkpoints_batch.insert_batch(
            &checkpoint_db.watermarks,
            [(
                &CheckpointWatermark::HighestPruned,
                &(checkpoint_number, CheckpointDigest::random()),
            )],
        )?;

        if let Some(rest_index) = rest_index {
            rest_index.prune(&checkpoint_content_to_prune)?;
        }
        perpetual_batch.write()?;
        checkpoints_batch.write()?;
        metrics
            .last_pruned_effects_checkpoint
            .set(checkpoint_number as i64);
        Ok(())
    }

    /// Prunes old data based on effects from all checkpoints from epochs eligible for pruning
    pub async fn prune_objects_for_eligible_epochs(
        perpetual_db: &Arc<AuthorityPerpetualTables>,
        checkpoint_store: &Arc<CheckpointStore>,
        rest_index: Option<&RestIndexStore>,
        objects_lock_table: &Arc<RwLockTable<ObjectContentDigest>>,
        config: AuthorityStorePruningConfig,
        metrics: Arc<AuthorityStorePruningMetrics>,
        indirect_objects_threshold: usize,
        epoch_duration_ms: u64,
    ) -> anyhow::Result<()> {
        let _scope = monitored_scope("PruneObjectsForEligibleEpochs");
        let (mut max_eligible_checkpoint_number, epoch_id) = checkpoint_store
            .get_highest_executed_checkpoint()?
            .map(|c| (*c.sequence_number(), c.epoch))
            .unwrap_or_default();
        let pruned_checkpoint_number = perpetual_db.get_highest_pruned_checkpoint()?;
        if config.smooth && config.num_epochs_to_retain > 0 {
            max_eligible_checkpoint_number = Self::smoothed_max_eligible_checkpoint_number(
                checkpoint_store,
                max_eligible_checkpoint_number,
                pruned_checkpoint_number,
                epoch_id,
                epoch_duration_ms,
                config.num_epochs_to_retain,
            )?;
        }
        Self::prune_for_eligible_epochs(
            perpetual_db,
            checkpoint_store,
            rest_index,
            PruningMode::Objects,
            config.num_epochs_to_retain,
            pruned_checkpoint_number,
            max_eligible_checkpoint_number,
            objects_lock_table,
            config,
            metrics.clone(),
            indirect_objects_threshold,
        )
        .await
    }

    pub async fn prune_checkpoints_for_eligible_epochs(
        perpetual_db: &Arc<AuthorityPerpetualTables>,
        checkpoint_store: &Arc<CheckpointStore>,
        rest_index: Option<&RestIndexStore>,
        objects_lock_table: &Arc<RwLockTable<ObjectContentDigest>>,
        config: AuthorityStorePruningConfig,
        metrics: Arc<AuthorityStorePruningMetrics>,
        indirect_objects_threshold: usize,
        archive_readers: ArchiveReaderBalancer,
        epoch_duration_ms: u64,
    ) -> anyhow::Result<()> {
        let _scope = monitored_scope("PruneCheckpointsForEligibleEpochs");
        let pruned_checkpoint_number =
            checkpoint_store.get_highest_pruned_checkpoint_seq_number()?;
        let (last_executed_checkpoint, epoch_id) = checkpoint_store
            .get_highest_executed_checkpoint()?
            .map(|c| (*c.sequence_number(), c.epoch))
            .unwrap_or_default();
        let latest_archived_checkpoint = archive_readers
            .get_archive_watermark()
            .await?
            .unwrap_or(u64::MAX);
        let mut max_eligible_checkpoint = min(latest_archived_checkpoint, last_executed_checkpoint);
        if config.num_epochs_to_retain != u64::MAX {
            max_eligible_checkpoint = min(
                max_eligible_checkpoint,
                perpetual_db.get_highest_pruned_checkpoint()?,
            );
        }
        if config.smooth {
            if let Some(num_epochs_to_retain) = config.num_epochs_to_retain_for_checkpoints {
                max_eligible_checkpoint = Self::smoothed_max_eligible_checkpoint_number(
                    checkpoint_store,
                    max_eligible_checkpoint,
                    pruned_checkpoint_number,
                    epoch_id,
                    epoch_duration_ms,
                    num_epochs_to_retain,
                )?;
            }
        }
        debug!("Max eligible checkpoint {}", max_eligible_checkpoint);
        Self::prune_for_eligible_epochs(
            perpetual_db,
            checkpoint_store,
            rest_index,
            PruningMode::Checkpoints,
            config
                .num_epochs_to_retain_for_checkpoints()
                .ok_or_else(|| anyhow!("config value not set"))?,
            pruned_checkpoint_number,
            max_eligible_checkpoint,
            objects_lock_table,
            config,
            metrics.clone(),
            indirect_objects_threshold,
        )
        .await
    }

    /// Prunes old object versions based on effects from all checkpoints from epochs eligible for pruning
    pub async fn prune_for_eligible_epochs(
        perpetual_db: &Arc<AuthorityPerpetualTables>,
        checkpoint_store: &Arc<CheckpointStore>,
        rest_index: Option<&RestIndexStore>,
        mode: PruningMode,
        num_epochs_to_retain: u64,
        starting_checkpoint_number: CheckpointSequenceNumber,
        max_eligible_checkpoint: CheckpointSequenceNumber,
        objects_lock_table: &Arc<RwLockTable<ObjectContentDigest>>,
        config: AuthorityStorePruningConfig,
        metrics: Arc<AuthorityStorePruningMetrics>,
        indirect_objects_threshold: usize,
    ) -> anyhow::Result<()> {
        let _scope = monitored_scope("PruneForEligibleEpochs");

        let mut checkpoint_number = starting_checkpoint_number;
        let current_epoch = checkpoint_store
            .get_highest_executed_checkpoint()?
            .map(|c| c.epoch())
            .unwrap_or_default();

        let mut checkpoints_to_prune = vec![];
        let mut checkpoint_content_to_prune = vec![];
        let mut effects_to_prune = vec![];

        loop {
            let Some(ckpt) = checkpoint_store
                .certified_checkpoints
                .get(&(checkpoint_number + 1))?
            else {
                break;
            };
            let checkpoint = ckpt.into_inner();
            // Skipping because  checkpoint's epoch or checkpoint number is too new.
            // We have to respect the highest executed checkpoint watermark (including the watermark itself)
            // because there might be parts of the system that still require access to old object versions
            // (i.e. state accumulator).
            if (current_epoch < checkpoint.epoch() + num_epochs_to_retain)
                || (*checkpoint.sequence_number() >= max_eligible_checkpoint)
            {
                break;
            }
            checkpoint_number = *checkpoint.sequence_number();

            let content = checkpoint_store
                .get_checkpoint_contents(&checkpoint.content_digest)?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "checkpoint content data is missing: {}",
                        checkpoint.sequence_number
                    )
                })?;
            let effects = perpetual_db
                .effects
                .multi_get(content.iter().map(|tx| tx.effects))?;

            info!("scheduling pruning for checkpoint {:?}", checkpoint_number);
            checkpoints_to_prune.push(*checkpoint.digest());
            checkpoint_content_to_prune.push(content);
            effects_to_prune.extend(effects.into_iter().flatten());

            if effects_to_prune.len() >= config.max_transactions_in_batch
                || checkpoints_to_prune.len() >= config.max_checkpoints_in_batch
            {
                match mode {
                    PruningMode::Objects => {
                        Self::prune_objects(
                            effects_to_prune,
                            perpetual_db,
                            objects_lock_table,
                            checkpoint_number,
                            metrics.clone(),
                            indirect_objects_threshold,
                            !config.killswitch_tombstone_pruning,
                        )
                        .await?
                    }
                    PruningMode::Checkpoints => Self::prune_checkpoints(
                        perpetual_db,
                        checkpoint_store,
                        rest_index,
                        checkpoint_number,
                        checkpoints_to_prune,
                        checkpoint_content_to_prune,
                        &effects_to_prune,
                        metrics.clone(),
                    )?,
                };
                checkpoints_to_prune = vec![];
                checkpoint_content_to_prune = vec![];
                effects_to_prune = vec![];
                // yield back to the tokio runtime. Prevent potential halt of other tasks
                tokio::task::yield_now().await;
            }
        }

        if !checkpoints_to_prune.is_empty() {
            match mode {
                PruningMode::Objects => {
                    Self::prune_objects(
                        effects_to_prune,
                        perpetual_db,
                        objects_lock_table,
                        checkpoint_number,
                        metrics.clone(),
                        indirect_objects_threshold,
                        !config.killswitch_tombstone_pruning,
                    )
                    .await?
                }
                PruningMode::Checkpoints => Self::prune_checkpoints(
                    perpetual_db,
                    checkpoint_store,
                    rest_index,
                    checkpoint_number,
                    checkpoints_to_prune,
                    checkpoint_content_to_prune,
                    &effects_to_prune,
                    metrics.clone(),
                )?,
            };
        }
        Ok(())
    }

    fn compact_next_sst_file(
        perpetual_db: Arc<AuthorityPerpetualTables>,
        delay_days: usize,
        last_processed: Arc<Mutex<HashMap<String, SystemTime>>>,
    ) -> anyhow::Result<Option<LiveFile>> {
        let db_path = perpetual_db.objects.rocksdb.path();
        let mut state = last_processed
            .lock()
            .expect("failed to obtain a lock for last processed SST files");
        let mut sst_file_for_compaction: Option<LiveFile> = None;
        let time_threshold =
            SystemTime::now() - Duration::from_secs(delay_days as u64 * 24 * 60 * 60);
        for sst_file in perpetual_db.objects.rocksdb.live_files()? {
            let file_path = db_path.join(sst_file.name.clone().trim_matches('/'));
            let last_modified = std::fs::metadata(file_path)?.modified()?;
            if !PERIODIC_PRUNING_TABLES.contains(&sst_file.column_family_name)
                || sst_file.level < 1
                || sst_file.start_key.is_none()
                || sst_file.end_key.is_none()
                || last_modified > time_threshold
                || state.get(&sst_file.name).unwrap_or(&UNIX_EPOCH) > &time_threshold
            {
                continue;
            }
            if let Some(candidate) = &sst_file_for_compaction {
                if candidate.size > sst_file.size {
                    continue;
                }
            }
            sst_file_for_compaction = Some(sst_file);
        }
        let Some(sst_file) = sst_file_for_compaction else {
            return Ok(None);
        };
        info!(
            "Manual compaction of sst file {:?}. Size: {:?}, level: {:?}",
            sst_file.name, sst_file.size, sst_file.level
        );
        perpetual_db.objects.compact_range_raw(
            &sst_file.column_family_name,
            sst_file.start_key.clone().unwrap(),
            sst_file.end_key.clone().unwrap(),
        )?;
        state.insert(sst_file.name.clone(), SystemTime::now());
        Ok(Some(sst_file))
    }

    fn pruning_tick_duration_ms(epoch_duration_ms: u64) -> u64 {
        min(epoch_duration_ms / 2, MIN_PRUNING_TICK_DURATION_MS)
    }

    fn smoothed_max_eligible_checkpoint_number(
        checkpoint_store: &Arc<CheckpointStore>,
        mut max_eligible_checkpoint: CheckpointSequenceNumber,
        pruned_checkpoint: CheckpointSequenceNumber,
        epoch_id: EpochId,
        epoch_duration_ms: u64,
        num_epochs_to_retain: u64,
    ) -> anyhow::Result<CheckpointSequenceNumber> {
        if epoch_id < num_epochs_to_retain {
            return Ok(0);
        }
        let last_checkpoint_in_epoch = checkpoint_store
            .get_epoch_last_checkpoint(epoch_id - num_epochs_to_retain)?
            .map(|checkpoint| checkpoint.sequence_number)
            .unwrap_or_default();
        max_eligible_checkpoint = max_eligible_checkpoint.min(last_checkpoint_in_epoch);
        if max_eligible_checkpoint == 0 {
            return Ok(max_eligible_checkpoint);
        }
        let num_intervals = epoch_duration_ms
            .checked_div(Self::pruning_tick_duration_ms(epoch_duration_ms))
            .unwrap_or(1);
        let delta = max_eligible_checkpoint
            .checked_sub(pruned_checkpoint)
            .unwrap_or_default()
            .checked_div(num_intervals)
            .unwrap_or(1);
        Ok(pruned_checkpoint + delta)
    }

    fn setup_pruning(
        config: AuthorityStorePruningConfig,
        epoch_duration_ms: u64,
        perpetual_db: Arc<AuthorityPerpetualTables>,
        checkpoint_store: Arc<CheckpointStore>,
        rest_index: Option<Arc<RestIndexStore>>,
        objects_lock_table: Arc<RwLockTable<ObjectContentDigest>>,
        metrics: Arc<AuthorityStorePruningMetrics>,
        indirect_objects_threshold: usize,
        archive_readers: ArchiveReaderBalancer,
    ) -> Sender<()> {
        let (sender, mut recv) = tokio::sync::oneshot::channel();
        debug!(
            "Starting object pruning service with num_epochs_to_retain={}",
            config.num_epochs_to_retain
        );

        let tick_duration =
            Duration::from_millis(Self::pruning_tick_duration_ms(epoch_duration_ms));
        let pruning_initial_delay = if cfg!(msim) {
            Duration::from_millis(1)
        } else {
            Duration::from_secs(config.pruning_run_delay_seconds.unwrap_or(60 * 60))
        };
        let mut objects_prune_interval =
            tokio::time::interval_at(Instant::now() + pruning_initial_delay, tick_duration);
        let mut checkpoints_prune_interval =
            tokio::time::interval_at(Instant::now() + pruning_initial_delay, tick_duration);

        let perpetual_db_for_compaction = perpetual_db.clone();
        if let Some(delay_days) = config.periodic_compaction_threshold_days {
            spawn_monitored_task!(async move {
                let last_processed = Arc::new(Mutex::new(HashMap::new()));
                loop {
                    let db = perpetual_db_for_compaction.clone();
                    let state = Arc::clone(&last_processed);
                    let result = tokio::task::spawn_blocking(move || {
                        Self::compact_next_sst_file(db, delay_days, state)
                    })
                    .await;
                    let mut sleep_interval_secs = 1;
                    match result {
                        Err(err) => error!("Failed to compact sst file: {:?}", err),
                        Ok(Err(err)) => error!("Failed to compact sst file: {:?}", err),
                        Ok(Ok(None)) => {
                            sleep_interval_secs = 3600;
                        }
                        _ => {}
                    }
                    tokio::time::sleep(Duration::from_secs(sleep_interval_secs)).await;
                }
            });
        }

        metrics
            .num_epochs_to_retain_for_objects
            .set(config.num_epochs_to_retain as i64);
        metrics.num_epochs_to_retain_for_checkpoints.set(
            config
                .num_epochs_to_retain_for_checkpoints
                .unwrap_or_default() as i64,
        );

        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    _ = objects_prune_interval.tick(), if config.num_epochs_to_retain != u64::MAX => {
                        if let Err(err) = Self::prune_objects_for_eligible_epochs(&perpetual_db, &checkpoint_store, rest_index.as_deref(), &objects_lock_table, config.clone(), metrics.clone(), indirect_objects_threshold, epoch_duration_ms).await {
                            error!("Failed to prune objects: {:?}", err);
                        }
                    },
                    _ = checkpoints_prune_interval.tick(), if !matches!(config.num_epochs_to_retain_for_checkpoints(), None | Some(u64::MAX) | Some(0)) => {
                        if let Err(err) = Self::prune_checkpoints_for_eligible_epochs(&perpetual_db, &checkpoint_store, rest_index.as_deref(), &objects_lock_table, config.clone(), metrics.clone(), indirect_objects_threshold, archive_readers.clone(), epoch_duration_ms).await {
                            error!("Failed to prune checkpoints: {:?}", err);
                        }
                    },
                    _ = &mut recv => break,
                }
            }
        });
        sender
    }

    pub fn new(
        perpetual_db: Arc<AuthorityPerpetualTables>,
        checkpoint_store: Arc<CheckpointStore>,
        rest_index: Option<Arc<RestIndexStore>>,
        objects_lock_table: Arc<RwLockTable<ObjectContentDigest>>,
        mut pruning_config: AuthorityStorePruningConfig,
        is_validator: bool,
        epoch_duration_ms: u64,
        registry: &Registry,
        indirect_objects_threshold: usize,
        archive_readers: ArchiveReaderBalancer,
    ) -> Self {
        if pruning_config.num_epochs_to_retain > 0 && pruning_config.num_epochs_to_retain < u64::MAX
        {
            warn!("Using objects pruner with num_epochs_to_retain = {} can lead to performance issues", pruning_config.num_epochs_to_retain);
            if is_validator {
                warn!("Resetting to aggressive pruner.");
                pruning_config.num_epochs_to_retain = 0;
            } else {
                warn!("Consider using an aggressive pruner (num_epochs_to_retain = 0)");
            }
        }
        AuthorityStorePruner {
            _objects_pruner_cancel_handle: Self::setup_pruning(
                pruning_config,
                epoch_duration_ms,
                perpetual_db,
                checkpoint_store,
                rest_index,
                objects_lock_table,
                AuthorityStorePruningMetrics::new(registry),
                indirect_objects_threshold,
                archive_readers,
            ),
        }
    }

    pub fn compact(perpetual_db: &Arc<AuthorityPerpetualTables>) -> Result<(), TypedStoreError> {
        perpetual_db.objects.compact_range(
            &ObjectKey(ObjectID::ZERO, SequenceNumber::MIN),
            &ObjectKey(ObjectID::MAX, SequenceNumber::MAX),
        )
    }
}

#[cfg(test)]
mod tests {
    use more_asserts as ma;
    use std::path::Path;
    use std::time::Duration;
    use std::{collections::HashSet, sync::Arc};
    use tracing::log::info;

    use crate::authority::authority_store_pruner::AuthorityStorePruningMetrics;
    use crate::authority::authority_store_tables::AuthorityPerpetualTables;
    use crate::authority::authority_store_types::{
        get_store_object_pair, ObjectContentDigest, StoreData, StoreObject, StoreObjectPair,
        StoreObjectWrapper,
    };
    use prometheus::Registry;
    use sui_storage::mutex_table::RwLockTable;
    use sui_types::base_types::ObjectDigest;
    use sui_types::effects::TransactionEffects;
    use sui_types::effects::TransactionEffectsAPI;
    use sui_types::{
        base_types::{ObjectID, SequenceNumber},
        object::Object,
        storage::ObjectKey,
    };
    use typed_store::rocks::util::reference_count_merge_operator;
    use typed_store::rocks::{DBMap, MetricConf, ReadWriteOptions};
    use typed_store::Map;

    use super::AuthorityStorePruner;

    fn get_keys_after_pruning(path: &Path) -> anyhow::Result<HashSet<ObjectKey>> {
        let perpetual_db_path = path.join(Path::new("perpetual"));
        let cf_names = AuthorityPerpetualTables::describe_tables();
        let cfs: Vec<&str> = cf_names.keys().map(|x| x.as_str()).collect();
        let mut db_options = rocksdb::Options::default();
        db_options.set_merge_operator(
            "refcount operator",
            reference_count_merge_operator,
            reference_count_merge_operator,
        );
        let perpetual_db = typed_store::rocks::open_cf(
            perpetual_db_path,
            Some(db_options),
            MetricConf::new("perpetual_pruning"),
            &cfs,
        );

        let mut after_pruning = HashSet::new();
        let objects = DBMap::<ObjectKey, StoreObjectWrapper>::reopen(
            &perpetual_db?,
            Some("objects"),
            // open the db to bypass default db options which ignores range tombstones
            // so we can read the accurate number of retained versions
            &ReadWriteOptions::default(),
            false,
        )?;
        let iter = objects.unbounded_iter();
        for (k, _v) in iter {
            after_pruning.insert(k);
        }
        Ok(after_pruning)
    }

    type GenerateTestDataResult = (Vec<ObjectKey>, Vec<ObjectKey>, Vec<ObjectKey>);

    fn generate_test_data(
        db: Arc<AuthorityPerpetualTables>,
        num_versions_per_object: u64,
        num_object_versions_to_retain: u64,
        total_unique_object_ids: u32,
        indirect_object_threshold: usize,
    ) -> Result<GenerateTestDataResult, anyhow::Error> {
        assert!(num_versions_per_object >= num_object_versions_to_retain);

        let (mut to_keep, mut to_delete, mut tombstones) = (vec![], vec![], vec![]);
        let mut batch = db.objects.batch();

        let ids = ObjectID::in_range(ObjectID::ZERO, total_unique_object_ids.into())?;
        for id in ids {
            for (counter, seq) in (0..num_versions_per_object).rev().enumerate() {
                let object_key = ObjectKey(id, SequenceNumber::from_u64(seq));
                if counter < num_object_versions_to_retain.try_into().unwrap() {
                    // latest `num_object_versions_to_retain` should not have been pruned
                    to_keep.push(object_key);
                } else {
                    to_delete.push(object_key);
                }
                let StoreObjectPair(obj, indirect_obj) = get_store_object_pair(
                    Object::immutable_with_id_for_testing(id),
                    indirect_object_threshold,
                );
                batch.insert_batch(
                    &db.objects,
                    [(ObjectKey(id, SequenceNumber::from(seq)), obj.clone())],
                )?;
                if let StoreObject::Value(o) = obj.into_inner() {
                    if let StoreData::IndirectObject(metadata) = o.data {
                        batch.merge_batch(
                            &db.indirect_move_objects,
                            [(metadata.digest, indirect_obj.unwrap())],
                        )?;
                    }
                }
            }

            // Adding a tombstone for deleted object.
            if num_object_versions_to_retain == 0 {
                let tombstone_key = ObjectKey(id, SequenceNumber::from(num_versions_per_object));
                println!("Adding tombstone object {:?}", tombstone_key);
                batch.insert_batch(
                    &db.objects,
                    [(tombstone_key, StoreObjectWrapper::V1(StoreObject::Deleted))],
                )?;
                tombstones.push(tombstone_key);
            }
        }
        batch.write().unwrap();
        assert_eq!(
            to_keep.len() as u64,
            std::cmp::min(num_object_versions_to_retain, num_versions_per_object)
                * total_unique_object_ids as u64
        );
        assert_eq!(
            tombstones.len() as u64,
            if num_object_versions_to_retain == 0 {
                total_unique_object_ids as u64
            } else {
                0
            }
        );
        Ok((to_keep, to_delete, tombstones))
    }

    pub(crate) fn lock_table() -> Arc<RwLockTable<ObjectContentDigest>> {
        Arc::new(RwLockTable::new(1))
    }

    async fn run_pruner(
        path: &Path,
        num_versions_per_object: u64,
        num_object_versions_to_retain: u64,
        total_unique_object_ids: u32,
        indirect_object_threshold: usize,
    ) -> Vec<ObjectKey> {
        let registry = Registry::default();
        let metrics = AuthorityStorePruningMetrics::new(&registry);
        let to_keep = {
            let db = Arc::new(AuthorityPerpetualTables::open(path, None));
            let (to_keep, to_delete, tombstones) = generate_test_data(
                db.clone(),
                num_versions_per_object,
                num_object_versions_to_retain,
                total_unique_object_ids,
                indirect_object_threshold,
            )
            .unwrap();
            let mut effects = TransactionEffects::default();
            for object in to_delete {
                effects.unsafe_add_deleted_live_object_for_testing((
                    object.0,
                    object.1,
                    ObjectDigest::MIN,
                ));
            }
            for object in tombstones {
                effects.unsafe_add_object_tombstone_for_testing((
                    object.0,
                    object.1,
                    ObjectDigest::MIN,
                ));
            }
            AuthorityStorePruner::prune_objects(
                vec![effects],
                &db,
                &lock_table(),
                0,
                metrics,
                indirect_object_threshold,
                true,
            )
            .await
            .unwrap();
            to_keep
        };
        tokio::time::sleep(Duration::from_secs(3)).await;
        to_keep
    }

    // Tests pruning old version of live objects.
    #[tokio::test]
    async fn test_pruning_objects() {
        let path = tempfile::tempdir().unwrap().into_path();
        let to_keep = run_pruner(&path, 3, 2, 1000, 0).await;
        assert_eq!(
            HashSet::from_iter(to_keep),
            get_keys_after_pruning(&path).unwrap()
        );
        run_pruner(&tempfile::tempdir().unwrap().into_path(), 3, 2, 1000, 0).await;
    }

    // Tests pruning deleted objects (object tombstones).
    #[tokio::test]
    async fn test_pruning_tombstones() {
        let path = tempfile::tempdir().unwrap().into_path();
        let to_keep = run_pruner(&path, 0, 0, 1000, 0).await;
        assert_eq!(to_keep.len(), 0);
        assert_eq!(get_keys_after_pruning(&path).unwrap().len(), 0);

        let path = tempfile::tempdir().unwrap().into_path();
        let to_keep = run_pruner(&path, 3, 0, 1000, 0).await;
        assert_eq!(to_keep.len(), 0);
        assert_eq!(get_keys_after_pruning(&path).unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_ref_count_pruning() {
        let path = tempfile::tempdir().unwrap().into_path();
        run_pruner(&path, 3, 2, 1000, 1).await;
        {
            let perpetual_db = AuthorityPerpetualTables::open(&path, None);
            let count = perpetual_db.indirect_move_objects.keys().count();
            // references are not reset, expected to have 1000 unique objects
            assert_eq!(count, 1000);
        }

        let path = tempfile::tempdir().unwrap().into_path();
        run_pruner(&path, 3, 0, 1000, 1).await;
        {
            let perpetual_db = AuthorityPerpetualTables::open(&path, None);
            perpetual_db.indirect_move_objects.flush().unwrap();
            perpetual_db
                .indirect_move_objects
                .compact_range(&ObjectDigest::MIN, &ObjectDigest::MAX)
                .unwrap();
            perpetual_db
                .indirect_move_objects
                .compact_range(&ObjectDigest::MIN, &ObjectDigest::MAX)
                .unwrap();
            let count = perpetual_db.indirect_move_objects.keys().count();
            assert_eq!(count, 0);
        }
    }

    #[cfg(not(target_env = "msvc"))]
    #[tokio::test]
    async fn test_db_size_after_compaction() -> Result<(), anyhow::Error> {
        let primary_path = tempfile::tempdir()?.into_path();
        let perpetual_db = Arc::new(AuthorityPerpetualTables::open(&primary_path, None));
        let total_unique_object_ids = 10_000;
        let num_versions_per_object = 10;
        let ids = ObjectID::in_range(ObjectID::ZERO, total_unique_object_ids)?;
        let mut to_delete = vec![];
        for id in ids {
            for i in (0..num_versions_per_object).rev() {
                if i < num_versions_per_object - 2 {
                    to_delete.push((id, SequenceNumber::from(i)));
                }
                let obj = get_store_object_pair(Object::immutable_with_id_for_testing(id), 0).0;
                perpetual_db
                    .objects
                    .insert(&ObjectKey(id, SequenceNumber::from(i)), &obj)?;
            }
        }

        fn get_sst_size(path: &Path) -> u64 {
            let mut size = 0;
            for entry in std::fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext != "sst" {
                        continue;
                    }
                    size += std::fs::metadata(path).unwrap().len();
                }
            }
            size
        }

        let db_path = primary_path.clone().join("perpetual");
        let start = ObjectKey(ObjectID::ZERO, SequenceNumber::MIN);
        let end = ObjectKey(ObjectID::MAX, SequenceNumber::MAX);

        perpetual_db.objects.rocksdb.flush()?;
        perpetual_db.objects.compact_range_to_bottom(&start, &end)?;
        let before_compaction_size = get_sst_size(&db_path);

        let mut effects = TransactionEffects::default();
        for object in to_delete {
            effects.unsafe_add_deleted_live_object_for_testing((
                object.0,
                object.1,
                ObjectDigest::MIN,
            ));
        }
        let registry = Registry::default();
        let metrics = AuthorityStorePruningMetrics::new(&registry);
        let total_pruned = AuthorityStorePruner::prune_objects(
            vec![effects],
            &perpetual_db,
            &lock_table(),
            0,
            metrics,
            0,
            true,
        )
        .await;
        info!("Total pruned keys = {:?}", total_pruned);

        perpetual_db.objects.rocksdb.flush()?;
        perpetual_db.objects.compact_range_to_bottom(&start, &end)?;
        let after_compaction_size = get_sst_size(&db_path);

        info!(
            "Before compaction disk size = {:?}, after compaction disk size = {:?}",
            before_compaction_size, after_compaction_size
        );
        ma::assert_le!(after_compaction_size, before_compaction_size);
        Ok(())
    }
}

#[cfg(test)]
#[cfg(not(target_os = "macos"))]
#[cfg(not(target_env = "msvc"))]
mod pprof_tests {
    use crate::authority::authority_store_pruner::tests;

    use std::sync::Arc;
    use tracing::log::{error, info};

    use crate::authority::authority_store_pruner::tests::lock_table;
    use crate::authority::authority_store_pruner::AuthorityStorePruningMetrics;
    use crate::authority::authority_store_tables::AuthorityPerpetualTables;
    use crate::authority::authority_store_types::{get_store_object_pair, StoreObjectWrapper};
    use pprof::Symbol;
    use prometheus::Registry;
    use sui_types::base_types::ObjectDigest;
    use sui_types::base_types::VersionNumber;
    use sui_types::effects::TransactionEffects;
    use sui_types::effects::TransactionEffectsAPI;
    use sui_types::{
        base_types::{ObjectID, SequenceNumber},
        object::Object,
        storage::ObjectKey,
    };
    use typed_store::rocks::DBMap;
    use typed_store::Map;

    use super::AuthorityStorePruner;

    fn insert_keys(
        objects: &DBMap<ObjectKey, StoreObjectWrapper>,
    ) -> Result<TransactionEffects, anyhow::Error> {
        let mut to_delete = vec![];
        let num_versions_to_keep = 2;
        let total_unique_object_ids = 100_000;
        let num_versions_per_object = 10;
        let ids = ObjectID::in_range(ObjectID::ZERO, total_unique_object_ids)?;
        for id in ids {
            for i in (0..num_versions_per_object).rev() {
                let obj = get_store_object_pair(Object::immutable_with_id_for_testing(id), 0).0;
                objects.insert(&ObjectKey(id, SequenceNumber::from(i)), &obj)?;
                if i < num_versions_per_object - num_versions_to_keep {
                    to_delete.push((id, SequenceNumber::from(i)));
                }
                objects.insert(&ObjectKey(id, SequenceNumber::from(i)), &obj)?;
            }
        }

        let mut effects = TransactionEffects::default();
        for object in to_delete {
            effects.unsafe_add_deleted_live_object_for_testing((
                object.0,
                object.1,
                ObjectDigest::MIN,
            ));
        }
        Ok(effects)
    }

    fn read_keys(
        objects: &DBMap<ObjectKey, StoreObjectWrapper>,
        num_reads: u32,
    ) -> Result<(), anyhow::Error> {
        let mut i = 0;
        while i < num_reads {
            let _res = objects.get(&ObjectKey(ObjectID::random(), VersionNumber::MAX))?;
            i += 1;
        }
        Ok(())
    }

    fn is_rocksdb_range_tombstone_frame(vs: &[Symbol]) -> bool {
        for symbol in vs.iter() {
            if symbol
                .name()
                .contains("rocksdb::FragmentedRangeTombstoneList")
            {
                return true;
            }
        }
        false
    }

    #[tokio::test]
    async fn ensure_no_tombstone_fragmentation_in_stack_frame_with_ignore_tombstones(
    ) -> Result<(), anyhow::Error> {
        // This test writes a bunch of objects to objects table, invokes pruning on it and
        // then does a bunch of get(). We open the db with `ignore_range_delete` set to true (default mode).
        // We then record a cpu profile of the `get()` calls and do not find any range fragmentation stack frame
        // in it.
        let registry = Registry::default();
        let metrics = AuthorityStorePruningMetrics::new(&registry);
        let primary_path = tempfile::tempdir()?.into_path();
        let perpetual_db = Arc::new(AuthorityPerpetualTables::open(&primary_path, None));
        let effects = insert_keys(&perpetual_db.objects)?;
        AuthorityStorePruner::prune_objects(
            vec![effects],
            &perpetual_db,
            &tests::lock_table(),
            0,
            metrics,
            1,
            true,
        )
        .await?;
        let guard = pprof::ProfilerGuardBuilder::default()
            .frequency(1000)
            .build()
            .unwrap();
        read_keys(&perpetual_db.objects, 1000)?;
        if let Ok(report) = guard.report().build() {
            assert!(!report.data.keys().any(|f| f
                .frames
                .iter()
                .any(|vs| is_rocksdb_range_tombstone_frame(vs))));
        }
        Ok(())
    }

    #[tokio::test]
    async fn ensure_no_tombstone_fragmentation_in_stack_frame_after_flush(
    ) -> Result<(), anyhow::Error> {
        // This test writes a bunch of objects to objects table, invokes pruning on it and
        // then does a bunch of get(). We open the db with `ignore_range_delete` set to true (default mode).
        // We then record a cpu profile of the `get()` calls and do not find any range fragmentation stack frame
        // in it.
        let primary_path = tempfile::tempdir()?.into_path();
        let perpetual_db = Arc::new(AuthorityPerpetualTables::open(&primary_path, None));
        let effects = insert_keys(&perpetual_db.objects)?;
        let registry = Registry::default();
        let metrics = AuthorityStorePruningMetrics::new(&registry);
        AuthorityStorePruner::prune_objects(
            vec![effects],
            &perpetual_db,
            &lock_table(),
            0,
            metrics,
            1,
            true,
        )
        .await?;
        if let Ok(()) = perpetual_db.objects.flush() {
            info!("Completed flushing objects table");
        } else {
            error!("Failed to flush objects table");
        }
        let guard = pprof::ProfilerGuardBuilder::default()
            .frequency(1000)
            .build()
            .unwrap();
        read_keys(&perpetual_db.objects, 1000)?;
        if let Ok(report) = guard.report().build() {
            assert!(!report.data.keys().any(|f| f
                .frames
                .iter()
                .any(|vs| is_rocksdb_range_tombstone_frame(vs))));
        }
        Ok(())
    }
}
