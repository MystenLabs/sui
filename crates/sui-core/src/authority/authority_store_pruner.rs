// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoints::CheckpointStore;
use mysten_metrics::monitored_scope;
use std::cmp::max;
use std::collections::HashMap;
use std::{sync::Arc, time::Duration};
use sui_config::node::AuthorityStorePruningConfig;
use sui_types::base_types::SequenceNumber;
use sui_types::digests::CheckpointDigest;
use sui_types::messages::TransactionEffects;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use tokio::sync::oneshot::{self, Sender};
use tracing::debug;
use tracing::log::error;
use typed_store::rocks::DBMap;
use typed_store::Map;

use super::authority_store_tables::AuthorityPerpetualTables;

const MAX_TRANSACTIONS_IN_ONE_BATCH: usize = 1000;
const MAX_CHECKPOINTS_IN_ONE_BATCH: usize = 200;

pub struct AuthorityStorePruner {
    _objects_pruner_cancel_handle: oneshot::Sender<()>,
}

impl AuthorityStorePruner {
    fn handle_checkpoint(
        checkpoint_effects: impl IntoIterator<Item = TransactionEffects>,
        objects: &DBMap<ObjectKey, Object>,
    ) -> anyhow::Result<usize> {
        let _scope = monitored_scope("ObjectsLivePruner");
        let mut pruned = 0;
        let mut wb = objects.batch();
        let mut updates = HashMap::new();

        for effects in checkpoint_effects {
            for (object_id, seq_number) in effects.modified_at_versions {
                updates
                    .entry(object_id)
                    .and_modify(|version| *version = max(*version, seq_number))
                    .or_insert(seq_number);
            }
        }
        for (object_id, version) in updates {
            let object_key = ObjectKey(object_id, version);
            let iter = objects.iter().skip_prior_to(&object_key)?.reverse();
            let mut start_range = object_key;
            let end_range = ObjectKey(object_key.0, SequenceNumber::from(object_key.1.value() + 1));
            for (key, _) in iter.take_while(|(key, _)| key.0 == object_key.0) {
                start_range = key;
                pruned += 1;
            }
            wb = wb.delete_range(objects, &start_range, &end_range)?;
        }
        wb.write()?;
        Ok(pruned)
    }

    fn prune_objects(
        perpetual_db: &Arc<AuthorityPerpetualTables>,
        checkpoint_store: &Arc<CheckpointStore>,
        num_epochs_to_retain: u64,
    ) -> anyhow::Result<()> {
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
        loop {
            let Some(checkpoint) = checkpoint_store.get_checkpoint_by_sequence_number(checkpoint_number + 1)? else {break;};
            // checkpoint's epoch is too new. Skipping for now
            if current_epoch < checkpoint.epoch() + num_epochs_to_retain {
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

            if batch_effects.len() >= MAX_TRANSACTIONS_IN_ONE_BATCH
                || checkpoints_in_batch >= MAX_CHECKPOINTS_IN_ONE_BATCH
            {
                Self::handle_checkpoint(batch_effects, &perpetual_db.objects)?;
                checkpoint_store
                    .update_highest_pruned_checkpoint(checkpoint_number, checkpoint_digest)?;
                batch_effects = vec![];
                checkpoints_in_batch = 0;
            }
        }
        if !batch_effects.is_empty() {
            Self::handle_checkpoint(batch_effects, &perpetual_db.objects)?;
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
        num_epochs_to_retain: u64,
        epoch_duration_ms: u64,
        perpetual_db: Arc<AuthorityPerpetualTables>,
        checkpoint_store: Arc<CheckpointStore>,
    ) -> Sender<()> {
        let (sender, mut recv) = tokio::sync::oneshot::channel();
        debug!("Starting object pruning service with num_epochs_to_retain={num_epochs_to_retain}");
        let duration_ms = if num_epochs_to_retain > 0 {
            epoch_duration_ms / 2
        } else {
            1000
        };
        let mut prune_interval = tokio::time::interval(Duration::from_millis(duration_ms));

        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    _ = prune_interval.tick(), if num_epochs_to_retain != u64::MAX => {
                        if let Err(err) = Self::prune_objects(&perpetual_db, &checkpoint_store, num_epochs_to_retain) {
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
        pruning_config: &AuthorityStorePruningConfig,
        epoch_duration_ms: u64,
    ) -> Self {
        AuthorityStorePruner {
            _objects_pruner_cancel_handle: Self::setup_objects_pruning(
                pruning_config.num_epochs_to_retain,
                epoch_duration_ms,
                perpetual_db,
                checkpoint_store,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::Duration;
    use std::{collections::HashSet, sync::Arc};

    use crate::authority::authority_store_tables::AuthorityPerpetualTables;
    use sui_types::messages::TransactionEffects;
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

    fn generate_test_data(
        db: Arc<AuthorityPerpetualTables>,
        num_versions_per_object: u64,
        num_object_versions_to_retain: u64,
        total_unique_object_ids: u32,
    ) -> Result<(Vec<ObjectKey>, Vec<ObjectKey>), anyhow::Error> {
        let (mut to_keep, mut to_delete) = (vec![], vec![]);

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
                db.objects.insert(
                    &ObjectKey(id, SequenceNumber::from(i)),
                    &Object::immutable_with_id_for_testing(id),
                )?;
            }
        }
        assert_eq!(
            to_keep.len() as u64,
            std::cmp::min(num_object_versions_to_retain, num_versions_per_object)
                * total_unique_object_ids as u64
        );
        Ok((to_keep, to_delete))
    }

    #[tokio::test]
    async fn test_pruning() {
        let path = tempfile::tempdir().unwrap().into_path();

        let to_keep = {
            let db = Arc::new(AuthorityPerpetualTables::open(&path, None));
            let (to_keep, to_delete) = generate_test_data(db.clone(), 3, 2, 1000).unwrap();
            let effects = TransactionEffects {
                modified_at_versions: to_delete.into_iter().map(|o| (o.0, o.1)).collect(),
                ..Default::default()
            };
            let pruned =
                AuthorityStorePruner::handle_checkpoint(vec![effects], &db.objects).unwrap();
            assert_eq!(pruned, 1000);
            to_keep
        };

        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_eq!(
            HashSet::from_iter(to_keep),
            get_keys_after_pruning(path).unwrap()
        );
    }
}
