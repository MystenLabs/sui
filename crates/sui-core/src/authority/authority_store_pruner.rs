// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_store_types::StoreData;
use crate::checkpoints::CheckpointStore;
use mysten_metrics::monitored_scope;
use std::cmp::{max, min};
use std::collections::HashMap;
use std::{sync::Arc, time::Duration};
use sui_config::node::AuthorityStorePruningConfig;
use sui_types::digests::CheckpointDigest;
use sui_types::messages::{TransactionEffects, TransactionEffectsAPI};
use sui_types::{
    base_types::{ObjectID, VersionNumber},
    storage::ObjectKey,
};
use tokio::sync::oneshot::{self, Sender};
use tokio::time::Instant;
use tracing::log::{debug, error};
use typed_store::Map;

use super::authority_store_tables::AuthorityPerpetualTables;

pub struct AuthorityStorePruner {
    _objects_pruner_cancel_handle: oneshot::Sender<()>,
}

#[derive(Debug, Clone, Copy)]
enum DeletionMethod {
    RangeDelete,
    PointDelete,
}

impl AuthorityStorePruner {
    /// prunes old versions of objects based on transaction effects
    fn prune_effects(
        transaction_effects: Vec<TransactionEffects>,
        perpetual_db: &Arc<AuthorityPerpetualTables>,
        deletion_method: DeletionMethod,
    ) -> anyhow::Result<()> {
        let _scope = monitored_scope("ObjectsLivePruner");
        let mut wb = perpetual_db.objects.batch();

        let mut object_keys_to_prune = vec![];
        for effects in &transaction_effects {
            for (object_id, seq_number) in effects.modified_at_versions() {
                object_keys_to_prune.push(ObjectKey(*object_id, *seq_number));
            }
        }
        let mut indirect_objects: HashMap<_, i64> = HashMap::new();
        for object in perpetual_db
            .objects
            .multi_get(object_keys_to_prune.iter())?
        {
            if let Some(StoreData::IndirectObject(indirect_object)) = object.map(|o| o.data) {
                *indirect_objects.entry(indirect_object.digest).or_default() -= 1;
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
                    wb = wb.delete_range(&perpetual_db.objects, &start_range, &end_range)?;
                }
            }
            DeletionMethod::PointDelete => {
                wb = wb.delete_batch(&perpetual_db.objects, object_keys_to_prune)?;
            }
        }
        if !indirect_objects.is_empty() {
            let ref_count_update = indirect_objects
                .into_iter()
                .map(|(digest, delta)| (digest, delta.to_le_bytes()));
            wb = wb.partial_merge_batch(&perpetual_db.indirect_move_objects, ref_count_update)?;
        }
        wb.write()?;
        Ok(())
    }

    /// Prunes old object versions based on effects from all checkpoints from epochs eligible for pruning
    fn prune_objects_for_eligible_epochs(
        perpetual_db: &Arc<AuthorityPerpetualTables>,
        checkpoint_store: &Arc<CheckpointStore>,
        config: AuthorityStorePruningConfig,
    ) -> anyhow::Result<()> {
        let deletion_method = if config.use_range_deletion {
            DeletionMethod::RangeDelete
        } else {
            DeletionMethod::PointDelete
        };
        let mut checkpoint_number = checkpoint_store
            .get_highest_pruned_checkpoint_seq_number()?
            .unwrap_or_default();
        let mut checkpoint_digest = CheckpointDigest::random();
        let current_epoch = checkpoint_store
            .get_highest_executed_checkpoint()?
            .map(|c| c.epoch())
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
            .skip_to(&(checkpoint_number + 1))?;

        #[allow(clippy::explicit_counter_loop)]
        for (_, checkpoint) in iter {
            // checkpoint's epoch is too new. Skipping for now
            if current_epoch < checkpoint.epoch() + config.num_epochs_to_retain {
                break;
            }
            checkpoint_number = checkpoint.sequence_number();
            checkpoint_digest = checkpoint.digest();
            checkpoints_in_batch += 1;
            if network_total_transactions == checkpoint.summary.network_total_transactions {
                continue;
            }
            network_total_transactions = checkpoint.summary.network_total_transactions;

            let content = checkpoint_store
                .get_checkpoint_contents(&checkpoint.content_digest())?
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
                Self::prune_effects(batch_effects, perpetual_db, deletion_method)?;
                checkpoint_store
                    .update_highest_pruned_checkpoint(checkpoint_number, checkpoint_digest)?;
                batch_effects = vec![];
                checkpoints_in_batch = 0;
            }
        }
        if !batch_effects.is_empty() {
            Self::prune_effects(batch_effects, perpetual_db, deletion_method)?;
            checkpoint_store
                .update_highest_pruned_checkpoint(checkpoint_number, checkpoint_digest)?;
        }
        debug!(
            "Finished pruner iteration. Latest pruned checkpoint: {}",
            checkpoint_number
        );
        Ok(())
    }

    fn setup_objects_pruning(
        config: AuthorityStorePruningConfig,
        epoch_duration_ms: u64,
        perpetual_db: Arc<AuthorityPerpetualTables>,
        checkpoint_store: Arc<CheckpointStore>,
    ) -> Sender<()> {
        let (sender, mut recv) = tokio::sync::oneshot::channel();
        debug!(
            "Starting object pruning service with num_epochs_to_retain={}",
            config.num_epochs_to_retain
        );
        let tick_duration = if config.num_epochs_to_retain > 0 {
            Duration::from_millis(epoch_duration_ms / 2)
        } else {
            Duration::from_secs(1)
        };

        let pruning_initial_delay = min(tick_duration, Duration::from_secs(300));
        let mut prune_interval =
            tokio::time::interval_at(Instant::now() + pruning_initial_delay, tick_duration);

        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    _ = prune_interval.tick(), if config.num_epochs_to_retain != u64::MAX => {
                        if let Err(err) = Self::prune_objects_for_eligible_epochs(&perpetual_db, &checkpoint_store, config) {
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
        pruning_config: AuthorityStorePruningConfig,
        epoch_duration_ms: u64,
    ) -> Self {
        AuthorityStorePruner {
            _objects_pruner_cancel_handle: Self::setup_objects_pruning(
                pruning_config,
                epoch_duration_ms,
                perpetual_db,
                checkpoint_store,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use fs_extra::dir::get_size;
    use more_asserts as ma;
    use std::path::Path;
    use std::time::Duration;
    use std::{collections::HashSet, sync::Arc};
    use tracing::log::{error, info};

    use crate::authority::authority_store_pruner::DeletionMethod;
    use crate::authority::authority_store_tables::AuthorityPerpetualTables;
    use crate::authority::authority_store_types::{StoreData, StoreObject, StoreObjectPair};
    #[cfg(not(target_env = "msvc"))]
    use pprof::Symbol;
    use sui_types::base_types::{ObjectDigest, VersionNumber};
    use sui_types::messages::{TransactionEffects, TransactionEffectsAPI};
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
        let objects = DBMap::<ObjectKey, StoreObject>::reopen(
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
        objects: &DBMap<ObjectKey, StoreObject>,
    ) -> Result<TransactionEffects, anyhow::Error> {
        let mut to_delete = vec![];
        let num_versions_to_keep = 2;
        let total_unique_object_ids = 100_000;
        let num_versions_per_object = 10;
        let ids = ObjectID::in_range(ObjectID::ZERO, total_unique_object_ids)?;
        for id in ids {
            for i in (0..num_versions_per_object).rev() {
                let StoreObjectPair(obj, _) = Object::immutable_with_id_for_testing(id).into();
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
        objects: &DBMap<ObjectKey, StoreObject>,
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
                    Object::immutable_with_id_for_testing(id).into();
                batch = batch.insert_batch(
                    &db.objects,
                    [(ObjectKey(id, SequenceNumber::from(i)), obj.clone())],
                )?;
                if let StoreData::IndirectObject(metadata) = obj.data {
                    batch = batch.merge_batch(
                        &db.indirect_move_objects,
                        [(metadata.digest, indirect_obj.unwrap())],
                    )?;
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

    async fn run_pruner(
        path: &Path,
        num_versions_per_object: u64,
        num_object_versions_to_retain: u64,
        total_unique_object_ids: u32,
        deletion_method: DeletionMethod,
    ) -> Vec<ObjectKey> {
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
            AuthorityStorePruner::prune_effects(vec![effects], &db, deletion_method).unwrap();
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
        let total_unique_object_ids = 100_000;
        let num_versions_per_object = 10;
        let ids = ObjectID::in_range(ObjectID::ZERO, total_unique_object_ids)?;
        let mut to_delete = vec![];
        for id in ids {
            for i in (0..num_versions_per_object).rev() {
                if i < num_versions_per_object - 2 {
                    to_delete.push((id, SequenceNumber::from(i)));
                }
                let StoreObjectPair(obj, _) = Object::immutable_with_id_for_testing(id).into();
                perpetual_db
                    .objects
                    .insert(&ObjectKey(id, SequenceNumber::from(i)), &obj)?;
            }
        }
        perpetual_db.objects.rocksdb.flush()?;
        let before_compaction_size = get_size(primary_path.clone()).unwrap();

        let mut effects = TransactionEffects::default();
        *effects.modified_at_versions_mut_for_testing() = to_delete;
        let total_pruned = AuthorityStorePruner::prune_effects(
            vec![effects],
            &perpetual_db,
            DeletionMethod::RangeDelete,
        );
        info!("Total pruned keys = {:?}", total_pruned);
        let start = ObjectKey(ObjectID::ZERO, SequenceNumber::MIN);
        let end = ObjectKey(ObjectID::MAX, SequenceNumber::MAX);
        perpetual_db.objects.compact_range(&start, &end)?;

        let after_compaction_size = get_size(primary_path).unwrap();

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
        let primary_path = tempfile::tempdir()?.into_path();
        let perpetual_db = Arc::new(AuthorityPerpetualTables::open(&primary_path, None));
        let effects = insert_keys(&perpetual_db.objects)?;
        AuthorityStorePruner::prune_effects(
            vec![effects],
            &perpetual_db,
            DeletionMethod::RangeDelete,
        )?;
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
        AuthorityStorePruner::prune_effects(
            vec![effects],
            &perpetual_db,
            DeletionMethod::RangeDelete,
        )?;
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
