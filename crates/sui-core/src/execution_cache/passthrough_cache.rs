// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::authority_store::{ExecutionLockWriteGuard, SuiLockResult};
use crate::authority::authority_store_pruner::{
    AuthorityStorePruner, AuthorityStorePruningMetrics,
};
use crate::authority::epoch_start_configuration::EpochFlag;
use crate::authority::epoch_start_configuration::EpochStartConfiguration;
use crate::authority::AuthorityStore;
use crate::checkpoints::CheckpointStore;
use crate::state_accumulator::AccumulatorStore;
use crate::transaction_outputs::TransactionOutputs;

use either::Either;
use futures::{
    future::{join_all, BoxFuture},
    FutureExt,
};
use mysten_common::sync::notify_read::NotifyRead;
use prometheus::Registry;
use std::path::Path;
use std::sync::Arc;
use sui_config::node::AuthorityStorePruningConfig;
use sui_protocol_config::ProtocolVersion;
use sui_storage::package_object_cache::PackageObjectCache;
use sui_types::accumulator::Accumulator;
use sui_types::base_types::VerifiedExecutionData;
use sui_types::base_types::{EpochId, ObjectID, ObjectRef, SequenceNumber};
use sui_types::digests::{TransactionDigest, TransactionEffectsDigest, TransactionEventsDigest};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::error::{SuiError, SuiResult};
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;
use sui_types::storage::{MarkerValue, ObjectKey, ObjectOrTombstone, ObjectStore, PackageObject};
use sui_types::sui_system_state::{get_sui_system_state, SuiSystemState};
use sui_types::transaction::VerifiedTransaction;
use tap::TapFallible;
use tracing::{error, instrument};
use typed_store::Map;

use super::{
    CheckpointCache, ExecutionCacheMetrics, ExecutionCacheRead, ExecutionCacheReconfigAPI,
    ExecutionCacheWrite, NotifyReadWrapper, StateSyncAPI,
};

pub struct PassthroughCache {
    store: Arc<AuthorityStore>,
    metrics: Option<ExecutionCacheMetrics>,
    package_cache: Arc<PackageObjectCache>,
    executed_effects_digests_notify_read: NotifyRead<TransactionDigest, TransactionEffectsDigest>,
}

impl PassthroughCache {
    pub fn new(store: Arc<AuthorityStore>, registry: &Registry) -> Self {
        Self {
            store,
            metrics: Some(ExecutionCacheMetrics::new(registry)),
            package_cache: PackageObjectCache::new(),
            executed_effects_digests_notify_read: NotifyRead::new(),
        }
    }

    pub fn new_with_no_metrics(store: Arc<AuthorityStore>) -> Self {
        Self {
            store,
            metrics: None,
            package_cache: PackageObjectCache::new(),
            executed_effects_digests_notify_read: NotifyRead::new(),
        }
    }

    pub fn as_notify_read_wrapper(self: Arc<Self>) -> NotifyReadWrapper<Self> {
        NotifyReadWrapper(self)
    }

    pub fn store_for_testing(&self) -> &Arc<AuthorityStore> {
        &self.store
    }

    pub async fn prune_objects_and_compact_for_testing(
        &self,
        checkpoint_store: &Arc<CheckpointStore>,
    ) {
        let pruning_config = AuthorityStorePruningConfig {
            num_epochs_to_retain: 0,
            ..Default::default()
        };
        let _ = AuthorityStorePruner::prune_objects_for_eligible_epochs(
            &self.store.perpetual_tables,
            checkpoint_store,
            &self.store.objects_lock_table,
            pruning_config,
            AuthorityStorePruningMetrics::new_for_test(),
            usize::MAX,
        )
        .await;
        let _ = AuthorityStorePruner::compact(&self.store.perpetual_tables);
    }

    /// This is a temporary method to be used when we enable simplified_unwrap_then_delete.
    /// It re-accumulates state hash for the new epoch if simplified_unwrap_then_delete is enabled.
    #[instrument(level = "error", skip_all)]
    pub fn maybe_reaccumulate_state_hash(
        &self,
        cur_epoch_store: &AuthorityPerEpochStore,
        new_protocol_version: ProtocolVersion,
    ) {
        self.store
            .maybe_reaccumulate_state_hash(cur_epoch_store, new_protocol_version);
    }
}

impl ExecutionCacheRead for PassthroughCache {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        self.package_cache
            .get_package_object(package_id, &*self.store)
    }

    fn force_reload_system_packages(&self, system_package_ids: &[ObjectID]) {
        self.package_cache
            .force_reload_system_packages(system_package_ids.iter().cloned(), self);
    }

    fn get_object(&self, id: &ObjectID) -> SuiResult<Option<Object>> {
        self.store.get_object(id).map_err(Into::into)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(self.store.get_object_by_key(object_id, version)?)
    }

    fn multi_get_objects_by_key(
        &self,
        object_keys: &[ObjectKey],
    ) -> Result<Vec<Option<Object>>, SuiError> {
        Ok(self.store.multi_get_objects_by_key(object_keys)?)
    }

    fn object_exists_by_key(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<bool> {
        self.store.object_exists_by_key(object_id, version)
    }

    fn multi_object_exists_by_key(&self, object_keys: &[ObjectKey]) -> SuiResult<Vec<bool>> {
        self.store.multi_object_exists_by_key(object_keys)
    }

    fn get_latest_object_ref_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> SuiResult<Option<ObjectRef>> {
        self.store.get_latest_object_ref_or_tombstone(object_id)
    }

    fn get_latest_object_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<(ObjectKey, ObjectOrTombstone)>, SuiError> {
        self.store.get_latest_object_or_tombstone(object_id)
    }

    fn find_object_lt_or_eq_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        self.store.find_object_lt_or_eq_version(object_id, version)
    }

    fn get_lock(&self, obj_ref: ObjectRef, epoch_id: EpochId) -> SuiLockResult {
        self.store.get_lock(obj_ref, epoch_id)
    }

    fn get_latest_lock_for_object_id(&self, object_id: ObjectID) -> SuiResult<ObjectRef> {
        self.store.get_latest_lock_for_object_id(object_id)
    }

    fn check_owned_object_locks_exist(&self, owned_object_refs: &[ObjectRef]) -> SuiResult {
        self.store.check_owned_object_locks_exist(owned_object_refs)
    }

    fn multi_get_transaction_blocks(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<Arc<VerifiedTransaction>>>> {
        Ok(self
            .store
            .multi_get_transaction_blocks(digests)?
            .into_iter()
            .map(|o| o.map(Arc::new))
            .collect())
    }

    fn multi_get_executed_effects_digests(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffectsDigest>>> {
        self.store.multi_get_executed_effects_digests(digests)
    }

    fn multi_get_effects(
        &self,
        digests: &[TransactionEffectsDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        Ok(self.store.perpetual_tables.effects.multi_get(digests)?)
    }

    fn notify_read_executed_effects_digests<'a>(
        &'a self,
        digests: &'a [TransactionDigest],
    ) -> BoxFuture<'a, SuiResult<Vec<TransactionEffectsDigest>>> {
        async move {
            let registrations = self
                .executed_effects_digests_notify_read
                .register_all(digests);

            let executed_effects_digests = self.multi_get_executed_effects_digests(digests)?;

            let results = executed_effects_digests
                .into_iter()
                .zip(registrations)
                .map(|(a, r)| match a {
                    // Note that Some() clause also drops registration that is already fulfilled
                    Some(ready) => Either::Left(futures::future::ready(ready)),
                    None => Either::Right(r),
                });

            Ok(join_all(results).await)
        }
        .boxed()
    }

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> SuiResult<Vec<Option<TransactionEvents>>> {
        self.store.multi_get_events(event_digests)
    }

    fn get_sui_system_state_object_unsafe(&self) -> SuiResult<SuiSystemState> {
        get_sui_system_state(self)
    }

    fn get_marker_value(
        &self,
        object_id: &ObjectID,
        version: &SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<Option<MarkerValue>> {
        self.store.get_marker_value(object_id, version, epoch_id)
    }

    fn get_latest_marker(
        &self,
        object_id: &ObjectID,
        epoch_id: EpochId,
    ) -> SuiResult<Option<(SequenceNumber, MarkerValue)>> {
        self.store.get_latest_marker(object_id, epoch_id)
    }
}

impl CheckpointCache for PassthroughCache {
    fn deprecated_get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<Option<(EpochId, CheckpointSequenceNumber)>> {
        self.store.deprecated_get_transaction_checkpoint(digest)
    }

    fn deprecated_multi_get_transaction_checkpoint(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<(EpochId, CheckpointSequenceNumber)>>> {
        self.store
            .deprecated_multi_get_transaction_checkpoint(digests)
    }

    fn deprecated_insert_finalized_transactions(
        &self,
        digests: &[TransactionDigest],
        epoch: EpochId,
        sequence: CheckpointSequenceNumber,
    ) -> SuiResult {
        self.store
            .deprecated_insert_finalized_transactions(digests, epoch, sequence)
    }
}

impl ExecutionCacheWrite for PassthroughCache {
    #[instrument(level = "debug", skip_all)]
    fn write_transaction_outputs<'a>(
        &'a self,
        epoch_id: EpochId,
        tx_outputs: TransactionOutputs,
    ) -> BoxFuture<'a, SuiResult> {
        async move {
            let tx_digest = *tx_outputs.transaction.digest();
            let effects_digest = tx_outputs.effects.digest();
            self.store
                .write_transaction_outputs(epoch_id, tx_outputs)
                .await?;

            self.executed_effects_digests_notify_read
                .notify(&tx_digest, &effects_digest);

            if let Some(metrics) = &self.metrics {
                metrics
                    .pending_notify_read
                    .set(self.executed_effects_digests_notify_read.num_pending() as i64);
            }

            Ok(())
        }
        .boxed()
    }

    fn acquire_transaction_locks<'a>(
        &'a self,
        epoch_id: EpochId,
        owned_input_objects: &'a [ObjectRef],
        tx_digest: TransactionDigest,
    ) -> BoxFuture<'a, SuiResult> {
        self.store
            .acquire_transaction_locks(epoch_id, owned_input_objects, tx_digest)
            .boxed()
    }
}

impl ExecutionCacheReconfigAPI for PassthroughCache {
    fn insert_genesis_object(&self, object: Object) -> SuiResult {
        self.store.insert_genesis_object(object)
    }

    fn bulk_insert_genesis_objects(&self, objects: &[Object]) -> SuiResult {
        self.store.bulk_insert_genesis_objects(objects)
    }

    fn revert_state_update(&self, digest: &TransactionDigest) -> SuiResult {
        self.store.revert_state_update(digest)
    }

    fn set_epoch_start_configuration(
        &self,
        epoch_start_config: &EpochStartConfiguration,
    ) -> SuiResult {
        self.store.set_epoch_start_configuration(epoch_start_config)
    }

    fn update_epoch_flags_metrics(&self, old: &[EpochFlag], new: &[EpochFlag]) {
        self.store.update_epoch_flags_metrics(old, new)
    }

    fn clear_object_per_epoch_marker_table(&self, execution_guard: &ExecutionLockWriteGuard<'_>) {
        self.store
            .clear_object_per_epoch_marker_table(execution_guard)
            .tap_err(|e| {
                error!(?e, "Failed to clear object per-epoch marker table");
            })
            .ok();
    }

    fn expensive_check_sui_conservation(
        &self,
        old_epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult {
        self.store
            .expensive_check_sui_conservation(self, old_epoch_store)
    }

    fn checkpoint_db(&self, path: &Path) -> SuiResult {
        self.store.perpetual_tables.checkpoint_db(path)
    }
}

impl StateSyncAPI for PassthroughCache {
    fn insert_transaction_and_effects(
        &self,
        transaction: &VerifiedTransaction,
        transaction_effects: &TransactionEffects,
    ) -> SuiResult {
        Ok(self
            .store
            .insert_transaction_and_effects(transaction, transaction_effects)?)
    }

    fn multi_insert_transaction_and_effects(
        &self,
        transactions_and_effects: &[VerifiedExecutionData],
    ) -> SuiResult {
        Ok(self
            .store
            .multi_insert_transaction_and_effects(transactions_and_effects.iter())?)
    }
}

impl AccumulatorStore for PassthroughCache {
    fn get_object_ref_prior_to_key_deprecated(
        &self,
        object_id: &ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> SuiResult<Option<ObjectRef>> {
        self.store
            .get_object_ref_prior_to_key_deprecated(object_id, version)
    }

    fn get_root_state_accumulator_for_epoch(
        &self,
        epoch: EpochId,
    ) -> SuiResult<Option<(CheckpointSequenceNumber, Accumulator)>> {
        self.store.get_root_state_accumulator_for_epoch(epoch)
    }

    fn get_root_state_accumulator_for_highest_epoch(
        &self,
    ) -> SuiResult<Option<(EpochId, (CheckpointSequenceNumber, Accumulator))>> {
        self.store.get_root_state_accumulator_for_highest_epoch()
    }

    fn insert_state_accumulator_for_epoch(
        &self,
        epoch: EpochId,
        checkpoint_seq_num: &CheckpointSequenceNumber,
        acc: &Accumulator,
    ) -> SuiResult {
        self.store
            .insert_state_accumulator_for_epoch(epoch, checkpoint_seq_num, acc)
    }

    fn iter_live_object_set(
        &self,
        include_wrapped_tombstone: bool,
    ) -> Box<dyn Iterator<Item = crate::authority::authority_store_tables::LiveObject> + '_> {
        self.store.iter_live_object_set(include_wrapped_tombstone)
    }
}
