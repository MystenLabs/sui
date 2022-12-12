// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cmp::Ordering, sync::Arc, time::Duration};
use sui_config::node::AuthorityStorePruningConfig;
use sui_types::{
    base_types::{ObjectID, SequenceNumber, VersionNumber},
    storage::ObjectKey,
};
use tokio::{
    sync::oneshot::{self, Sender},
    time::{self, Instant},
};
use tracing::log::{error, info};
use typed_store::Map;

use super::authority_store_tables::AuthorityPerpetualTables;

const MAX_OPS_IN_ONE_WRITE_BATCH: u64 = 10000;

pub struct AuthorityStorePruner {
    _objects_pruner_cancel_handle: oneshot::Sender<()>,
}

impl AuthorityStorePruner {
    fn prune_objects(
        num_versions_to_retain: u64,
        perpetual_db: Arc<AuthorityPerpetualTables>,
    ) -> u64 {
        let iter = perpetual_db.objects.iter().skip_to_last().reverse();
        let mut total_keys_scanned = 0;
        let mut total_objects_scanned = 0;
        let mut num_versions_for_key = 0;
        let mut object_id: Option<ObjectID> = None;
        let mut seq_num: Option<VersionNumber> = None;
        let mut num_pending_wb_ops = 0;
        let mut total_pruned: u64 = 0;
        let mut wb = perpetual_db.objects.batch();
        for (key, _value) in iter {
            total_keys_scanned += 1;
            if let Some(obj_id) = object_id {
                if obj_id != key.0 {
                    total_objects_scanned += 1;
                    if let Some(seq) = seq_num {
                        // delete keys in range [(obj_id, VersionNumber::ZERO), (obj_id, seq))
                        let start = ObjectKey(obj_id, SequenceNumber::from_u64(0));
                        let end = ObjectKey(obj_id, seq);
                        if let Ok(new_wb) = wb.delete_range(&perpetual_db.objects, &start, &end) {
                            wb = new_wb;
                            num_pending_wb_ops += 1;
                        } else {
                            error!("Failed to invoke delete_range on write batch while compacting objects");
                            wb = perpetual_db.objects.batch();
                            num_pending_wb_ops = 0;
                            break;
                        }
                        if num_pending_wb_ops >= MAX_OPS_IN_ONE_WRITE_BATCH {
                            if wb.write().is_err() {
                                error!("Failed to commit write batch while compacting objects");
                                wb = perpetual_db.objects.batch();
                                num_pending_wb_ops = 0;
                                break;
                            } else {
                                info!("Committed write batch while compacting objects, keys scanned = {:?}, objects scanned = {:?}", total_keys_scanned, total_objects_scanned);
                            }
                            wb = perpetual_db.objects.batch();
                            num_pending_wb_ops = 0;
                        }
                    }
                    num_versions_for_key = 0;
                    object_id = Some(key.0);
                    seq_num = None;
                }
                num_versions_for_key += 1;
                // We'll keep maximum `num_versions_to_retain` latest version of any object
                match num_versions_for_key.cmp(&num_versions_to_retain) {
                    Ordering::Equal => {
                        seq_num = Some(key.1);
                    }
                    Ordering::Greater => {
                        total_pruned += 1;
                    }
                    Ordering::Less => {}
                }
            } else {
                num_versions_for_key = 1;
                total_objects_scanned = 1;
                object_id = Some(key.0);
            }
        }
        if let Some(obj_id) = object_id {
            if let Some(seq) = seq_num {
                // delete keys in range [(obj_id, VersionNumber::ZERO), (obj_id, seq))
                let start = ObjectKey(obj_id, SequenceNumber::from_u64(0));
                let end = ObjectKey(obj_id, seq);
                if let Ok(new_wb) = wb.delete_range(&perpetual_db.objects, &start, &end) {
                    wb = new_wb;
                    num_pending_wb_ops += 1;
                } else {
                    error!("Failed to invoke delete_range on write batch while compacting objects");
                    wb = perpetual_db.objects.batch();
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
        let mut prune_interval =
            tokio::time::interval_at(Instant::now() + pruning_initial_delay, pruning_timeperiod);
        prune_interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    _ = prune_interval.tick() => {
                        info!("Starting pruning of objects table");
                        let perpetual_db = perpetual_db.clone();
                        let num_pruned = Self::prune_objects(num_versions_to_retain, perpetual_db);
                        info!("Finished pruning with total object versions pruned = {}", num_pruned);
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
    use std::{collections::HashSet, sync::Arc};

    use crate::authority::authority_store_tables::AuthorityPerpetualTables;
    use fs_extra::dir::get_size;
    use more_asserts as ma;
    use sui_types::{
        base_types::{ObjectID, SequenceNumber},
        object::Object,
        storage::ObjectKey,
    };
    use tracing::log::info;
    use typed_store::Map;

    use super::AuthorityStorePruner;

    async fn test_pruning(
        perpetual_db: Arc<AuthorityPerpetualTables>,
        num_versions_per_object: u64,
        num_object_versions_to_retain: u64,
        total_unique_object_ids: u32,
    ) -> Result<u64, anyhow::Error> {
        // this contains the set of keys that should not have been pruned
        let mut expected = HashSet::new();
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
        let total_pruned = AuthorityStorePruner::prune_objects(
            num_object_versions_to_retain,
            perpetual_db.clone(),
        );
        let mut after_pruning = HashSet::new();
        let iter = perpetual_db.objects.iter();
        for (k, _v) in iter {
            after_pruning.insert(k);
        }
        assert_eq!(expected, after_pruning);
        Ok(total_pruned)
    }

    #[tokio::test]
    async fn test_rights_versions_are_pruned() -> Result<(), anyhow::Error> {
        {
            let primary_path = tempfile::tempdir()?.into_path();
            let perpetual_db = Arc::new(AuthorityPerpetualTables::open(&primary_path, None));
            // add 3 versions for every object id and 1000 unique object ids
            let total_pruned = test_pruning(
                perpetual_db,
                /* num_versions_per_object */ 3,
                /* num_object_versions_to_retain */ 2,
                /* total_unique_object_ids */ 1000,
            )
            .await;
            // We had 3 versions for 1000 unique object ids, we wanted to retain 2 version per object id i.e. we should have pruned 1000 keys
            assert_eq!(1000, total_pruned.unwrap());
        }
        {
            let primary_path = tempfile::tempdir()?.into_path();
            let perpetual_db = Arc::new(AuthorityPerpetualTables::open(&primary_path, None));
            // add 3 versions for every object id and 1000 unique object ids
            let total_pruned = test_pruning(
                perpetual_db,
                /* num_versions_per_object */ 2,
                /* num_object_versions_to_retain */ 2,
                /* total_unique_object_ids */ 1000,
            )
            .await;
            // We had only 2 version per key, we did not prune anything
            assert_eq!(0, total_pruned.unwrap());
        }
        {
            let primary_path = tempfile::tempdir()?.into_path();
            let perpetual_db = Arc::new(AuthorityPerpetualTables::open(&primary_path, None));
            // add 3 versions for every object id and 1000 unique object ids
            let total_pruned = test_pruning(
                perpetual_db,
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

        let total_pruned = AuthorityStorePruner::prune_objects(2, perpetual_db.clone());
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
}
