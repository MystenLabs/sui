// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::authority_store_tables::{AuthorityPerpetualTables, AuthorityPrunerTables};
use crate::authority::authority_store_types::{StoreObject, StoreObjectWrapper};
use crate::checkpoints::{CheckpointStore, CheckpointWatermark};
use crate::jsonrpc_index::IndexStore;
use crate::rpc_index::RpcIndexStore;
use anyhow::anyhow;
use bincode::Options;
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use once_cell::sync::Lazy;
use prometheus::{
    IntCounter, IntGauge, Registry, register_int_counter_with_registry,
    register_int_gauge_with_registry,
};
#[cfg(tidehunter)]
use serde::de::DeserializeOwned;
use std::cmp::{max, min};
use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::AtomicU64;
use std::sync::{Mutex, Weak};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{sync::Arc, time::Duration};
use sui_config::node::AuthorityStorePruningConfig;
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
use typed_store::rocksdb::LiveFile;
use typed_store::rocksdb::compaction_filter::Decision;
use typed_store::{Map, TypedStoreError};

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

#[derive(Default)]
pub struct PrunerWatermarks {
    pub epoch_id: Arc<AtomicU64>,
    pub checkpoint_id: Arc<AtomicU64>,
}

static MIN_PRUNING_TICK_DURATION_MS: u64 = 10 * 1000;

pub struct AuthorityStorePruningMetrics {
    pub last_pruned_checkpoint: IntGauge,
    pub num_pruned_objects: IntCounter,
    pub num_pruned_tombstones: IntCounter,
    pub last_pruned_effects_checkpoint: IntGauge,
    pub last_pruned_indexes_transaction: IntGauge,
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
            last_pruned_indexes_transaction: register_int_gauge_with_registry!(
                "last_pruned_indexes_transaction",
                "Last pruned indexes transaction",
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
        pruner_db: Option<&Arc<AuthorityPrunerTables>>,
        checkpoint_number: CheckpointSequenceNumber,
        metrics: Arc<AuthorityStorePruningMetrics>,
        enable_pruning_tombstones: bool,
    ) -> anyhow::Result<()> {
        let _scope = monitored_scope("ObjectsLivePruner");
        let mut wb = perpetual_db.objects.batch();
        let mut pruner_db_wb = pruner_db.map(|db| db.object_tombstones.batch());

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
            match pruner_db_wb {
                Some(ref mut batch) => {
                    batch.insert_batch(
                        &pruner_db.expect("invariant checked").object_tombstones,
                        std::iter::once((object_id, max_version)),
                    )?;
                }
                None => {
                    let start_range = ObjectKey(object_id, min_version);
                    let end_range = ObjectKey(object_id, (max_version.value() + 1).into());
                    wb.schedule_delete_range(&perpetual_db.objects, &start_range, &end_range)?;
                }
            }
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

        perpetual_db.set_highest_pruned_checkpoint(&mut wb, checkpoint_number)?;
        metrics.last_pruned_checkpoint.set(checkpoint_number as i64);

        if let Some(batch) = pruner_db_wb {
            batch.write()?;
        }
        wb.write()?;
        Ok(())
    }

    fn prune_checkpoints(
        perpetual_db: &Arc<AuthorityPerpetualTables>,
        checkpoint_db: &Arc<CheckpointStore>,
        rpc_index: Option<&RpcIndexStore>,
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
            transactions.iter(),
        )?;

        let mut effect_digests = vec![];
        for effects in effects_to_prune {
            let effects_digest = effects.digest();
            debug!("Pruning effects {:?}", effects_digest);
            effect_digests.push(effects_digest);

            if effects.events_digest().is_some() {
                perpetual_batch
                    .delete_batch(&perpetual_db.events_2, [effects.transaction_digest()])?;
            }
        }
        perpetual_batch.delete_batch(
            &perpetual_db.unchanged_loaded_runtime_objects,
            transactions.iter(),
        )?;
        perpetual_batch.delete_batch(&perpetual_db.effects, effect_digests)?;

        let mut checkpoints_batch = checkpoint_db.tables.certified_checkpoints.batch();

        let checkpoint_content_digests =
            checkpoint_content_to_prune.iter().map(|ckpt| ckpt.digest());
        checkpoints_batch.delete_batch(
            &checkpoint_db.tables.checkpoint_content,
            checkpoint_content_digests.clone(),
        )?;
        checkpoints_batch.delete_batch(
            &checkpoint_db.tables.checkpoint_sequence_by_contents_digest,
            checkpoint_content_digests,
        )?;

        checkpoints_batch.delete_batch(
            &checkpoint_db.tables.checkpoint_by_digest,
            checkpoints_to_prune,
        )?;

        checkpoints_batch.insert_batch(
            &checkpoint_db.tables.watermarks,
            [(
                &CheckpointWatermark::HighestPruned,
                &(checkpoint_number, CheckpointDigest::random()),
            )],
        )?;

        if let Some(rpc_index) = rpc_index {
            rpc_index.prune(checkpoint_number, &checkpoint_content_to_prune)?;
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
        rpc_index: Option<&RpcIndexStore>,
        pruner_db: Option<&Arc<AuthorityPrunerTables>>,
        config: AuthorityStorePruningConfig,
        metrics: Arc<AuthorityStorePruningMetrics>,
        epoch_duration_ms: u64,
    ) -> anyhow::Result<()> {
        let _scope = monitored_scope("PruneObjectsForEligibleEpochs");
        let (mut max_eligible_checkpoint_number, epoch_id) = checkpoint_store
            .get_highest_executed_checkpoint()?
            .map(|c| (*c.sequence_number(), c.epoch))
            .unwrap_or_default();
        let pruned_checkpoint_number = perpetual_db
            .get_highest_pruned_checkpoint()?
            .unwrap_or_default();
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
            rpc_index,
            pruner_db,
            PruningMode::Objects,
            config.num_epochs_to_retain,
            pruned_checkpoint_number,
            max_eligible_checkpoint_number,
            config,
            metrics.clone(),
        )
        .await
    }

    pub async fn prune_checkpoints_for_eligible_epochs(
        perpetual_db: &Arc<AuthorityPerpetualTables>,
        checkpoint_store: &Arc<CheckpointStore>,
        rpc_index: Option<&RpcIndexStore>,
        pruner_db: Option<&Arc<AuthorityPrunerTables>>,
        config: AuthorityStorePruningConfig,
        metrics: Arc<AuthorityStorePruningMetrics>,
        epoch_duration_ms: u64,
        pruner_watermarks: &Arc<PrunerWatermarks>,
    ) -> anyhow::Result<()> {
        let _scope = monitored_scope("PruneCheckpointsForEligibleEpochs");
        let pruned_checkpoint_number = checkpoint_store
            .get_highest_pruned_checkpoint_seq_number()?
            .unwrap_or(0);
        let (mut max_eligible_checkpoint, epoch_id) = checkpoint_store
            .get_highest_executed_checkpoint()?
            .map(|c| (*c.sequence_number(), c.epoch))
            .unwrap_or_default();
        if config.num_epochs_to_retain != u64::MAX {
            max_eligible_checkpoint = min(
                max_eligible_checkpoint,
                perpetual_db
                    .get_highest_pruned_checkpoint()?
                    .unwrap_or_default(),
            );
        }
        if config.smooth
            && let Some(num_epochs_to_retain) = config.num_epochs_to_retain_for_checkpoints
        {
            max_eligible_checkpoint = Self::smoothed_max_eligible_checkpoint_number(
                checkpoint_store,
                max_eligible_checkpoint,
                pruned_checkpoint_number,
                epoch_id,
                epoch_duration_ms,
                num_epochs_to_retain,
            )?;
        }
        debug!("Max eligible checkpoint {}", max_eligible_checkpoint);
        Self::prune_for_eligible_epochs(
            perpetual_db,
            checkpoint_store,
            rpc_index,
            pruner_db,
            PruningMode::Checkpoints,
            config
                .num_epochs_to_retain_for_checkpoints()
                .ok_or_else(|| anyhow!("config value not set"))?,
            pruned_checkpoint_number,
            max_eligible_checkpoint,
            config.clone(),
            metrics.clone(),
        )
        .await?;

        if let Some(num_epochs_to_retain) = config.num_epochs_to_retain_for_checkpoints() {
            Self::update_pruning_watermarks(
                checkpoint_store,
                num_epochs_to_retain,
                pruner_watermarks,
            )?;
        }
        Ok(())
    }

    /// Prunes old object versions based on effects from all checkpoints from epochs eligible for pruning
    pub async fn prune_for_eligible_epochs(
        perpetual_db: &Arc<AuthorityPerpetualTables>,
        checkpoint_store: &Arc<CheckpointStore>,
        rpc_index: Option<&RpcIndexStore>,
        pruner_db: Option<&Arc<AuthorityPrunerTables>>,
        mode: PruningMode,
        num_epochs_to_retain: u64,
        starting_checkpoint_number: CheckpointSequenceNumber,
        max_eligible_checkpoint: CheckpointSequenceNumber,
        config: AuthorityStorePruningConfig,
        metrics: Arc<AuthorityStorePruningMetrics>,
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
                .tables
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
                            pruner_db,
                            checkpoint_number,
                            metrics.clone(),
                            !config.killswitch_tombstone_pruning,
                        )
                        .await?
                    }
                    PruningMode::Checkpoints => Self::prune_checkpoints(
                        perpetual_db,
                        checkpoint_store,
                        rpc_index,
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
                        pruner_db,
                        checkpoint_number,
                        metrics.clone(),
                        !config.killswitch_tombstone_pruning,
                    )
                    .await?
                }
                PruningMode::Checkpoints => Self::prune_checkpoints(
                    perpetual_db,
                    checkpoint_store,
                    rpc_index,
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

    fn prune_indexes(
        indexes: Option<&IndexStore>,
        config: &AuthorityStorePruningConfig,
        epoch_duration_ms: u64,
        metrics: &AuthorityStorePruningMetrics,
    ) -> anyhow::Result<()> {
        if let (Some(mut epochs_to_retain), Some(indexes)) =
            (config.num_epochs_to_retain_for_indexes, indexes)
        {
            if epochs_to_retain < 7 {
                warn!("num_epochs_to_retain_for_indexes is too low. Reseting it to 7");
                epochs_to_retain = 7;
            }
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
            if let Some(cut_time_ms) =
                u64::try_from(now)?.checked_sub(epochs_to_retain * epoch_duration_ms)
            {
                let transaction_id = indexes.prune(cut_time_ms)?;
                metrics
                    .last_pruned_indexes_transaction
                    .set(transaction_id as i64);
            }
        }
        Ok(())
    }

    async fn prune_executed_tx_digests(
        perpetual_db: &Arc<AuthorityPerpetualTables>,
        checkpoint_store: &Arc<CheckpointStore>,
    ) -> anyhow::Result<()> {
        let current_epoch = checkpoint_store
            .get_highest_executed_checkpoint()?
            .map(|c| c.epoch)
            .unwrap_or_default();

        if current_epoch < 2 {
            return Ok(());
        }

        let target_epoch = current_epoch - 1;

        use sui_types::base_types::TransactionDigest;
        let start_key = (0u64, TransactionDigest::ZERO);
        let end_key = (target_epoch, TransactionDigest::ZERO);

        info!(
            "Pruning executed_transaction_digests for epochs < {} (current epoch: {})",
            target_epoch, current_epoch
        );

        let mut batch = perpetual_db.executed_transaction_digests.batch();
        batch.schedule_delete_range(
            &perpetual_db.executed_transaction_digests,
            &start_key,
            // `to` is non-inclusive so target_epoch and all later epochs are preserved
            &end_key,
        )?;
        batch.write()?;
        Ok(())
    }

    #[cfg(tidehunter)]
    fn prune_executed_tx_digests_th(
        perpetual_db: &Arc<AuthorityPerpetualTables>,
        checkpoint_store: &Arc<CheckpointStore>,
    ) -> anyhow::Result<()> {
        let current_epoch = checkpoint_store
            .get_highest_executed_checkpoint()?
            .map(|c| c.epoch)
            .unwrap_or_default();

        if current_epoch < 2 {
            return Ok(());
        }

        let last_epoch_to_delete = current_epoch - 2;
        let from_key = (0u64, TransactionDigest::ZERO);
        let to_key = (last_epoch_to_delete, TransactionDigest::new([0xff; 32]));
        info!(
            "Pruning executed_transaction_digests for epochs 0 to {} (current epoch: {})",
            last_epoch_to_delete, current_epoch
        );
        perpetual_db
            .executed_transaction_digests
            .drop_cells_in_range(&from_key, &to_key)?;
        Ok(())
    }

    fn update_pruning_watermarks(
        checkpoint_store: &Arc<CheckpointStore>,
        num_epochs_to_retain: u64,
        pruning_watermark: &Arc<PrunerWatermarks>,
    ) -> anyhow::Result<bool> {
        use std::sync::atomic::Ordering;
        let current_watermark = pruning_watermark.epoch_id.load(Ordering::Relaxed);
        let current_epoch_id = checkpoint_store
            .get_highest_executed_checkpoint()?
            .map(|c| c.epoch)
            .unwrap_or_default();
        if current_epoch_id < num_epochs_to_retain {
            return Ok(false);
        }
        let target_epoch_id = current_epoch_id - num_epochs_to_retain;
        let checkpoint_id =
            checkpoint_store.get_epoch_last_checkpoint_seq_number(target_epoch_id)?;

        let new_watermark = target_epoch_id + 1;
        if current_watermark == new_watermark {
            return Ok(false);
        }
        info!("relocation: setting epoch watermark to {}", new_watermark);
        pruning_watermark
            .epoch_id
            .store(new_watermark, Ordering::Relaxed);
        if let Some(checkpoint_id) = checkpoint_id {
            info!(
                "relocation: setting checkpoint watermark to {}",
                checkpoint_id
            );
            pruning_watermark
                .checkpoint_id
                .store(checkpoint_id, Ordering::Relaxed);
        }
        Ok(true)
    }

    #[cfg(tidehunter)]
    fn prune_th(
        perpetual_db: &Arc<AuthorityPerpetualTables>,
        checkpoint_store: &Arc<CheckpointStore>,
        num_epochs_to_retain: u64,
        pruning_watermark: Arc<PrunerWatermarks>,
    ) -> anyhow::Result<()> {
        let watermark_updated = Self::update_pruning_watermarks(
            checkpoint_store,
            num_epochs_to_retain,
            &pruning_watermark,
        )?;
        if !watermark_updated {
            info!("skip relocation. Watermark hasn't changed");
            return Ok(());
        }
        perpetual_db.objects.db.start_relocation()?;
        checkpoint_store.tables.watermarks.db.start_relocation()?;
        Self::prune_executed_tx_digests_th(perpetual_db, checkpoint_store)?;
        Ok(())
    }

    fn compact_next_sst_file(
        perpetual_db: Arc<AuthorityPerpetualTables>,
        delay_days: usize,
        last_processed: Arc<Mutex<HashMap<String, SystemTime>>>,
    ) -> anyhow::Result<Option<LiveFile>> {
        let db_path = perpetual_db.objects.db.path_for_pruning();
        let mut state = last_processed
            .lock()
            .expect("failed to obtain a lock for last processed SST files");
        let mut sst_file_for_compaction: Option<LiveFile> = None;
        let time_threshold =
            SystemTime::now() - Duration::from_secs(delay_days as u64 * 24 * 60 * 60);
        for sst_file in perpetual_db.objects.db.live_files()? {
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
            if let Some(candidate) = &sst_file_for_compaction
                && candidate.size > sst_file.size
            {
                continue;
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
        rpc_index: Option<Arc<RpcIndexStore>>,
        jsonrpc_index: Option<Arc<IndexStore>>,
        pruner_db: Option<Arc<AuthorityPrunerTables>>,
        metrics: Arc<AuthorityStorePruningMetrics>,
        pruner_watermarks: Arc<PrunerWatermarks>,
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

        metrics
            .num_epochs_to_retain_for_objects
            .set(config.num_epochs_to_retain as i64);
        metrics.num_epochs_to_retain_for_checkpoints.set(
            config
                .num_epochs_to_retain_for_checkpoints
                .unwrap_or_default() as i64,
        );

        #[cfg(tidehunter)]
        {
            if let Some(num_epochs_to_retain) = config.num_epochs_to_retain_for_checkpoints() {
                tokio::task::spawn(async move {
                    loop {
                        objects_prune_interval.tick().await;
                        if let Err(err) = Self::prune_th(
                            &perpetual_db,
                            &checkpoint_store,
                            num_epochs_to_retain,
                            pruner_watermarks.clone(),
                        ) {
                            error!("Failed to prune tidehunter: {:?}", err);
                        }
                    }
                });
            }
        }
        #[cfg(not(tidehunter))]
        {
            let mut checkpoints_prune_interval =
                tokio::time::interval_at(Instant::now() + pruning_initial_delay, tick_duration);
            let mut indexes_prune_interval =
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
            tokio::task::spawn(async move {
                loop {
                    tokio::select! {
                        _ = objects_prune_interval.tick(), if config.num_epochs_to_retain != u64::MAX => {
                            if let Err(err) = Self::prune_objects_for_eligible_epochs(&perpetual_db, &checkpoint_store, rpc_index.as_deref(), pruner_db.as_ref(), config.clone(), metrics.clone(), epoch_duration_ms).await {
                                error!("Failed to prune objects: {:?}", err);
                            }
                            if let Err(err) = Self::prune_executed_tx_digests(&perpetual_db, &checkpoint_store).await {
                                error!("Failed to prune executed_tx_digests: {:?}", err);
                            }
                        },
                        _ = checkpoints_prune_interval.tick(), if !matches!(config.num_epochs_to_retain_for_checkpoints(), None | Some(u64::MAX) | Some(0)) => {
                            if let Err(err) = Self::prune_checkpoints_for_eligible_epochs(&perpetual_db, &checkpoint_store, rpc_index.as_deref(), pruner_db.as_ref(), config.clone(), metrics.clone(), epoch_duration_ms, &pruner_watermarks).await {
                                error!("Failed to prune checkpoints: {:?}", err);
                            }
                        },
                        _ = indexes_prune_interval.tick(), if config.num_epochs_to_retain_for_indexes.is_some() => {
                            if let Err(err) = Self::prune_indexes(jsonrpc_index.as_deref(), &config, epoch_duration_ms, &metrics) {
                                error!("Failed to prune indexes: {:?}", err);
                            }
                        }
                        _ = &mut recv => break,
                    }
                }
            });
        }
        sender
    }

    pub fn new(
        perpetual_db: Arc<AuthorityPerpetualTables>,
        checkpoint_store: Arc<CheckpointStore>,
        rpc_index: Option<Arc<RpcIndexStore>>,
        jsonrpc_index: Option<Arc<IndexStore>>,
        mut pruning_config: AuthorityStorePruningConfig,
        is_validator: bool,
        epoch_duration_ms: u64,
        registry: &Registry,
        pruner_db: Option<Arc<AuthorityPrunerTables>>,
        pruner_watermarks: Arc<PrunerWatermarks>, // used by tidehunter relocation filters
    ) -> Self {
        if pruning_config.num_epochs_to_retain > 0 && pruning_config.num_epochs_to_retain < u64::MAX
        {
            warn!(
                "Using objects pruner with num_epochs_to_retain = {} can lead to performance issues",
                pruning_config.num_epochs_to_retain
            );
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
                rpc_index,
                jsonrpc_index,
                pruner_db,
                AuthorityStorePruningMetrics::new(registry),
                pruner_watermarks,
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

#[cfg(tidehunter)]
pub(crate) fn apply_relocation_filter<T: DeserializeOwned>(
    config: typed_store::tidehunter_util::KeySpaceConfig,
    pruner_watermark: Arc<AtomicU64>,
    extractor: impl Fn(T) -> u64 + Send + Sync + 'static,
    by_key: bool,
) -> typed_store::tidehunter_util::KeySpaceConfig {
    use bincode::Options;
    use std::sync::atomic::Ordering;
    use typed_store::tidehunter_util::Decision;
    config.with_relocation_filter(move |key, value| {
        let data = if by_key {
            bincode::DefaultOptions::new()
                .with_big_endian()
                .with_fixint_encoding()
                .deserialize(&key)
                .expect("relocation filter deserialization error")
        } else {
            bcs::from_bytes(&value).expect("relocation filter deserialization error")
        };
        if extractor(data) < pruner_watermark.load(Ordering::Relaxed) {
            Decision::Remove
        } else {
            Decision::StopRelocation
        }
    })
}

#[derive(Clone)]
pub struct ObjectsCompactionFilter {
    db: Weak<AuthorityPrunerTables>,
    metrics: Arc<ObjectCompactionMetrics>,
}

impl ObjectsCompactionFilter {
    pub fn new(db: Arc<AuthorityPrunerTables>, registry: &Registry) -> Self {
        Self {
            db: Arc::downgrade(&db),
            metrics: ObjectCompactionMetrics::new(registry),
        }
    }
    pub fn filter(&mut self, key: &[u8], value: &[u8]) -> anyhow::Result<Decision> {
        let ObjectKey(object_id, version) = bincode::DefaultOptions::new()
            .with_big_endian()
            .with_fixint_encoding()
            .deserialize(key)?;
        let object: StoreObjectWrapper = bcs::from_bytes(value)?;
        if matches!(object.into_inner(), StoreObject::Value(_))
            && let Some(db) = self.db.upgrade()
        {
            match db.object_tombstones.get(&object_id)? {
                Some(gc_version) => {
                    if version <= gc_version {
                        self.metrics.key_removed.inc();
                        return Ok(Decision::Remove);
                    }
                    self.metrics.key_kept.inc();
                }
                None => self.metrics.key_not_found.inc(),
            }
        }
        Ok(Decision::Keep)
    }
}

struct ObjectCompactionMetrics {
    key_removed: IntCounter,
    key_kept: IntCounter,
    key_not_found: IntCounter,
}

impl ObjectCompactionMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            key_removed: register_int_counter_with_registry!(
                "objects_compaction_filter_key_removed",
                "Compaction key removed",
                registry
            )
            .unwrap(),
            key_kept: register_int_counter_with_registry!(
                "objects_compaction_filter_key_kept",
                "Compaction key kept",
                registry
            )
            .unwrap(),
            key_not_found: register_int_counter_with_registry!(
                "objects_compaction_filter_key_not_found",
                "Compaction key not found",
                registry
            )
            .unwrap(),
        })
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
        StoreObject, StoreObjectWrapper, get_store_object,
    };
    use prometheus::Registry;
    use sui_types::base_types::ObjectDigest;
    use sui_types::effects::TransactionEffects;
    use sui_types::effects::TransactionEffectsAPI;
    use sui_types::{
        base_types::{ObjectID, SequenceNumber},
        object::Object,
        storage::ObjectKey,
    };
    use typed_store::Map;
    use typed_store::rocks::{DBMap, MetricConf, ReadWriteOptions, default_db_options};

    use super::AuthorityStorePruner;

    fn get_keys_after_pruning(path: &Path) -> anyhow::Result<HashSet<ObjectKey>> {
        let perpetual_db_path = path.join(Path::new("perpetual"));
        let cf_names = AuthorityPerpetualTables::describe_tables();
        let cfs: Vec<_> = cf_names
            .keys()
            .map(|x| (x.as_str(), default_db_options().options))
            .collect();
        let perpetual_db = typed_store::rocks::open_cf_opts(
            perpetual_db_path,
            None,
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
        let iter = objects.safe_iter();
        for item in iter {
            after_pruning.insert(item?.0);
        }
        Ok(after_pruning)
    }

    type GenerateTestDataResult = (Vec<ObjectKey>, Vec<ObjectKey>, Vec<ObjectKey>);

    fn generate_test_data(
        db: Arc<AuthorityPerpetualTables>,
        num_versions_per_object: u64,
        num_object_versions_to_retain: u64,
        total_unique_object_ids: u32,
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
                let obj = get_store_object(Object::immutable_with_id_for_testing(id));
                batch.insert_batch(
                    &db.objects,
                    [(ObjectKey(id, SequenceNumber::from(seq)), obj.clone())],
                )?;
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

    async fn run_pruner(
        path: &Path,
        num_versions_per_object: u64,
        num_object_versions_to_retain: u64,
        total_unique_object_ids: u32,
    ) -> Vec<ObjectKey> {
        let registry = Registry::default();
        let metrics = AuthorityStorePruningMetrics::new(&registry);
        let to_keep = {
            let db = Arc::new(AuthorityPerpetualTables::open(path, None, None));
            let (to_keep, to_delete, tombstones) = generate_test_data(
                db.clone(),
                num_versions_per_object,
                num_object_versions_to_retain,
                total_unique_object_ids,
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
            AuthorityStorePruner::prune_objects(vec![effects], &db, None, 0, metrics, true)
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
        let path = tempfile::tempdir().unwrap().keep();
        let to_keep = run_pruner(&path, 3, 2, 1000).await;
        assert_eq!(
            HashSet::from_iter(to_keep),
            get_keys_after_pruning(&path).unwrap()
        );
        run_pruner(&tempfile::tempdir().unwrap().keep(), 3, 2, 1000).await;
    }

    // Tests pruning deleted objects (object tombstones).
    #[tokio::test]
    async fn test_pruning_tombstones() {
        let path = tempfile::tempdir().unwrap().keep();
        let to_keep = run_pruner(&path, 0, 0, 1000).await;
        assert_eq!(to_keep.len(), 0);
        assert_eq!(get_keys_after_pruning(&path).unwrap().len(), 0);

        let path = tempfile::tempdir().unwrap().keep();
        let to_keep = run_pruner(&path, 3, 0, 1000).await;
        assert_eq!(to_keep.len(), 0);
        assert_eq!(get_keys_after_pruning(&path).unwrap().len(), 0);
    }

    #[cfg(not(target_env = "msvc"))]
    #[tokio::test]
    async fn test_db_size_after_compaction() -> Result<(), anyhow::Error> {
        let primary_path = tempfile::tempdir()?.keep();
        let perpetual_db = Arc::new(AuthorityPerpetualTables::open(&primary_path, None, None));
        let total_unique_object_ids = 10_000;
        let num_versions_per_object = 10;
        let ids = ObjectID::in_range(ObjectID::ZERO, total_unique_object_ids)?;
        let mut to_delete = vec![];
        for id in ids {
            for i in (0..num_versions_per_object).rev() {
                if i < num_versions_per_object - 2 {
                    to_delete.push((id, SequenceNumber::from(i)));
                }
                let obj = get_store_object(Object::immutable_with_id_for_testing(id));
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

        perpetual_db.objects.compact_range(&start, &end)?;
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
            None,
            0,
            metrics,
            true,
        )
        .await;
        info!("Total pruned keys = {:?}", total_pruned);

        perpetual_db.objects.compact_range(&start, &end)?;
        let after_compaction_size = get_sst_size(&db_path);

        info!(
            "Before compaction disk size = {:?}, after compaction disk size = {:?}",
            before_compaction_size, after_compaction_size
        );
        ma::assert_le!(after_compaction_size, before_compaction_size);
        Ok(())
    }
}
