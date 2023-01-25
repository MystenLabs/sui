// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};
use sui_config::node::AuthorityStorePruningConfig;
use sui_types::object::Object;
use sui_types::{
    base_types::{ObjectID, VersionNumber},
    storage::ObjectKey,
};
use tokio::{
    sync::oneshot::{self, Sender},
    time::{self, Instant},
};
use tracing::log::{error, info};
use typed_store::rocks::DBMap;
use typed_store::Map;

use super::authority_store_tables::AuthorityPerpetualTables;

const MAX_OPS_IN_ONE_WRITE_BATCH: u64 = 10000;

pub struct AuthorityStorePruner {
    _objects_pruner_cancel_handle: oneshot::Sender<()>,
}

impl AuthorityStorePruner {
    fn prune_objects(num_versions_to_retain: u64, objects: &DBMap<ObjectKey, Object>) -> u64 {
        let iter = objects.iter().skip_to_last().reverse();
        let mut total_keys_scanned = 0;
        let mut total_objects_scanned = 0;
        let mut num_versions_for_key = 0;
        let mut object_id: Option<ObjectID> = None;
        let mut delete_range_start: Option<VersionNumber> = None;
        let mut delete_range_end: Option<VersionNumber> = None;
        let mut num_pending_wb_ops = 0;
        let mut total_pruned: u64 = 0;
        let mut wb = objects.batch();
        for (key, _value) in iter {
            total_keys_scanned += 1;
            if let Some(obj_id) = object_id {
                if obj_id != key.0 {
                    total_objects_scanned += 1;
                    if let (Some(start_seq_num), Some(end_seq_num)) =
                        (delete_range_start, delete_range_end)
                    {
                        let start = ObjectKey(obj_id, start_seq_num);
                        let end = ObjectKey(obj_id, end_seq_num);
                        if let Ok(new_wb) = wb.delete_range(objects, &start, &end) {
                            wb = new_wb;
                            num_pending_wb_ops += 1;
                        } else {
                            error!("Failed to invoke delete_range on write batch while compacting objects");
                            wb = objects.batch();
                            num_pending_wb_ops = 0;
                            break;
                        }
                        if num_pending_wb_ops >= MAX_OPS_IN_ONE_WRITE_BATCH {
                            if wb.write().is_err() {
                                error!("Failed to commit write batch while compacting objects");
                                wb = objects.batch();
                                num_pending_wb_ops = 0;
                                break;
                            } else {
                                info!("Committed write batch while compacting objects, keys scanned = {:?}, objects scanned = {:?}", total_keys_scanned, total_objects_scanned);
                            }
                            wb = objects.batch();
                            num_pending_wb_ops = 0;
                        }
                    }
                    num_versions_for_key = 0;
                    object_id = Some(key.0);
                    delete_range_end = None;
                }
                num_versions_for_key += 1;
                // We'll keep maximum `num_versions_to_retain` latest version of any object
                delete_range_start = match delete_range_end {
                    Some(_end) => {
                        total_pruned += 1;
                        assert!(num_versions_for_key > num_versions_to_retain);
                        Some(key.1)
                    }
                    None => {
                        if num_versions_for_key == num_versions_to_retain {
                            delete_range_end = Some(key.1)
                        }
                        None
                    }
                };
            } else {
                num_versions_for_key = 1;
                total_objects_scanned = 1;
                object_id = Some(key.0);
            }
        }
        if let Some(obj_id) = object_id {
            if let (Some(start_seq_num), Some(end_seq_num)) = (delete_range_start, delete_range_end)
            {
                let start = ObjectKey(obj_id, start_seq_num);
                let end = ObjectKey(obj_id, end_seq_num);
                if let Ok(new_wb) = wb.delete_range(objects, &start, &end) {
                    wb = new_wb;
                    num_pending_wb_ops += 1;
                } else {
                    error!("Failed to invoke delete_range on write batch while compacting objects");
                    wb = objects.batch();
                    num_pending_wb_ops = 0;
                }
            }
        }
        if num_pending_wb_ops > 0 {
            if wb.write().is_err() {
                error!("Failed to commit write batch while compacting objects");
            } else {
                info!("Committed write batch while compacting objects, keys scanned = {:?}, objects scanned = {:?}", total_keys_scanned, total_objects_scanned);
            }
        }
        info!(
            "Finished compacting objects, keys scanned = {:?}, objects scanned = {:?}",
            total_keys_scanned, total_objects_scanned
        );
        total_pruned
    }

    fn setup_objects_pruning(
        num_versions_to_retain: u64,
        pruning_timeperiod: Duration,
        pruning_initial_delay: Duration,
        perpetual_db: Arc<AuthorityPerpetualTables>,
    ) -> Sender<()> {
        let (sender, mut recv) = tokio::sync::oneshot::channel();
        if num_versions_to_retain == u64::MAX {
            info!("Skipping pruning of objects table as we want to retain all versions");
            return sender;
        }
        info!(
            "Starting object pruning service with num_versions_to_retain={num_versions_to_retain}"
        );
        let mut prune_interval =
            tokio::time::interval_at(Instant::now() + pruning_initial_delay, pruning_timeperiod);
        prune_interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    _ = prune_interval.tick() => {
                        info!("Starting pruning of objects table");
                        let num_pruned = Self::prune_objects(num_versions_to_retain, &perpetual_db.objects);
                        info!("Finished pruning with total object versions pruned = {}", num_pruned);
                        if let Ok(()) = perpetual_db.objects.flush() {
                            info!("Completed flushing objects table");
                        } else {
                            error!("Failed to flush objects table");
                        }
                    }
                    _ = &mut recv => break,
                }
            }
        });
        sender
    }
    pub fn new(
        perpetual_db: Arc<AuthorityPerpetualTables>,
        pruning_config: &AuthorityStorePruningConfig,
    ) -> Self {
        AuthorityStorePruner {
            _objects_pruner_cancel_handle: Self::setup_objects_pruning(
                pruning_config.objects_num_latest_versions_to_retain,
                Duration::from_secs(pruning_config.objects_pruning_period_secs),
                Duration::from_secs(pruning_config.objects_pruning_initial_delay_secs),
                perpetual_db,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use fs_extra::dir::get_size;
    use more_asserts as ma;
    use std::path::{Path, PathBuf};
    use std::time::Duration;
    use std::{collections::HashSet, sync::Arc};
    use tracing::log::{error, info};

    use crate::authority::authority_store_tables::AuthorityPerpetualTables;
    #[cfg(not(target_env = "msvc"))]
    use pprof::Symbol;
    use sui_types::base_types::VersionNumber;
    use sui_types::{
        base_types::{ObjectID, SequenceNumber},
        object::Object,
        storage::ObjectKey,
    };
    use typed_store::rocks::{DBMap, MetricConf, ReadWriteOptions};
    use typed_store::Map;

    use super::AuthorityStorePruner;

    fn get_keys_after_pruning(db_path: PathBuf) -> anyhow::Result<HashSet<ObjectKey>> {
        let perpetual_db_path = db_path.join(Path::new("perpetual"));
        let cf_names = AuthorityPerpetualTables::describe_tables();
        let cfs: Vec<&str> = cf_names.keys().map(|x| x.as_str()).collect();
        let perpetual_db =
            typed_store::rocks::open_cf(perpetual_db_path, None, MetricConf::default(), &cfs);

        let mut after_pruning = HashSet::new();
        let objects = DBMap::<ObjectKey, Object>::reopen(
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

    fn insert_keys(objects: &DBMap<ObjectKey, Object>) -> Result<(), anyhow::Error> {
        let total_unique_object_ids = 100_000;
        let num_versions_per_object = 10;
        let ids = ObjectID::in_range(ObjectID::ZERO, total_unique_object_ids)?;
        for id in ids {
            for i in (0..num_versions_per_object).rev() {
                objects.insert(
                    &ObjectKey(id, SequenceNumber::from(i)),
                    &Object::immutable_with_id_for_testing(id),
                )?;
            }
        }
        Ok(())
    }

    fn read_keys(objects: &DBMap<ObjectKey, Object>, num_reads: u32) -> Result<(), anyhow::Error> {
        let mut i = 0;
        while i < num_reads {
            let _res = objects.get(&ObjectKey(ObjectID::random(), VersionNumber::MAX))?;
            i += 1;
        }
        Ok(())
    }

    async fn test_pruning(
        primary_path: PathBuf,
        num_versions_per_object: u64,
        num_object_versions_to_retain: u64,
        total_unique_object_ids: u32,
    ) -> Result<u64, anyhow::Error> {
        let mut expected = HashSet::new();
        let total_pruned = {
            // create db
            // let primary_path = tempfile::tempdir()?.into_path();
            let perpetual_db = Arc::new(AuthorityPerpetualTables::open(&primary_path, None));
            // this contains the set of keys that should not have been pruned

            let ids = ObjectID::in_range(ObjectID::ZERO, total_unique_object_ids.into())?;
            for id in ids {
                for (counter, i) in (0..num_versions_per_object).rev().enumerate() {
                    let object_key = ObjectKey(id, SequenceNumber::from_u64(i));
                    if counter < num_object_versions_to_retain.try_into().unwrap() {
                        // latest `num_object_versions_to_retain` should not have been pruned
                        expected.insert(object_key);
                    }
                    perpetual_db.objects.insert(
                        &ObjectKey(id, SequenceNumber::from(i)),
                        &Object::immutable_with_id_for_testing(id),
                    )?;
                }
            }
            assert_eq!(
                expected.len() as u64,
                std::cmp::min(num_object_versions_to_retain, num_versions_per_object)
                    * total_unique_object_ids as u64
            );
            AuthorityStorePruner::prune_objects(
                num_object_versions_to_retain,
                &perpetual_db.objects,
            )
        };
        tokio::time::sleep(Duration::from_secs(3)).await;
        let after_pruning = get_keys_after_pruning(primary_path)?;
        assert_eq!(expected, after_pruning);
        Ok(total_pruned)
    }

    #[cfg(not(target_env = "msvc"))]
    #[tokio::test]
    async fn test_correct_object_versions_are_pruned() -> Result<(), anyhow::Error> {
        {
            // add 3 versions for every object id and 1000 unique object ids
            let total_pruned = test_pruning(
                tempfile::tempdir()?.into_path(),
                /* num_versions_per_object */ 3,
                /* num_object_versions_to_retain */ 2,
                /* total_unique_object_ids */ 1000,
            )
            .await;
            // We had 3 versions for 1000 unique object ids, we wanted to retain 2 version per object id i.e. we should have pruned 1000 keys
            assert_eq!(1000, total_pruned.unwrap());
        }
        {
            // add 3 versions for every object id and 1000 unique object ids
            let total_pruned = test_pruning(
                tempfile::tempdir()?.into_path(),
                /* num_versions_per_object */ 2,
                /* num_object_versions_to_retain */ 2,
                /* total_unique_object_ids */ 1000,
            )
            .await;
            // We had only 2 version per key, we did not prune anything
            assert_eq!(0, total_pruned.unwrap());
        }
        {
            // add 3 versions for every object id and 1000 unique object ids
            let total_pruned = test_pruning(
                tempfile::tempdir()?.into_path(),
                /* num_versions_per_object */ 1,
                /* num_object_versions_to_retain */ 2,
                /* total_unique_object_ids */ 1000,
            )
            .await;
            // We had only 1 version per key, we did not prune anything
            assert_eq!(0, total_pruned.unwrap());
        }
        Ok(())
    }

    #[cfg(not(target_env = "msvc"))]
    #[tokio::test]
    async fn test_db_size_after_compaction() -> Result<(), anyhow::Error> {
        let primary_path = tempfile::tempdir()?.into_path();
        let perpetual_db = Arc::new(AuthorityPerpetualTables::open(&primary_path, None));
        let total_unique_object_ids = 100_000;
        let num_versions_per_object = 10;
        let ids = ObjectID::in_range(ObjectID::ZERO, total_unique_object_ids)?;
        for id in ids {
            for i in (0..num_versions_per_object).rev() {
                perpetual_db.objects.insert(
                    &ObjectKey(id, SequenceNumber::from(i)),
                    &Object::immutable_with_id_for_testing(id),
                )?;
            }
        }
        perpetual_db.objects.rocksdb.flush()?;
        let before_compaction_size = get_size(primary_path.clone()).unwrap();

        let total_pruned = AuthorityStorePruner::prune_objects(2, &perpetual_db.objects);
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
    async fn ensure_tombstone_fragmentation_in_stack_frame_without_ignore_range_delete(
    ) -> Result<(), anyhow::Error> {
        // This test writes a bunch of objects to objects table, invokes pruning on it and
        // then does a bunch of get(). We open the db with `ignore_range_delete` set to false.
        // We then record a cpu profile of the `get()` calls and find a range fragmentation stack frame
        // in it.
        let primary_path = tempfile::tempdir()?.into_path();
        let perpetual_db_path = primary_path.join(Path::new("perpetual"));
        let cf_names = AuthorityPerpetualTables::describe_tables();
        let cfs: Vec<&str> = cf_names.keys().map(|x| x.as_str()).collect();
        let perpetual_db =
            typed_store::rocks::open_cf(perpetual_db_path, None, MetricConf::default(), &cfs);
        let objects = DBMap::<ObjectKey, Object>::reopen(
            &perpetual_db?,
            Some("objects"),
            &ReadWriteOptions {
                // Disable `ignore_range_delete` so we can see rocksdb trying to
                // fragment range tombstones during `get()` calls
                ignore_range_deletions: false,
            },
        )?;
        insert_keys(&objects)?;
        let _total_pruned = AuthorityStorePruner::prune_objects(2, &objects);
        let guard = pprof::ProfilerGuardBuilder::default()
            .frequency(1000)
            .build()
            .unwrap();
        read_keys(&objects, 10)?;
        if let Ok(report) = guard.report().build() {
            assert!(report.data.keys().any(|f| f
                .frames
                .iter()
                .any(|vs| is_rocksdb_range_tombstone_frame(vs))));
        }
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
        insert_keys(&perpetual_db.objects)?;
        let _total_pruned = AuthorityStorePruner::prune_objects(2, &perpetual_db.objects);
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
        insert_keys(&perpetual_db.objects)?;
        let _total_pruned = AuthorityStorePruner::prune_objects(2, &perpetual_db.objects);
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
