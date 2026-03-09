// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_sdk_types::{EpochId, ValidatorCommittee};
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
        use sui_types::effects::TransactionEffectsAPI;

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

    #[tracing::instrument(skip(self))]
    pub fn get_transaction_read(
        &self,
        digest: sui_sdk_types::Digest,
    ) -> crate::Result<TransactionRead> {
        let (transaction, signatures, effects, events) = self.get_transaction(digest)?;

        let checkpoint = self.inner().get_transaction_checkpoint(&(digest.into()));

        let timestamp_ms = if let Some(checkpoint) = checkpoint {
            self.inner()
                .get_checkpoint_by_sequence_number(checkpoint)
                .map(|checkpoint| checkpoint.timestamp_ms)
        } else {
            None
        };

        let unchanged_loaded_runtime_objects = self
            .inner()
            .get_unchanged_loaded_runtime_objects(&(digest.into()));

        Ok(TransactionRead {
            digest,
            transaction,
            signatures,
            effects,
            events,
            checkpoint,
            timestamp_ms,
            unchanged_loaded_runtime_objects,
        })
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

#[derive(Debug)]
pub struct TransactionRead {
    pub digest: sui_sdk_types::Digest,
    pub transaction: sui_types::transaction::TransactionData,
    pub signatures: Vec<sui_types::signature::GenericSignature>,
    pub effects: sui_types::effects::TransactionEffects,
    pub events: Option<sui_types::effects::TransactionEvents>,
    #[allow(unused)]
    pub checkpoint: Option<u64>,
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
