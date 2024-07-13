// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::authority_store::{ExecutionLockWriteGuard, SuiLockResult};
use crate::authority::epoch_start_configuration::EpochStartConfiguration;
use crate::authority::epoch_start_configuration::{EpochFlag, EpochStartConfigTrait};
use crate::authority::AuthorityStore;
use crate::state_accumulator::AccumulatorStore;
use crate::transaction_outputs::TransactionOutputs;

use futures::future::BoxFuture;
use futures::FutureExt;
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Duration;
use sui_protocol_config::ProtocolVersion;
use sui_types::accumulator::Accumulator;
use sui_types::base_types::VerifiedExecutionData;
use sui_types::base_types::{EpochId, ObjectID, ObjectRef, SequenceNumber};
use sui_types::bridge::Bridge;
use sui_types::digests::{TransactionDigest, TransactionEffectsDigest, TransactionEventsDigest};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;
use sui_types::storage::{MarkerValue, ObjectKey, ObjectOrTombstone, PackageObject};
use sui_types::sui_system_state::SuiSystemState;
use sui_types::transaction::{VerifiedSignedTransaction, VerifiedTransaction};

use super::{
    CheckpointCache, ExecutionCacheCommit, ExecutionCacheConfigType, ExecutionCacheMetrics,
    ExecutionCacheReconfigAPI, ExecutionCacheWrite, ObjectCacheRead, PassthroughCache,
    StateSyncAPI, TestingAPI, TransactionCacheRead, WritebackCache,
};

macro_rules! delegate_method {
    ($self:ident.$method:ident($($args:ident),*)) => {
        match *$self.mode.read() {
            ExecutionCacheConfigType::PassthroughCache => $self.passthrough_cache.$method($($args),*),
            ExecutionCacheConfigType::WritebackCache => $self.writeback_cache.$method($($args),*),
        }
    };
}

pub struct ProxyCache {
    // Note: both caches must be constructed at startup, rather than using ArcSwap
    // (or some similar strategy). This is because we need to proxy iter_live_object_set,
    // which requires that we borrow from a member of the cache. If we used ArcSwap,
    // we would be forced to borrow from a local variable after loading from the ArcSwap.
    //
    // Cache implementations are entirely passive, so the unused one will have no effect.
    passthrough_cache: PassthroughCache,
    writeback_cache: WritebackCache,
    mode: RwLock<ExecutionCacheConfigType>,
}

impl ProxyCache {
    pub fn new(
        epoch_start_config: &EpochStartConfiguration,
        store: Arc<AuthorityStore>,
        metrics: Arc<ExecutionCacheMetrics>,
    ) -> Self {
        let cache_type = epoch_start_config.execution_cache_type();
        tracing::info!("using cache impl {:?}", cache_type);
        let passthrough_cache = PassthroughCache::new(store.clone(), metrics.clone());
        let writeback_cache = WritebackCache::new(store.clone(), metrics.clone());

        Self {
            passthrough_cache,
            writeback_cache,
            mode: RwLock::new(cache_type),
        }
    }

    async fn reconfigure_cache_impl(&self, epoch_start_config: &EpochStartConfiguration) {
        let cache_type = epoch_start_config.execution_cache_type();
        tracing::info!("switching to cache impl {:?}", cache_type);
        if matches!(cache_type, ExecutionCacheConfigType::PassthroughCache) {
            // we may switch back to the writeback cache next epoch, at which point its caches will
            // be stale if not cleared now

            // When we call invalidate_all on Moka caches, it sets the valid after time stamp to the current
            // time. Upon retrieval, it ignores entries whose insertion time is strictly less than the valid-after
            // time. In the simulator, time remains constant for the duration of a single task poll, so it is
            // possible that entries have been inserted in the same tick, and will therefore not be invalidated
            // properly. So, this sleep is necessary for passing tests, and it also is some insurance against
            // hitting the same issue in production. (It should be more or less impossible for two consecutive
            // calls to Instant::now() to return the same value in production, but there is no harm in having a
            // short sleep here just to be sure).
            tokio::time::sleep(Duration::from_nanos(100)).await;
            self.writeback_cache.clear_caches_and_assert_empty();
        }
        *self.mode.write() = cache_type;
    }
}

impl ObjectCacheRead for ProxyCache {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        delegate_method!(self.get_package_object(package_id))
    }

    fn force_reload_system_packages(&self, system_package_ids: &[ObjectID]) {
        delegate_method!(self.force_reload_system_packages(system_package_ids))
    }

    fn get_object(&self, id: &ObjectID) -> SuiResult<Option<Object>> {
        delegate_method!(self.get_object(id))
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        delegate_method!(self.get_object_by_key(object_id, version))
    }

    fn multi_get_objects_by_key(
        &self,
        object_keys: &[ObjectKey],
    ) -> Result<Vec<Option<Object>>, SuiError> {
        delegate_method!(self.multi_get_objects_by_key(object_keys))
    }

    fn object_exists_by_key(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<bool> {
        delegate_method!(self.object_exists_by_key(object_id, version))
    }

    fn multi_object_exists_by_key(&self, object_keys: &[ObjectKey]) -> SuiResult<Vec<bool>> {
        delegate_method!(self.multi_object_exists_by_key(object_keys))
    }

    fn get_latest_object_ref_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> SuiResult<Option<ObjectRef>> {
        delegate_method!(self.get_latest_object_ref_or_tombstone(object_id))
    }

    fn get_latest_object_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<(ObjectKey, ObjectOrTombstone)>, SuiError> {
        delegate_method!(self.get_latest_object_or_tombstone(object_id))
    }

    fn find_object_lt_or_eq_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        delegate_method!(self.find_object_lt_or_eq_version(object_id, version))
    }

    fn get_lock(&self, obj_ref: ObjectRef, epoch_store: &AuthorityPerEpochStore) -> SuiLockResult {
        delegate_method!(self.get_lock(obj_ref, epoch_store))
    }

    fn _get_live_objref(&self, object_id: ObjectID) -> SuiResult<ObjectRef> {
        delegate_method!(self._get_live_objref(object_id))
    }

    fn check_owned_objects_are_live(&self, owned_object_refs: &[ObjectRef]) -> SuiResult {
        delegate_method!(self.check_owned_objects_are_live(owned_object_refs))
    }

    fn get_sui_system_state_object_unsafe(&self) -> SuiResult<SuiSystemState> {
        delegate_method!(self.get_sui_system_state_object_unsafe())
    }

    fn get_bridge_object_unsafe(&self) -> SuiResult<Bridge> {
        delegate_method!(self.get_bridge_object_unsafe())
    }

    fn get_marker_value(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<Option<MarkerValue>> {
        delegate_method!(self.get_marker_value(object_id, version, epoch_id))
    }

    fn get_latest_marker(
        &self,
        object_id: &ObjectID,
        epoch_id: EpochId,
    ) -> SuiResult<Option<(SequenceNumber, MarkerValue)>> {
        delegate_method!(self.get_latest_marker(object_id, epoch_id))
    }

    fn get_highest_pruned_checkpoint(&self) -> SuiResult<CheckpointSequenceNumber> {
        delegate_method!(self.get_highest_pruned_checkpoint())
    }
}

impl TransactionCacheRead for ProxyCache {
    fn multi_get_transaction_blocks(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<Arc<VerifiedTransaction>>>> {
        delegate_method!(self.multi_get_transaction_blocks(digests))
    }

    fn multi_get_executed_effects_digests(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffectsDigest>>> {
        delegate_method!(self.multi_get_executed_effects_digests(digests))
    }

    fn multi_get_effects(
        &self,
        digests: &[TransactionEffectsDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        delegate_method!(self.multi_get_effects(digests))
    }

    fn notify_read_executed_effects_digests<'a>(
        &'a self,
        digests: &'a [TransactionDigest],
    ) -> BoxFuture<'a, SuiResult<Vec<TransactionEffectsDigest>>> {
        delegate_method!(self.notify_read_executed_effects_digests(digests))
    }

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> SuiResult<Vec<Option<TransactionEvents>>> {
        delegate_method!(self.multi_get_events(event_digests))
    }
}

impl ExecutionCacheWrite for ProxyCache {
    fn write_transaction_outputs(
        &self,
        epoch_id: EpochId,
        tx_outputs: Arc<TransactionOutputs>,
    ) -> BoxFuture<'_, SuiResult> {
        delegate_method!(self.write_transaction_outputs(epoch_id, tx_outputs))
    }

    fn acquire_transaction_locks<'a>(
        &'a self,
        epoch_store: &'a AuthorityPerEpochStore,
        owned_input_objects: &'a [ObjectRef],
        transaction: VerifiedSignedTransaction,
    ) -> BoxFuture<'a, SuiResult> {
        delegate_method!(self.acquire_transaction_locks(
            epoch_store,
            owned_input_objects,
            transaction
        ))
    }
}

impl AccumulatorStore for ProxyCache {
    fn get_object_ref_prior_to_key_deprecated(
        &self,
        object_id: &ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> SuiResult<Option<ObjectRef>> {
        delegate_method!(self.get_object_ref_prior_to_key_deprecated(object_id, version))
    }

    fn get_root_state_accumulator_for_epoch(
        &self,
        epoch: EpochId,
    ) -> SuiResult<Option<(CheckpointSequenceNumber, Accumulator)>> {
        delegate_method!(self.get_root_state_accumulator_for_epoch(epoch))
    }

    fn get_root_state_accumulator_for_highest_epoch(
        &self,
    ) -> SuiResult<Option<(EpochId, (CheckpointSequenceNumber, Accumulator))>> {
        delegate_method!(self.get_root_state_accumulator_for_highest_epoch())
    }

    fn insert_state_accumulator_for_epoch(
        &self,
        epoch: EpochId,
        checkpoint_seq_num: &CheckpointSequenceNumber,
        acc: &Accumulator,
    ) -> SuiResult {
        delegate_method!(self.insert_state_accumulator_for_epoch(epoch, checkpoint_seq_num, acc))
    }

    fn iter_live_object_set(
        &self,
        include_wrapped_tombstone: bool,
    ) -> Box<dyn Iterator<Item = crate::authority::authority_store_tables::LiveObject> + '_> {
        delegate_method!(self.iter_live_object_set(include_wrapped_tombstone))
    }

    fn iter_cached_live_object_set_for_testing(
        &self,
        include_wrapped_tombstone: bool,
    ) -> Box<dyn Iterator<Item = crate::authority::authority_store_tables::LiveObject> + '_> {
        delegate_method!(self.iter_cached_live_object_set_for_testing(include_wrapped_tombstone))
    }
}

impl ExecutionCacheCommit for ProxyCache {
    fn commit_transaction_outputs<'a>(
        &'a self,
        epoch: EpochId,
        digests: &'a [TransactionDigest],
    ) -> BoxFuture<'a, SuiResult> {
        delegate_method!(self.commit_transaction_outputs(epoch, digests))
    }

    fn persist_transactions<'a>(
        &'a self,
        digests: &'a [TransactionDigest],
    ) -> BoxFuture<'a, SuiResult> {
        delegate_method!(self.persist_transactions(digests))
    }
}

impl CheckpointCache for ProxyCache {
    fn deprecated_get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<Option<(EpochId, CheckpointSequenceNumber)>> {
        delegate_method!(self.deprecated_get_transaction_checkpoint(digest))
    }

    fn deprecated_multi_get_transaction_checkpoint(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<(EpochId, CheckpointSequenceNumber)>>> {
        delegate_method!(self.deprecated_multi_get_transaction_checkpoint(digests))
    }

    fn deprecated_insert_finalized_transactions(
        &self,
        digests: &[TransactionDigest],
        epoch: EpochId,
        sequence: CheckpointSequenceNumber,
    ) -> SuiResult {
        delegate_method!(self.deprecated_insert_finalized_transactions(digests, epoch, sequence))
    }
}

impl ExecutionCacheReconfigAPI for ProxyCache {
    fn insert_genesis_object(&self, object: Object) -> SuiResult {
        delegate_method!(self.insert_genesis_object(object))
    }

    fn bulk_insert_genesis_objects(&self, objects: &[Object]) -> SuiResult {
        delegate_method!(self.bulk_insert_genesis_objects(objects))
    }

    fn revert_state_update(&self, digest: &TransactionDigest) -> SuiResult {
        delegate_method!(self.revert_state_update(digest))
    }

    fn set_epoch_start_configuration(
        &self,
        epoch_start_config: &EpochStartConfiguration,
    ) -> SuiResult {
        delegate_method!(self.set_epoch_start_configuration(epoch_start_config))
    }

    fn update_epoch_flags_metrics(&self, old: &[EpochFlag], new: &[EpochFlag]) {
        delegate_method!(self.update_epoch_flags_metrics(old, new))
    }

    fn clear_state_end_of_epoch(&self, execution_guard: &ExecutionLockWriteGuard<'_>) {
        delegate_method!(self.clear_state_end_of_epoch(execution_guard))
    }

    fn expensive_check_sui_conservation(
        &self,
        old_epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult {
        delegate_method!(self.expensive_check_sui_conservation(old_epoch_store))
    }

    fn checkpoint_db(&self, path: &std::path::Path) -> SuiResult {
        delegate_method!(self.checkpoint_db(path))
    }

    fn maybe_reaccumulate_state_hash(
        &self,
        cur_epoch_store: &AuthorityPerEpochStore,
        new_protocol_version: ProtocolVersion,
    ) {
        delegate_method!(self.maybe_reaccumulate_state_hash(cur_epoch_store, new_protocol_version))
    }

    fn reconfigure_cache<'a>(
        &'a self,
        epoch_start_config: &'a EpochStartConfiguration,
    ) -> BoxFuture<'a, ()> {
        self.reconfigure_cache_impl(epoch_start_config).boxed()
    }
}

impl StateSyncAPI for ProxyCache {
    fn insert_transaction_and_effects(
        &self,
        transaction: &VerifiedTransaction,
        transaction_effects: &TransactionEffects,
    ) -> SuiResult {
        delegate_method!(self.insert_transaction_and_effects(transaction, transaction_effects))
    }

    fn multi_insert_transaction_and_effects(
        &self,
        transactions_and_effects: &[VerifiedExecutionData],
    ) -> SuiResult {
        delegate_method!(self.multi_insert_transaction_and_effects(transactions_and_effects))
    }
}

impl TestingAPI for ProxyCache {
    fn database_for_testing(&self) -> Arc<AuthorityStore> {
        delegate_method!(self.database_for_testing())
    }
}
