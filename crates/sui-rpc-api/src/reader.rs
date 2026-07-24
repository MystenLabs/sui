// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use mysten_common::ZipDebugEqIteratorExt;
use sui_sdk_types::{EpochId, ValidatorCommittee};
use sui_types::base_types::TransactionDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::storage::ObjectKey;
use sui_types::storage::RpcStateReader;
use sui_types::storage::error::{Error as StorageError, Result};
use tap::Pipe;

#[derive(Clone)]
pub struct StateReader {
    inner: Arc<dyn RpcStateReader>,
}

impl StateReader {
    pub fn new(inner: Arc<dyn RpcStateReader>) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &Arc<dyn RpcStateReader> {
        &self.inner
    }

    #[tracing::instrument(skip(self))]
    pub fn get_committee(&self, epoch: EpochId) -> Option<ValidatorCommittee> {
        self.inner
            .get_committee(epoch)
            .map(|committee| (*committee).clone().into())
    }

    #[tracing::instrument(skip(self))]
    pub fn get_system_state(&self) -> Result<sui_types::sui_system_state::SuiSystemState> {
        sui_types::sui_system_state::get_sui_system_state(self.inner())
            .map_err(StorageError::custom)
            .map_err(StorageError::custom)
    }

    #[tracing::instrument(skip(self))]
    pub fn get_display_object_v2_by_type(
        &self,
        object_type: &move_core_types::language_storage::StructTag,
    ) -> Option<sui_types::display_registry::Display> {
        let object_id =
            sui_types::display_registry::display_object_id(object_type.clone().into()).ok()?;

        let object = self.inner.get_object(&object_id)?;

        let move_object = object.data.try_as_move()?;

        bcs::from_bytes(move_object.contents()).ok()
    }

    #[tracing::instrument(skip(self))]
    pub fn get_system_state_summary(
        &self,
    ) -> Result<sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary> {
        use sui_types::sui_system_state::SuiSystemStateTrait;

        let system_state = self.get_system_state()?;
        let summary = system_state.into_sui_system_state_summary();

        Ok(summary)
    }

    pub fn get_authenticator_state(
        &self,
    ) -> Result<Option<sui_types::authenticator_state::AuthenticatorStateInner>> {
        sui_types::authenticator_state::get_authenticator_state(self.inner())
            .map_err(StorageError::custom)
    }

    #[tracing::instrument(skip(self))]
    pub fn get_transaction(
        &self,
        digest: sui_sdk_types::Digest,
    ) -> crate::Result<(
        sui_types::transaction::TransactionData,
        Vec<sui_types::signature::GenericSignature>,
        sui_types::effects::TransactionEffects,
        Option<sui_types::effects::TransactionEvents>,
    )> {
        let transaction_digest = digest.into();

        let transaction = (*self
            .inner()
            .get_transaction(&transaction_digest)
            .ok_or(TransactionNotFoundError(digest))?)
        .clone()
        .into_inner();
        let effects = self
            .inner()
            .get_transaction_effects(&transaction_digest)
            .ok_or(TransactionNotFoundError(digest))?;
        let events = if effects.events_digest().is_some() {
            self.inner()
                .get_events(effects.transaction_digest())
                .ok_or(TransactionNotFoundError(digest))?
                .pipe(Some)
        } else {
            None
        };

        let transaction = transaction.into_data().into_inner();
        let signatures = transaction.tx_signatures;
        let transaction = transaction.intent_message.value;

        Ok((transaction, signatures, effects, events))
    }

    /// Fetch transaction reads using checkpoints supplied by the ledger index rows.
    pub fn multi_get_transaction_reads(
        &self,
        items: &[(sui_sdk_types::Digest, u64)],
    ) -> crate::Result<Vec<TransactionRead>> {
        let transaction_digests = items
            .iter()
            .map(|(digest, _)| (*digest).into())
            .collect::<Vec<TransactionDigest>>();
        let transactions = self.inner().multi_get_transactions(&transaction_digests);
        let effects = self
            .inner()
            .multi_get_transaction_effects(&transaction_digests);
        let events = self.inner().multi_get_events(&transaction_digests);
        let unchanged_loaded_runtime_objects = self
            .inner()
            .multi_get_unchanged_loaded_runtime_objects(&transaction_digests);
        let timestamps = dedup_checkpoint_timestamps(
            items.iter().map(|(_, checkpoint)| *checkpoint),
            |unique| {
                self.inner()
                    .multi_get_checkpoint_by_sequence_number(unique)
                    .into_iter()
                    .map(|checkpoint| checkpoint.map(|checkpoint| checkpoint.timestamp_ms))
                    .collect()
            },
        );

        let mut reads = Vec::with_capacity(items.len());
        for (
            ((((digest, checkpoint), _transaction_digest), transaction), (effects, events)),
            unchanged_loaded_runtime_objects,
        ) in items
            .iter()
            .copied()
            .zip_debug_eq(transaction_digests)
            .zip_debug_eq(transactions)
            .zip_debug_eq(effects.into_iter().zip_debug_eq(events))
            .zip_debug_eq(unchanged_loaded_runtime_objects)
        {
            let transaction = (*transaction.ok_or(TransactionNotFoundError(digest))?)
                .clone()
                .into_inner();
            let effects = effects.ok_or(TransactionNotFoundError(digest))?;
            let events = if effects.events_digest().is_some() {
                events.ok_or(TransactionNotFoundError(digest))?.pipe(Some)
            } else {
                None
            };

            let transaction = transaction.into_data().into_inner();
            let signatures = transaction.tx_signatures;
            let transaction = transaction.intent_message.value;
            let timestamp_ms = timestamps[&checkpoint];

            reads.push(TransactionRead {
                digest,
                transaction,
                signatures,
                effects,
                events,
                timestamp_ms,
                unchanged_loaded_runtime_objects,
            });
        }

        Ok(reads)
    }

    pub fn multi_get_events(
        &self,
        digests: &[TransactionDigest],
    ) -> Vec<Option<sui_types::effects::TransactionEvents>> {
        self.inner().multi_get_events(digests)
    }

    #[tracing::instrument(skip(self))]
    pub fn get_transaction_read(
        &self,
        digest: sui_sdk_types::Digest,
    ) -> crate::Result<TransactionRead> {
        let (transaction, signatures, effects, events) = self.get_transaction(digest)?;

        let checkpoint = self.inner().get_transaction_checkpoint(&(digest.into()));
        let timestamp_ms =
            checkpoint.and_then(|checkpoint| self.checkpoint_timestamp_ms(checkpoint));

        let unchanged_loaded_runtime_objects = self
            .inner()
            .get_unchanged_loaded_runtime_objects(&(digest.into()));

        Ok(TransactionRead {
            digest,
            transaction,
            signatures,
            effects,
            events,
            timestamp_ms,
            unchanged_loaded_runtime_objects,
        })
    }

    /// Timestamp of the checkpoint summary, if it is still in the store.
    fn checkpoint_timestamp_ms(&self, checkpoint: u64) -> Option<u64> {
        self.inner()
            .get_checkpoint_by_sequence_number(checkpoint)
            .map(|checkpoint| checkpoint.timestamp_ms)
    }

    pub fn lookup_address_balance(
        &self,
        owner: sui_types::base_types::SuiAddress,
        coin_type: move_core_types::language_storage::StructTag,
    ) -> Option<u64> {
        use sui_types::MoveTypeTagTraitGeneric;
        use sui_types::SUI_ACCUMULATOR_ROOT_OBJECT_ID;
        use sui_types::accumulator_root::AccumulatorKey;
        use sui_types::dynamic_field::DynamicFieldKey;

        let balance_type = sui_types::balance::Balance::type_tag(coin_type.into());

        let key = AccumulatorKey { owner };
        let key_type_tag = AccumulatorKey::get_type_tag(&[balance_type]);

        DynamicFieldKey(SUI_ACCUMULATOR_ROOT_OBJECT_ID, key, key_type_tag)
            .into_unbounded_id()
            .unwrap()
            .load_object(self.inner())
            .and_then(|o| o.load_value::<u128>().ok())
            .map(|balance| balance as u64)
    }

    // Return the lowest available checkpoint watermark for which the RPC service can return proper
    // responses for.
    pub fn get_lowest_available_checkpoint(&self) -> Result<u64, crate::RpcError> {
        // This is the lowest lowest_available_checkpoint from the checkpoint store
        let lowest_available_checkpoint = self.inner().get_lowest_available_checkpoint()?;
        // This is the lowest lowest_available_checkpoint from the perpetual store
        let lowest_available_checkpoint_objects =
            self.inner().get_lowest_available_checkpoint_objects()?;

        // Return the higher of the two for our lower watermark
        Ok(lowest_available_checkpoint.max(lowest_available_checkpoint_objects))
    }
}

fn dedup_checkpoint_timestamps(
    checkpoints: impl IntoIterator<Item = u64>,
    fetch: impl FnOnce(&[u64]) -> Vec<Option<u64>>,
) -> HashMap<u64, Option<u64>> {
    let unique_checkpoints = checkpoints
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if unique_checkpoints.is_empty() {
        return HashMap::new();
    }

    let timestamps = fetch(&unique_checkpoints);
    unique_checkpoints
        .into_iter()
        .zip_debug_eq(timestamps)
        .collect()
}

#[derive(Debug)]
pub struct TransactionRead {
    pub digest: sui_sdk_types::Digest,
    pub transaction: sui_types::transaction::TransactionData,
    pub signatures: Vec<sui_types::signature::GenericSignature>,
    pub effects: sui_types::effects::TransactionEffects,
    pub events: Option<sui_types::effects::TransactionEvents>,
    pub timestamp_ms: Option<u64>,
    pub unchanged_loaded_runtime_objects: Option<Vec<ObjectKey>>,
}

#[derive(Debug)]
pub struct TransactionNotFoundError(pub sui_sdk_types::Digest);

impl std::fmt::Display for TransactionNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Transaction {} not found", self.0)
    }
}

impl std::error::Error for TransactionNotFoundError {}

impl From<TransactionNotFoundError> for crate::RpcError {
    fn from(value: TransactionNotFoundError) -> Self {
        Self::new(tonic::Code::NotFound, value.to_string())
    }
}

pub struct DisplayStore<'s> {
    state: &'s StateReader,
}

impl<'s> DisplayStore<'s> {
    pub fn new(state: &'s StateReader) -> Self {
        Self { state }
    }
}

#[async_trait::async_trait]
impl sui_display::v2::Store for DisplayStore<'_> {
    async fn latest(
        &self,
        id: move_core_types::account_address::AccountAddress,
    ) -> anyhow::Result<Option<(move_core_types::annotated_value::MoveTypeLayout, Vec<u8>)>> {
        let Some(object) = self.state.inner().get_object(&id.into()) else {
            return Ok(None);
        };

        let Some(move_object) = object.data.try_as_move() else {
            return Ok(None);
        };

        let object_type = move_object.type_().clone().into();

        let Some(layout) = self.state.inner().get_struct_layout(&object_type)? else {
            return Ok(None);
        };

        Ok(Some((layout, move_object.contents().to_vec())))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn dedup_checkpoint_timestamps_fetches_sorted_unique_checkpoints_once() {
        let fetch_calls = Cell::new(0);
        let timestamps = dedup_checkpoint_timestamps([5, 5, 3, 5, 3], |checkpoints| {
            fetch_calls.set(fetch_calls.get() + 1);
            assert_eq!(checkpoints, &[3, 5]);
            vec![Some(300), Some(500)]
        });

        assert_eq!(fetch_calls.get(), 1);
        assert_eq!(timestamps.len(), 2);
        assert_eq!(timestamps[&3], Some(300));
        assert_eq!(timestamps[&5], Some(500));
    }

    #[test]
    fn dedup_checkpoint_timestamps_skips_fetch_for_empty_input() {
        let fetch_calls = Cell::new(0);
        let timestamps = dedup_checkpoint_timestamps([], |_| {
            fetch_calls.set(fetch_calls.get() + 1);
            Vec::new()
        });

        assert_eq!(fetch_calls.get(), 0);
        assert!(timestamps.is_empty());
    }

    #[test]
    fn dedup_checkpoint_timestamps_preserves_missing_summary() {
        let timestamps = dedup_checkpoint_timestamps([2, 1], |checkpoints| {
            assert_eq!(checkpoints, &[1, 2]);
            vec![None, Some(200)]
        });

        assert_eq!(timestamps[&1], None);
        assert_eq!(timestamps[&2], Some(200));
    }
}
