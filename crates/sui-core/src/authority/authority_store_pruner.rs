// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_store_types::{ObjectContentDigest, StoreData, StoreObject};
use crate::checkpoints::CheckpointStore;
use mysten_metrics::monitored_scope;
use prometheus::{
    register_int_counter_with_registry, register_int_gauge_with_registry, IntCounter, IntGauge,
    Registry,
};
use std::cmp::{max, min};
use std::collections::HashMap;
use std::{sync::Arc, time::Duration};
use sui_config::node::AuthorityStorePruningConfig;
use sui_storage::mutex_table::RwLockTable;
use sui_types::base_types::SequenceNumber;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::{
    base_types::{ObjectID, VersionNumber},
    storage::ObjectKey,
};
use tokio::sync::oneshot::{self, Sender};
use tokio::time::Instant;
use tracing::log::{debug, error};
use typed_store::{Map, TypedStoreError};

use super::authority_store_tables::AuthorityPerpetualTables;

pub struct AuthorityStorePruner {
    _objects_pruner_cancel_handle: oneshot::Sender<()>,
}

pub struct AuthorityStorePruningMetrics {
    pub last_pruned_checkpoint: IntGauge,
    pub num_pruned_objects: IntCounter,
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
        };
        Arc::new(this)
    }
}

#[derive(Debug, Clone, Copy)]
enum DeletionMethod {
    RangeDelete,
    PointDelete,
}

impl AuthorityStorePruner {
    /// prunes old versions of objects based on transaction effects
    async fn prune_effects(
        transaction_effects: Vec<TransactionEffects>,
        perpetual_db: &Arc<AuthorityPerpetualTables>,
        objects_lock_table: &Arc<RwLockTable<ObjectContentDigest>>,
        checkpoint_number: CheckpointSequenceNumber,
        deletion_method: DeletionMethod,
        metrics: Arc<AuthorityStorePruningMetrics>,
        indirect_objects_threshold: usize,
    ) -> anyhow::Result<()> {
        let _scope = monitored_scope("ObjectsLivePruner");
        let mut wb = perpetual_db.objects.batch();

        let mut object_keys_to_prune = vec![];
        for effects in &transaction_effects {
            for (object_id, seq_number) in effects.modified_at_versions() {
                object_keys_to_prune.push(ObjectKey(*object_id, *seq_number));
            }
        }
        metrics
            .num_pruned_objects
            .inc_by(object_keys_to_prune.len() as u64);
        let mut indirect_objects: HashMap<_, i64> = HashMap::new();

        if indirect_objects_threshold > 0 && indirect_objects_threshold < usize::MAX {
            for object in perpetual_db
                .objects
                .multi_get(object_keys_to_prune.iter())?
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

        match deletion_method {
            DeletionMethod::RangeDelete => {
                let mut updates: HashMap<ObjectID, (VersionNumber, VersionNumber)> = HashMap::new();
                for effects in transaction_effects {
                    for (object_id, seq_number) in effects.modified_at_versions() {
                        updates
                            .entry(*object_id)
                            .and_modify(|range| {
                                *range = (min(range.0, *seq_number), max(range.1, *seq_number))
                            })
                            .or_insert((*seq_number, *seq_number));
                    }
                }
                for (object_id, (min_version, max_version)) in updates {
                    let start_range = ObjectKey(object_id, min_version);
                    let end_range = ObjectKey(object_id, (max_version.value() + 1).into());
                    wb.delete_range(&perpetual_db.objects, &start_range, &end_range)?;
                }
            }
            DeletionMethod::PointDelete => {
                wb.delete_batch(&perpetual_db.objects, object_keys_to_prune)?;
            }
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

    /// Prunes old object versions based on effects from all checkpoints from epochs eligible for pruning
    pub async fn prune_objects_for_eligible_epochs(
        perpetual_db: &Arc<AuthorityPerpetualTables>,
        checkpoint_store: &Arc<CheckpointStore>,
        objects_lock_table: &Arc<RwLockTable<ObjectContentDigest>>,
        config: AuthorityStorePruningConfig,
        metrics: Arc<AuthorityStorePruningMetrics>,
        indirect_objects_threshold: usize,
    ) -> anyhow::Result<()> {
        let deletion_method = if config.use_range_deletion {
            DeletionMethod::RangeDelete
        } else {
            DeletionMethod::PointDelete
        };
        let mut checkpoint_number = perpetual_db.get_highest_pruned_checkpoint()?;
        let (highest_executed_checkpoint, current_epoch) = checkpoint_store
            .get_highest_executed_checkpoint()?
            .map(|c| (c.sequence_number, c.epoch()))
            .unwrap_or_default();
        let mut checkpoints_in_batch = 0;
        let mut batch_effects = vec![];
        let mut network_total_transactions = 0;

        debug!(
            "Starting object pruning. Current epoch: {}. Latest pruned checkpoint: {}",
            current_epoch, checkpoint_number
        );
        let iter = checkpoint_store
            .certified_checkpoints
            .iter()
            .skip_to(&(checkpoint_number + 1))?
            .map(|(k, ckpt)| (k, ckpt.into_inner()));

        #[allow(clippy::explicit_counter_loop)]
        for (_, checkpoint) in iter {
            checkpoint_number = *checkpoint.sequence_number();
            // Skipping because  checkpoint's epoch or checkpoint number is too new.
            // We have to respect the highest executed checkpoint watermark because there might be
            // parts of the system that still require access to old object versions (i.e. state accumulator)
            if (current_epoch < checkpoint.epoch() + config.num_epochs_to_retain)
                || (checkpoint_number > highest_executed_checkpoint)
            {
                break;
            }
            checkpoints_in_batch += 1;
            if network_total_transactions == checkpoint.network_total_transactions {
                continue;
            }
            network_total_transactions = checkpoint.network_total_transactions;

            let content = checkpoint_store
                .get_checkpoint_contents(&checkpoint.content_digest)?
                .ok_or_else(|| anyhow::anyhow!("checkpoint content data is missing"))?;
            let effects = perpetual_db
                .effects
                .multi_get(content.iter().map(|tx| tx.effects))?;

            if effects.iter().any(|effect| effect.is_none()) {
                return Err(anyhow::anyhow!("transaction effects data is missing"));
            }
            batch_effects.extend(effects.into_iter().flatten());

            if batch_effects.len() >= config.max_transactions_in_batch
                || checkpoints_in_batch >= config.max_checkpoints_in_batch
            {
                Self::prune_effects(
                    batch_effects,
                    perpetual_db,
                    objects_lock_table,
                    checkpoint_number,
                    deletion_method,
                    metrics.clone(),
                    indirect_objects_threshold,
                )
                .await?;
                batch_effects = vec![];
                checkpoints_in_batch = 0;
            }
        }
        if !batch_effects.is_empty() {
            Self::prune_effects(
                batch_effects,
                perpetual_db,
                objects_lock_table,
                checkpoint_number,
                deletion_method,
                metrics.clone(),
                indirect_objects_threshold,
            )
            .await?;
        }
        Ok(())
    }

    fn setup_objects_pruning(
        config: AuthorityStorePruningConfig,
        epoch_duration_ms: u64,
        perpetual_db: Arc<AuthorityPerpetualTables>,
        checkpoint_store: Arc<CheckpointStore>,
        objects_lock_table: Arc<RwLockTable<ObjectContentDigest>>,
        metrics: Arc<AuthorityStorePruningMetrics>,
        indirect_objects_threshold: usize,
    ) -> Sender<()> {
        let (sender, mut recv) = tokio::sync::oneshot::channel();
        debug!(
            "Starting object pruning service with num_epochs_to_retain={}",
            config.num_epochs_to_retain
        );
        let tick_duration = Duration::from_secs(config.pruning_run_delay_seconds.unwrap_or(
            if config.num_epochs_to_retain > 0 {
                min(1000 * epoch_duration_ms / 2, 60 * 60)
            } else {
                60
            },
        ));
        let pruning_initial_delay =
            Duration::from_secs(config.pruning_run_delay_seconds.unwrap_or(60 * 60));
        let mut prune_interval =
            tokio::time::interval_at(Instant::now() + pruning_initial_delay, tick_duration);

        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    _ = prune_interval.tick(), if config.num_epochs_to_retain != u64::MAX => {
                        if let Err(err) = Self::prune_objects_for_eligible_epochs(&perpetual_db, &checkpoint_store, &objects_lock_table, config, metrics.clone(), indirect_objects_threshold).await {
                            error!("Failed to prune objects: {:?}", err);
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
        objects_lock_table: Arc<RwLockTable<ObjectContentDigest>>,
        pruning_config: AuthorityStorePruningConfig,
        epoch_duration_ms: u64,
        registry: &Registry,
        indirect_objects_threshold: usize,
    ) -> Self {
        AuthorityStorePruner {
            _objects_pruner_cancel_handle: Self::setup_objects_pruning(
                pruning_config,
                epoch_duration_ms,
                perpetual_db,
                checkpoint_store,
                objects_lock_table,
                AuthorityStorePruningMetrics::new(registry),
                indirect_objects_threshold,
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
    use tracing::log::{error, info};

    use crate::authority::authority_store_pruner::{AuthorityStorePruningMetrics, DeletionMethod};
    use crate::authority::authority_store_tables::AuthorityPerpetualTables;
    use crate::authority::authority_store_types::{
        get_store_object_pair, ObjectContentDigest, StoreData, StoreObject, StoreObjectPair,
        StoreObjectWrapper,
    };
    #[cfg(not(target_env = "msvc"))]
    use pprof::Symbol;
    use prometheus::Registry;
    use sui_storage::mutex_table::RwLockTable;
    use sui_types::base_types::{ObjectDigest, VersionNumber};
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
            MetricConf::default(),
            &cfs,
        );

        let mut after_pruning = HashSet::new();
        let objects = DBMap::<ObjectKey, StoreObjectWrapper>::reopen(
            &perpetual_db?,
            Some("objects"),
            // open the db to bypass default db options which ignores range tombstones
            // so we can read the accurate number of retained versions
            &ReadWriteOptions::default(),
        )?;
        let iter = objects.iter();
        for (k, _v) in iter {
            after_pruning.insert(k);
        }
        Ok(after_pruning)
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
        *effects.modified_at_versions_mut_for_testing() = to_delete;
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

    fn generate_test_data(
        db: Arc<AuthorityPerpetualTables>,
        num_versions_per_object: u64,
        num_object_versions_to_retain: u64,
        total_unique_object_ids: u32,
    ) -> Result<(Vec<ObjectKey>, Vec<ObjectKey>), anyhow::Error> {
        let (mut to_keep, mut to_delete) = (vec![], vec![]);
        let mut batch = db.objects.batch();

        let ids = ObjectID::in_range(ObjectID::ZERO, total_unique_object_ids.into())?;
        for id in ids {
            for (counter, i) in (0..num_versions_per_object).rev().enumerate() {
                let object_key = ObjectKey(id, SequenceNumber::from_u64(i));
                if counter < num_object_versions_to_retain.try_into().unwrap() {
                    // latest `num_object_versions_to_retain` should not have been pruned
                    to_keep.push(object_key);
                } else {
                    to_delete.push(object_key);
                }
                let StoreObjectPair(obj, indirect_obj) =
                    get_store_object_pair(Object::immutable_with_id_for_testing(id), 1);
                batch.insert_batch(
                    &db.objects,
                    [(ObjectKey(id, SequenceNumber::from(i)), obj.clone())],
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
        }
        batch.write().unwrap();
        assert_eq!(
            to_keep.len() as u64,
            std::cmp::min(num_object_versions_to_retain, num_versions_per_object)
                * total_unique_object_ids as u64
        );
        Ok((to_keep, to_delete))
    }

    fn lock_table() -> Arc<RwLockTable<ObjectContentDigest>> {
        Arc::new(RwLockTable::new(1))
    }

    async fn run_pruner(
        path: &Path,
        num_versions_per_object: u64,
        num_object_versions_to_retain: u64,
        total_unique_object_ids: u32,
        deletion_method: DeletionMethod,
    ) -> Vec<ObjectKey> {
        let registry = Registry::default();
        let metrics = AuthorityStorePruningMetrics::new(&registry);
        let to_keep = {
            let db = Arc::new(AuthorityPerpetualTables::open(path, None));
            let (to_keep, to_delete) = generate_test_data(
                db.clone(),
                num_versions_per_object,
                num_object_versions_to_retain,
                total_unique_object_ids,
            )
            .unwrap();
            let mut effects = TransactionEffects::default();
            *effects.modified_at_versions_mut_for_testing() =
                to_delete.into_iter().map(|o| (o.0, o.1)).collect();
            AuthorityStorePruner::prune_effects(
                vec![effects],
                &db,
                &lock_table(),
                0,
                deletion_method,
                metrics,
                1,
            )
            .await
            .unwrap();
            to_keep
        };
        tokio::time::sleep(Duration::from_secs(3)).await;
        to_keep
    }

    #[tokio::test]
    async fn test_pruning() {
        let path = tempfile::tempdir().unwrap().into_path();
        let to_keep = run_pruner(&path, 3, 2, 1000, DeletionMethod::PointDelete).await;
        assert_eq!(
            HashSet::from_iter(to_keep),
            get_keys_after_pruning(&path).unwrap()
        );
        run_pruner(
            &tempfile::tempdir().unwrap().into_path(),
            3,
            2,
            1000,
            DeletionMethod::RangeDelete,
        )
        .await;
    }

    #[tokio::test]
    async fn test_ref_count_pruning() {
        let path = tempfile::tempdir().unwrap().into_path();
        run_pruner(&path, 3, 2, 1000, DeletionMethod::RangeDelete).await;
        {
            let perpetual_db = AuthorityPerpetualTables::open(&path, None);
            let count = perpetual_db.indirect_move_objects.keys().count();
            // references are not reset, expected to have 1000 unique objects
            assert_eq!(count, 1000);
        }

        let path = tempfile::tempdir().unwrap().into_path();
        run_pruner(&path, 3, 0, 1000, DeletionMethod::RangeDelete).await;
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
        *effects.modified_at_versions_mut_for_testing() = to_delete;
        let registry = Registry::default();
        let metrics = AuthorityStorePruningMetrics::new(&registry);
        let total_pruned = AuthorityStorePruner::prune_effects(
            vec![effects],
            &perpetual_db,
            &lock_table(),
            0,
            DeletionMethod::RangeDelete,
            metrics,
            0,
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

    #[cfg(not(target_env = "msvc"))]
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
        AuthorityStorePruner::prune_effects(
            vec![effects],
            &perpetual_db,
            &lock_table(),
            0,
            DeletionMethod::RangeDelete,
            metrics,
            1,
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
        AuthorityStorePruner::prune_effects(
            vec![effects],
            &perpetual_db,
            &lock_table(),
            0,
            DeletionMethod::RangeDelete,
            metrics,
            1,
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
