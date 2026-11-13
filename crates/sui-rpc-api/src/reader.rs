// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use mysten_common::ZipDebugEqIteratorExt;
use sui_package_resolver::{SyncPackageStore, SyncResolver};
use sui_sdk_types::{EpochId, ValidatorCommittee};
use sui_types::base_types::ObjectID;
use sui_types::base_types::TransactionDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use sui_types::storage::RpcStateReader;
use sui_types::storage::error::{Error as StorageError, Result};
use tap::Pipe;

use crate::ErrorDetails;
use crate::ErrorReason;
use crate::ledger_history::query_options::{
    above_latest_available, below_serving_floor, ledger_error_info,
};

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
    pub fn get_address_aliases(
        &self,
        address: sui_types::base_types::SuiAddress,
    ) -> Result<Option<sui_types::address_alias::AddressAliases>> {
        let object_id = sui_types::address_alias::address_aliases_object_id(address)
            .map_err(StorageError::custom)?;

        let Some(object) = self.inner.get_object(&object_id) else {
            return Ok(None);
        };

        let move_object = object
            .data
            .try_as_move()
            .ok_or_else(|| StorageError::custom("address aliases object is not a Move object"))?;

        bcs::from_bytes(move_object.contents())
            .map(Some)
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

    /// The upper bound for checkpoint-pinned reads ([`Self::get_object_at_checkpoint`], and
    /// `GetPackage`'s `at_checkpoint`): the highest checkpoint every rpc index has committed.
    ///
    /// Read it before the reads. It only grows, and the indexes only add data above it, so it
    /// stays valid for reads made after it was read. The lower bound is the opposite; see
    /// [`Self::at_checkpoint_lower_bound`].
    ///
    /// `UNIMPLEMENTED` (`NOT_SERVED_HERE`, cause `no_rpc_indexes`) if the rpc indexes are
    /// disabled. `UNAVAILABLE` (`NOT_READY`) if they have not committed a checkpoint yet (cause
    /// `index_empty`), or are still backfilling history after a restore (cause
    /// `restore_backfill`): until every history index reaches the restore checkpoint, a
    /// checkpoint-pinned read can be wrong at any checkpoint the indexes cover (restore of a
    /// pruned node), or the highest indexed checkpoint lies far below the restored live state
    /// (restore with nothing pruned).
    pub fn at_checkpoint_upper_bound(&self) -> Result<AtCheckpointUpperBound, crate::RpcError> {
        let indexes = self.inner().indexes().ok_or_else(indexes_disabled)?;
        if indexes.is_history_backfill_pending().map_err(index_error)? {
            return Err(not_ready(
                NotReadyCause::RestoreBackfill,
                "rpc index history is still backfilling after a restore",
            ));
        }
        let highest_indexed = indexes
            .get_highest_indexed_checkpoint_seq_number()
            .map_err(index_error)?
            .ok_or_else(index_empty)?;
        Ok(AtCheckpointUpperBound { highest_indexed })
    }

    /// `UNIMPLEMENTED` (`NOT_SERVED_HERE`, cause `no_object_history`) if this node does not keep
    /// object versions once they are replaced (see
    /// [`RpcStateReader::retains_object_version_history`]): reads that depend on an object's
    /// replaced versions would find them gone and answer `NotFound`. The refusal depends only on
    /// the node's configuration, so check it before anything that depends on the node's state.
    pub fn require_object_version_history(&self) -> Result<(), crate::RpcError> {
        if self.inner().retains_object_version_history() {
            return Ok(());
        }
        Err(not_served_here(
            NotServedHereCause::NoObjectHistory,
            "this node keeps only the latest version of each object, so it cannot answer reads \
             that depend on earlier versions",
        ))
    }

    /// The lower bound for checkpoint-pinned reads: the checkpoint just below the higher of
    /// [`RpcStateReader::get_lowest_available_checkpoint_objects`] and
    /// [`RpcStateReader::get_lowest_checkpoint_with_index_history`]. The lowest available
    /// checkpoint's input objects are available, and they are the object state at the end of the
    /// checkpoint before: pruning through a checkpoint deletes only the object versions and index
    /// rows replaced at or before it. Likewise, index rows loaded without history hold the state
    /// at the end of the checkpoint before the first one with index history.
    ///
    /// Read it after the reads, and answer every read at a checkpoint below it as pruned,
    /// whatever the read returned. Pruning can raise the floor while the reads run, and a read
    /// below the new floor may have seen pruned data. Pruning raises the floor no later than it
    /// deletes data (the rpc-store commits its retractions and floor before the object versions
    /// are deleted), only deletes data that reads below the new floor need, and never lowers the
    /// floor, so a read at a checkpoint this bound accepts saw no pruned data.
    pub fn at_checkpoint_lower_bound(&self) -> Result<AtCheckpointLowerBound, crate::RpcError> {
        let lowest_available_objects = self.inner().get_lowest_available_checkpoint_objects()?;
        let lowest_with_index_history = self.inner().get_lowest_checkpoint_with_index_history()?;
        Ok(AtCheckpointLowerBound {
            lowest_available: lowest_available_objects.max(lowest_with_index_history),
        })
    }

    /// The live object as of the end of `checkpoint`, or `None` if it did not exist yet or had
    /// been deleted or wrapped. `OutOfRange` (`ABOVE_LATEST_AVAILABLE`) if `checkpoint` is above
    /// `upper_bound`; check the lower bound after the read with
    /// [`Self::at_checkpoint_lower_bound`].
    pub fn get_object_at_checkpoint(
        &self,
        object_id: ObjectID,
        checkpoint: u64,
        upper_bound: &AtCheckpointUpperBound,
    ) -> Result<Option<Object>, crate::RpcError> {
        upper_bound.check(checkpoint)?;

        let indexes = self.inner().indexes().ok_or_else(indexes_disabled)?;
        let Some(version) = indexes
            .get_object_version_at_checkpoint(object_id, checkpoint)
            .map_err(index_error)?
        else {
            return Ok(None);
        };
        // A deletion or wrapping tombstone has no object at its version.
        Ok(self.inner().get_object_by_key(&object_id, version))
    }
}

/// See `StateReader::at_checkpoint_upper_bound`. Read once per request so that a batch checks
/// every item against the same bound. Other implementors of the rpc services build it with
/// [`Self::new`] from the height they serve (e.g. sui-kv-rpc's served height), so every backend
/// applies the same check.
#[derive(Clone, Copy, Debug)]
pub struct AtCheckpointUpperBound {
    /// The highest checkpoint every rpc index has committed.
    highest_indexed: u64,
}

impl AtCheckpointUpperBound {
    /// The bound of a backend whose indexes have all committed `highest_indexed`.
    pub fn new(highest_indexed: u64) -> Self {
        Self { highest_indexed }
    }

    /// The highest checkpoint every rpc index has committed.
    pub fn highest_indexed(&self) -> u64 {
        self.highest_indexed
    }

    /// `OutOfRange` (`ABOVE_LATEST_AVAILABLE`, for the request field `at_checkpoint`) if
    /// `checkpoint` is above the bound: the indexes may not have caught up to it, so an answer
    /// could be wrong.
    pub fn check(&self, checkpoint: u64) -> Result<(), crate::RpcError> {
        if checkpoint > self.highest_indexed {
            return Err(checkpoint_not_yet_indexed_error(
                checkpoint,
                self.highest_indexed,
            ));
        }
        Ok(())
    }

    /// `OutOfRange` (`ABOVE_LATEST_AVAILABLE`, for a cursor) if a cursor recording `checkpoint`
    /// names a position above the bound, which the indexes have not caught up to.
    pub fn check_cursor(&self, checkpoint: u64) -> Result<(), crate::RpcError> {
        if checkpoint > self.highest_indexed {
            return Err(above_latest_available(
                "cursor",
                checkpoint,
                self.highest_indexed.saturating_add(1),
            ));
        }
        Ok(())
    }

    /// The error for a cursor or page token naming a position the indexes have not reached whose
    /// checkpoint is not known, such as a version this backend has no row for: `OutOfRange` with
    /// an `ABOVE_LATEST_AVAILABLE` [`ErrorInfo`](crate::proto::google::rpc::ErrorInfo) for the
    /// bound "cursor", placing the cursor at the lowest checkpoint it can be at, the first one
    /// above the bound.
    pub fn unindexed_cursor_error(&self) -> crate::RpcError {
        let first_unindexed = self.highest_indexed.saturating_add(1);
        above_latest_available("cursor", first_unindexed, first_unindexed)
    }
}

/// The error for an `at_checkpoint` read above the highest indexed checkpoint: `OutOfRange` with
/// an `ABOVE_LATEST_AVAILABLE` [`ErrorInfo`](crate::proto::google::rpc::ErrorInfo). Public so
/// reads that compare the bound themselves, rather than through
/// [`AtCheckpointUpperBound::check`], fail identically on every backend.
pub fn checkpoint_not_yet_indexed_error(checkpoint: u64, highest_indexed: u64) -> crate::RpcError {
    above_latest_available(
        "at_checkpoint",
        checkpoint,
        highest_indexed.saturating_add(1),
    )
}

/// See `StateReader::at_checkpoint_lower_bound`.
#[derive(Clone, Copy, Debug)]
pub struct AtCheckpointLowerBound {
    /// The lowest checkpoint whose input objects are available and covered by the rpc indexes'
    /// history. The object state at the end of the checkpoint before it is intact.
    lowest_available: u64,
}

impl AtCheckpointLowerBound {
    /// The bound of a node whose lowest available checkpoint, for object data and index history
    /// alike, is `lowest_available`.
    #[cfg(test)]
    pub(crate) fn new(lowest_available: u64) -> Self {
        Self { lowest_available }
    }

    /// `OutOfRange` (`BELOW_EARLIEST_AVAILABLE`) if `checkpoint` is below the bound: its history
    /// may have been pruned, so an answer could be wrong.
    pub fn check(&self, checkpoint: u64) -> Result<(), crate::RpcError> {
        if checkpoint < self.lowest_intact() {
            return Err(self.pruned());
        }
        Ok(())
    }

    /// `OutOfRange` (`BELOW_EARLIEST_AVAILABLE`) for data this node has pruned, such as a
    /// replaced version of an object named by its version rather than a checkpoint.
    pub fn pruned(&self) -> crate::RpcError {
        below_serving_floor(self.lowest_intact())
    }

    /// The lowest checkpoint whose object state is intact, if history below it is gone: object
    /// versions replaced at or before its end are pruned, or the indexes never had them. `None`
    /// when the history is complete from genesis.
    pub fn pruned_floor(&self) -> Option<u64> {
        self.lowest_available.checked_sub(1)
    }

    /// The lowest checkpoint whose object state is intact: the one just below
    /// `lowest_available`, or 0.
    fn lowest_intact(&self) -> u64 {
        self.lowest_available.saturating_sub(1)
    }
}

/// The `cause` of a `NOT_SERVED_HERE` refusal: why this node is not set up for a request. The
/// same request to the same node always fails the same way, so clients do not retry it there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotServedHereCause {
    /// The node runs without rpc indexes.
    NoRpcIndexes,
    /// The node keeps only the latest version of each object (see
    /// [`RpcStateReader::retains_object_version_history`]).
    NoObjectHistory,
    /// A kv-rpc `enable-*` flag the request needs is off.
    DisabledByConfig,
}

impl NotServedHereCause {
    /// The value of the refusal's `cause` metadata.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoRpcIndexes => "no_rpc_indexes",
            Self::NoObjectHistory => "no_object_history",
            Self::DisabledByConfig => "disabled_by_config",
        }
    }
}

/// The `cause` of a `NOT_READY` refusal: the temporary state that keeps this node from serving a
/// request it will serve once the state clears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotReadyCause {
    /// The rpc indexes have not committed a checkpoint yet.
    IndexEmpty,
    /// The rpc indexes are still backfilling history after a restore from a snapshot.
    RestoreBackfill,
}

impl NotReadyCause {
    /// The value of the refusal's `cause` metadata.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IndexEmpty => "index_empty",
            Self::RestoreBackfill => "restore_backfill",
        }
    }
}

/// A refusal for a request this node is not set up for: `Unimplemented` with a `NOT_SERVED_HERE`
/// [`ErrorInfo`](crate::proto::google::rpc::ErrorInfo) whose `cause` metadata is `cause`.
pub fn not_served_here(cause: NotServedHereCause, message: impl Into<String>) -> crate::RpcError {
    refusal(
        tonic::Code::Unimplemented,
        ErrorReason::NotServedHere,
        cause.as_str(),
        message,
    )
}

/// A refusal for a request this node cannot serve now but will: `Unavailable` with a `NOT_READY`
/// [`ErrorInfo`](crate::proto::google::rpc::ErrorInfo) whose `cause` metadata is `cause`.
pub fn not_ready(cause: NotReadyCause, message: impl Into<String>) -> crate::RpcError {
    refusal(
        tonic::Code::Unavailable,
        ErrorReason::NotReady,
        cause.as_str(),
        message,
    )
}

fn refusal(
    code: tonic::Code,
    reason: ErrorReason,
    cause: &str,
    message: impl Into<String>,
) -> crate::RpcError {
    let error_info = ledger_error_info(reason, [("cause", cause.to_owned())]);
    crate::RpcError::new(code, message)
        .with_details(ErrorDetails::new().with_error_info(error_info))
}

/// [`not_served_here`] with cause `no_rpc_indexes`: this node runs without rpc indexes, so reads
/// that need them cannot be answered.
pub(crate) fn indexes_disabled() -> crate::RpcError {
    not_served_here(NotServedHereCause::NoRpcIndexes, "rpc indexes are disabled")
}

/// [`not_ready`] with cause `index_empty`: the rpc indexes have not committed a checkpoint yet.
pub(crate) fn index_empty() -> crate::RpcError {
    not_ready(NotReadyCause::IndexEmpty, "rpc index is empty")
}

fn index_error(error: StorageError) -> crate::RpcError {
    crate::RpcError::new(tonic::Code::Internal, error.to_string())
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

pub struct DisplayStore<'s, S> {
    state: &'s StateReader,
    resolver: &'s SyncResolver<S>,
}

impl<'s, S> DisplayStore<'s, S> {
    pub fn new(state: &'s StateReader, resolver: &'s SyncResolver<S>) -> Self {
        Self { state, resolver }
    }
}

#[async_trait::async_trait]
impl<S: SyncPackageStore + Sync> sui_display::v2::Store for DisplayStore<'_, S> {
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

        let layout = self.resolver.datatype_layout(&object_type)?.into_layout();

        Ok(Some((layout, move_object.contents().to_vec())))
    }
}

/// Asserts `error` is `code` with exactly one [`ErrorInfo`](crate::proto::google::rpc::ErrorInfo):
/// `reason` in the [`ErrorReason::DOMAIN`] domain, whose only metadata is `cause`.
#[cfg(test)]
pub(crate) fn assert_refusal(
    error: crate::RpcError,
    code: tonic::Code,
    reason: ErrorReason,
    cause: &str,
) {
    use crate::proto::google::rpc::ErrorInfo;

    let status = error.into_status_proto();
    assert_eq!(status.code, code as i32, "{status:?}");
    let error_infos: Vec<ErrorInfo> = status
        .details
        .iter()
        .filter_map(|detail| detail.to_msg::<ErrorInfo>().ok())
        .collect();
    let [error_info] = error_infos.as_slice() else {
        panic!("expected exactly one ErrorInfo in {status:?}");
    };
    assert_eq!(error_info.reason, reason.as_str_name(), "{status:?}");
    assert_eq!(error_info.domain, ErrorReason::DOMAIN, "{status:?}");
    assert_eq!(
        error_info.metadata,
        [("cause".to_owned(), cause.to_owned())].into(),
        "{status:?}"
    );
}

/// [`assert_refusal`] for [`not_served_here`] with `cause`.
#[cfg(test)]
pub(crate) fn assert_not_served_here(error: crate::RpcError, cause: NotServedHereCause) {
    assert_refusal(
        error,
        tonic::Code::Unimplemented,
        ErrorReason::NotServedHere,
        cause.as_str(),
    );
}

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::Cell;

    use super::*;
    // Tests in other modules import the helpers from here.
    pub(crate) use super::assert_refusal;

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

    /// A [`StateReader`] over a fresh rpc-store that registers the embedded fullnode's pipelines.
    pub(crate) fn rpc_store_state_reader()
    -> (tempfile::TempDir, sui_consistent_store::Db, StateReader) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = sui_consistent_store::Db::open::<sui_rpc_store::RpcStoreSchema>(
            dir.path(),
            sui_consistent_store::DbOptions::default(),
        )
        .unwrap();
        let store = sui_rpc_store::RpcStoreReader::new(db.clone(), Arc::new(schema));
        (dir, db, StateReader::new(Arc::new(store)))
    }

    /// Commits `checkpoint` as the watermark of every pipeline the store registers.
    pub(crate) fn commit_every_pipeline(db: &sui_consistent_store::Db, checkpoint: u64) {
        let framework = sui_consistent_store::FrameworkSchema::new(db.clone());
        let mut batch = db.batch();
        for name in sui_rpc_store::LIVE_COHORT
            .iter()
            .chain(sui_rpc_store::HISTORY_COHORT)
        {
            batch
                .put(
                    &framework.watermarks,
                    &sui_consistent_store::PipelineTaskKey::new(*name),
                    &sui_consistent_store::Watermark::for_checkpoint(checkpoint),
                )
                .unwrap();
        }
        batch.commit().unwrap();
    }

    /// Records `state` as the restore state of each of `pipelines`.
    pub(crate) fn record_restore_state(
        db: &sui_consistent_store::Db,
        pipelines: &[&str],
        state: sui_consistent_store::restore_state::State,
    ) {
        let framework = sui_consistent_store::FrameworkSchema::new(db.clone());
        let mut batch = db.batch();
        for name in pipelines {
            batch
                .put(
                    &framework.restore,
                    &sui_consistent_store::PipelineTaskKey::new(*name),
                    &sui_consistent_store::RestoreState {
                        state: Some(state.clone()),
                    },
                )
                .unwrap();
        }
        batch.commit().unwrap();
    }

    /// Checkpoint-pinned reads are refused as not ready while the indexes are empty and while a
    /// restore that loaded history floor rows is pending, both before the restore completes and
    /// after, until the backfill commits the restore checkpoint; then the bound is the indexed tip.
    #[test]
    fn upper_bound_waits_for_index_and_restore_backfill() {
        use sui_consistent_store::restore_state;

        let (_dir, db, reader) = rpc_store_state_reader();
        assert_refusal(
            reader.at_checkpoint_upper_bound().unwrap_err(),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "index_empty",
        );

        commit_every_pipeline(&db, 10);
        record_restore_state(
            &db,
            &["object_version_by_checkpoint"],
            restore_state::State::InProgress(restore_state::InProgress::default()),
        );
        assert_refusal(
            reader.at_checkpoint_upper_bound().unwrap_err(),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "restore_backfill",
        );

        record_restore_state(
            &db,
            &["object_version_by_checkpoint"],
            restore_state::State::Complete(restore_state::Complete { restored_at: 20 }),
        );
        assert_refusal(
            reader.at_checkpoint_upper_bound().unwrap_err(),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "restore_backfill",
        );

        commit_every_pipeline(&db, 20);
        let bound = reader.at_checkpoint_upper_bound().unwrap();
        assert_eq!(bound.highest_indexed(), 20);
        bound.check(20).unwrap();
        bound.check(21).unwrap_err();
    }

    /// After a restore of the live cohort alone, the history cohort backfills from genesis with no
    /// restore rows of its own; checkpoint-pinned reads wait until it reaches the restore target
    /// rather than answer at the backfill's height.
    #[test]
    fn upper_bound_waits_for_history_backfill_after_live_only_restore() {
        use sui_consistent_store::restore_state;

        let (_dir, db, reader) = rpc_store_state_reader();
        record_restore_state(
            &db,
            sui_rpc_store::LIVE_COHORT,
            restore_state::State::Complete(restore_state::Complete { restored_at: 20 }),
        );
        commit_every_pipeline(&db, 10);
        assert_refusal(
            reader.at_checkpoint_upper_bound().unwrap_err(),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "restore_backfill",
        );

        commit_every_pipeline(&db, 20);
        assert_eq!(
            reader
                .at_checkpoint_upper_bound()
                .unwrap()
                .highest_indexed(),
            20
        );
    }

    /// The lowest available checkpoint's input objects are available, so the state at the end of
    /// the checkpoint before it is served, and only history below that one is pruned.
    #[test]
    fn lower_bound_serves_the_checkpoint_before_the_lowest_available() {
        use crate::ledger_history::query_options::assert_below_serving_floor_error;

        let unpruned = AtCheckpointLowerBound::new(0);
        unpruned.check(0).unwrap();
        assert_eq!(unpruned.pruned_floor(), None);

        let genesis_pruned = AtCheckpointLowerBound::new(1);
        genesis_pruned.check(0).unwrap();
        assert_eq!(genesis_pruned.pruned_floor(), Some(0));

        let pruned = AtCheckpointLowerBound::new(10);
        pruned.check(9).unwrap();
        assert_below_serving_floor_error(pruned.check(8).unwrap_err(), 9);
        assert_below_serving_floor_error(pruned.pruned(), 9);
        assert_eq!(pruned.pruned_floor(), Some(9));
    }
}
