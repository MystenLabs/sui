// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use move_core_types::annotated_value::MoveTypeLayout;
use move_core_types::language_storage::StructTag;
use sui_rpc_store::RpcStoreReader;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::base_types::VersionNumber;
use sui_types::committee::Committee;
use sui_types::committee::EpochId;
use sui_types::digests::ChainIdentifier;
use sui_types::digests::CheckpointContentsDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEvents;
use sui_types::full_checkpoint_content::ObjectSet;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::messages_checkpoint::VersionedFullCheckpointContents;
use sui_types::object::Object;
use sui_types::storage::BalanceInfo;
use sui_types::storage::BalanceIterator;
use sui_types::storage::CoinInfo;
use sui_types::storage::DynamicFieldIteratorItem;
use sui_types::storage::DynamicFieldKey;
use sui_types::storage::EpochInfo;
use sui_types::storage::LedgerBitmapBucketIterator;
use sui_types::storage::LedgerTxSeqDigest;
use sui_types::storage::LedgerTxSeqDigestIterator;
use sui_types::storage::ObjectKey;
use sui_types::storage::ObjectStore;
use sui_types::storage::OwnedObjectInfo;
use sui_types::storage::ReadStore;
use sui_types::storage::RpcIndexes;
use sui_types::storage::RpcStateReader;
use sui_types::storage::RuntimeObjectResolver;
use sui_types::storage::error::Error as StorageError;
use sui_types::storage::error::Kind as StorageErrorKind;
use sui_types::storage::error::Result as StorageResult;
use sui_types::transaction::VerifiedTransaction;
use typed_store_error::TypedStoreError;

use crate::store::DataStore;

/// Fork-aware RPC reader used by `sui-rpc-node` service wiring.
///
/// This is the only adapter that implements the upstream RPC storage traits:
/// post-fork indexed data is read from `sui-rpc-store` first, while pre-fork
/// sparse reads are delegated to [`DataStore`].
pub(crate) struct ForkRpcReader {
    rpc_store: RpcStoreReader,
    store: DataStore,
}

impl ForkRpcReader {
    /// Creates an RPC reader over committed `sui-rpc-store` data and fork state.
    ///
    /// `rpc_store` handles native RPC-store reads. `store` owns fork-specific
    /// misses, including checkpoint-scoped remote fetches and index initialization.
    pub(crate) fn new(rpc_store: RpcStoreReader, store: DataStore) -> Self {
        Self { rpc_store, store }
    }
}

impl ObjectStore for ForkRpcReader {
    /// Reads the current object from `sui-rpc-store`, then asks fork state on a miss.
    ///
    /// Store errors are logged and converted to `None` because the
    /// `ObjectStore` trait has no error channel.
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.rpc_store.get_object(object_id).or_else(|| {
            optional_store_read(
                "object lookup",
                DataStore::get_object(&self.store, object_id).map_err(to_storage_error),
            )
        })
    }

    /// Reads one object version from `sui-rpc-store`, then asks fork state on a miss.
    ///
    /// The store may fetch and persist a pre-fork object version before
    /// returning it.
    fn get_object_by_key(&self, object_id: &ObjectID, version: VersionNumber) -> Option<Object> {
        self.rpc_store
            .get_object_by_key(object_id, version)
            .or_else(|| {
                optional_store_read(
                    "versioned object lookup",
                    self.store
                        .get_object_at_version(object_id, version.value())
                        .map_err(to_storage_error),
                )
            })
    }
}

impl RuntimeObjectResolver for ForkRpcReader {
    /// Resolves a child object through fork-specific bounded object reads.
    ///
    /// `RpcStoreReader` does not perform the fork's parent/version validation,
    /// so this path is always delegated to fork state.
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> sui_types::error::SuiResult<Option<Object>> {
        self.store
            .read_child_object_fallible(parent, child, child_version_upper_bound)
    }

    /// Resolves a received object through fork state.
    ///
    /// The store checks the owner and version rules required by transaction
    /// execution.
    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> sui_types::error::SuiResult<Option<Object>> {
        self.store.get_object_received_at_version_fallible(
            owner,
            receiving_object_id,
            receive_object_at_version,
        )
    }
}

impl ReadStore for ForkRpcReader {
    /// Reads committee information from committed `sui-rpc-store` data.
    fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>> {
        self.rpc_store.get_committee(epoch)
    }

    /// Reads the latest checkpoint, using fork state only when the local store reports it missing.
    fn get_latest_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        use_store_on_missing(self.rpc_store.get_latest_checkpoint(), || {
            self.store.latest_checkpoint_for_rpc()
        })
    }

    /// Reads the highest verified checkpoint, using fork state only for a missing local row.
    fn get_highest_verified_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        use_store_on_missing(self.rpc_store.get_highest_verified_checkpoint(), || {
            self.store.latest_checkpoint_for_rpc()
        })
    }

    /// Reads the highest synced checkpoint, using fork state only for a missing local row.
    fn get_highest_synced_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        use_store_on_missing(self.rpc_store.get_highest_synced_checkpoint(), || {
            self.store.highest_synced_checkpoint_for_rpc()
        })
    }

    /// Returns the remote chain's lowest available checkpoint through fork state.
    ///
    /// This value is not derived from the fork's local store.
    fn get_lowest_available_checkpoint(&self) -> StorageResult<CheckpointSequenceNumber> {
        DataStore::get_lowest_available_checkpoint(&self.store).map_err(to_storage_error)
    }

    /// Reads a checkpoint summary by checkpoint digest with optional fork-state lookup.
    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        self.rpc_store.get_checkpoint_by_digest(digest).or_else(|| {
            optional_store_read(
                "checkpoint digest lookup",
                DataStore::get_checkpoint_by_digest(&self.store, digest).map_err(to_storage_error),
            )
        })
    }

    /// Reads a checkpoint summary by sequence number with optional fork-state lookup.
    ///
    /// The store can persist pre-fork checkpoint rows into `sui-rpc-store`.
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.rpc_store
            .get_checkpoint_by_sequence_number(sequence_number)
            .or_else(|| {
                optional_store_read(
                    "checkpoint sequence lookup",
                    DataStore::get_checkpoint_by_sequence_number(&self.store, sequence_number)
                        .map_err(to_storage_error),
                )
            })
    }

    /// Reads checkpoint contents by content digest with optional fork-state lookup.
    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        self.rpc_store
            .get_checkpoint_contents_by_digest(digest)
            .or_else(|| {
                optional_store_read(
                    "checkpoint contents digest lookup",
                    DataStore::get_checkpoint_contents_by_digest(&self.store, digest)
                        .map_err(to_storage_error),
                )
            })
    }

    /// Reads checkpoint contents by sequence number with optional fork-state lookup.
    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        self.rpc_store
            .get_checkpoint_contents_by_sequence_number(sequence_number)
            .or_else(|| {
                optional_store_read(
                    "checkpoint contents sequence lookup",
                    DataStore::get_checkpoint_contents_by_sequence_number(
                        &self.store,
                        sequence_number,
                    )
                    .map_err(to_storage_error),
                )
            })
    }

    /// Reads a transaction by digest, returning the fork-state transaction in an `Arc` on a miss.
    fn get_transaction(&self, tx_digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        self.rpc_store.get_transaction(tx_digest).or_else(|| {
            optional_store_read(
                "transaction lookup",
                DataStore::get_transaction(&self.store, tx_digest).map_err(to_storage_error),
            )
            .map(Arc::new)
        })
    }

    /// Reads transaction effects by digest with optional fork-state lookup.
    fn get_transaction_effects(&self, tx_digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.rpc_store
            .get_transaction_effects(tx_digest)
            .or_else(|| {
                optional_store_read(
                    "transaction effects lookup",
                    DataStore::get_transaction_effects(&self.store, tx_digest)
                        .map_err(to_storage_error),
                )
            })
    }

    /// Reads transaction events by digest from the RPC store owned by fork state on a miss.
    fn get_events(&self, event_digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.rpc_store.get_events(event_digest).or_else(|| {
            optional_store_read(
                "transaction events lookup",
                self.store.transaction_events(event_digest),
            )
        })
    }

    /// Reads unchanged runtime-loaded objects from committed `sui-rpc-store` data only.
    ///
    /// There is no fork-state path because the fork does not synthesize
    /// this execution cache.
    fn get_unchanged_loaded_runtime_objects(
        &self,
        digest: &TransactionDigest,
    ) -> Option<Vec<ObjectKey>> {
        self.rpc_store.get_unchanged_loaded_runtime_objects(digest)
    }

    /// Reads the checkpoint sequence that contains a transaction with optional fork-state lookup.
    fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Option<CheckpointSequenceNumber> {
        self.rpc_store
            .get_transaction_checkpoint(digest)
            .or_else(|| {
                optional_store_read(
                    "transaction checkpoint lookup",
                    DataStore::get_transaction_checkpoint(&self.store, digest)
                        .map_err(to_storage_error),
                )
            })
    }

    /// Reads full checkpoint contents from committed `sui-rpc-store` data only.
    ///
    /// The fork state currently exposes checkpoint summaries and contents, not
    /// full checkpoint payloads.
    fn get_full_checkpoint_contents(
        &self,
        sequence_number: Option<CheckpointSequenceNumber>,
        digest: &CheckpointContentsDigest,
    ) -> Option<VersionedFullCheckpointContents> {
        self.rpc_store
            .get_full_checkpoint_contents(sequence_number, digest)
    }
}

impl RpcStateReader for ForkRpcReader {
    /// Returns the lowest checkpoint with object data through fork state.
    ///
    /// This is remote-chain availability metadata, not fork-local state.
    fn get_lowest_available_checkpoint_objects(&self) -> StorageResult<CheckpointSequenceNumber> {
        DataStore::get_lowest_available_checkpoint_objects(&self.store).map_err(to_storage_error)
    }

    /// Reads the chain identifier, deriving it from fork state when it is missing locally.
    fn get_chain_identifier(&self) -> StorageResult<ChainIdentifier> {
        use_store_on_missing(self.rpc_store.get_chain_identifier(), || {
            self.store.chain_identifier()
        })
    }

    /// Exposes this reader as the RPC index provider.
    fn indexes(&self) -> Option<&dyn RpcIndexes> {
        Some(self)
    }

    /// Reads a struct layout from `sui-rpc-store`.
    fn get_struct_layout_with_overlay(
        &self,
        struct_tag: &StructTag,
        overlay: &ObjectSet,
    ) -> StorageResult<Option<MoveTypeLayout>> {
        match self
            .rpc_store
            .get_struct_layout_with_overlay(struct_tag, overlay)?
        {
            Some(layout) => Ok(Some(layout)),
            None => Ok(None),
        }
    }
}

impl RpcIndexes for ForkRpcReader {
    /// Reads epoch index metadata from `sui-rpc-store`.
    fn get_epoch_info(&self, epoch: EpochId) -> StorageResult<Option<EpochInfo>> {
        match self.rpc_store.get_epoch_info(epoch)? {
            Some(info) => Ok(Some(info)),
            None => Ok(None),
        }
    }

    /// Iterates owned objects from fork-managed RPC-store indexes.
    ///
    /// Owner indexes may require seed-bounded or checkpoint-scoped initialization
    /// before rows are safe to expose.
    fn owned_objects_iter(
        &self,
        owner: SuiAddress,
        object_type: Option<StructTag>,
        cursor: Option<OwnedObjectInfo>,
    ) -> StorageResult<Box<dyn Iterator<Item = Result<OwnedObjectInfo, TypedStoreError>> + '_>>
    {
        self.store.owned_objects_iter(owner, object_type, cursor)
    }

    /// Iterates dynamic fields from fork-managed RPC-store indexes.
    ///
    /// The store ensures the parent object's children have been written into
    /// the object-owner index before returning rows.
    fn dynamic_field_iter(
        &self,
        parent: ObjectID,
        cursor: Option<DynamicFieldKey>,
    ) -> StorageResult<Box<dyn Iterator<Item = DynamicFieldIteratorItem> + '_>> {
        self.store.dynamic_field_iter(parent, cursor)
    }

    /// Reads coin metadata from fork-managed RPC-store indexes.
    ///
    /// The store may initialize the relevant type indexes before assembling the
    /// metadata response.
    fn get_coin_info(&self, coin_type: &StructTag) -> StorageResult<Option<CoinInfo>> {
        self.store.coin_info(coin_type)
    }

    /// Reads an owner's coin balance from fork-managed RPC-store indexes.
    fn get_balance(
        &self,
        owner: &SuiAddress,
        coin_type: &StructTag,
    ) -> StorageResult<Option<BalanceInfo>> {
        self.store.balance(owner, coin_type)
    }

    /// Iterates balances from fork-managed RPC-store indexes.
    fn balance_iter(
        &self,
        owner: &SuiAddress,
        cursor: Option<(SuiAddress, StructTag)>,
    ) -> StorageResult<BalanceIterator<'_>> {
        self.store.balance_iter(owner, cursor)
    }

    /// Iterates package versions from committed `sui-rpc-store` indexes.
    fn package_versions_iter(
        &self,
        original_id: ObjectID,
        cursor: Option<u64>,
    ) -> StorageResult<Box<dyn Iterator<Item = Result<(u64, ObjectID), TypedStoreError>> + '_>>
    {
        RpcIndexes::package_versions_iter(&self.rpc_store, original_id, cursor)
    }

    /// Returns the highest indexed checkpoint from `sui-rpc-store` or fork state.
    fn get_highest_indexed_checkpoint_seq_number(
        &self,
    ) -> StorageResult<Option<CheckpointSequenceNumber>> {
        match self.rpc_store.get_highest_indexed_checkpoint_seq_number()? {
            Some(sequence) => Ok(Some(sequence)),
            None => self.store.highest_indexed_checkpoint_seq_number(),
        }
    }

    /// Reads the transaction sequence-to-digest index from `sui-rpc-store`.
    fn ledger_tx_seq_digest(&self, tx_seq: u64) -> StorageResult<Option<LedgerTxSeqDigest>> {
        RpcIndexes::ledger_tx_seq_digest(&self.rpc_store, tx_seq)
    }

    /// Iterates transaction sequence-to-digest rows from `sui-rpc-store`.
    fn ledger_tx_seq_digest_iter(
        &self,
        start: u64,
        end_exclusive: u64,
        descending: bool,
    ) -> StorageResult<LedgerTxSeqDigestIterator<'_>> {
        RpcIndexes::ledger_tx_seq_digest_iter(&self.rpc_store, start, end_exclusive, descending)
    }

    /// Iterates transaction bitmap buckets from `sui-rpc-store`.
    fn transaction_bitmap_bucket_iter(
        &self,
        dimension_key: Vec<u8>,
        start_bucket: u64,
        end_bucket_exclusive: u64,
        descending: bool,
    ) -> StorageResult<LedgerBitmapBucketIterator<'_>> {
        RpcIndexes::transaction_bitmap_bucket_iter(
            &self.rpc_store,
            dimension_key,
            start_bucket,
            end_bucket_exclusive,
            descending,
        )
    }

    /// Iterates event bitmap buckets from `sui-rpc-store`.
    fn event_bitmap_bucket_iter(
        &self,
        dimension_key: Vec<u8>,
        start_bucket: u64,
        end_bucket_exclusive: u64,
        descending: bool,
    ) -> StorageResult<LedgerBitmapBucketIterator<'_>> {
        RpcIndexes::event_bitmap_bucket_iter(
            &self.rpc_store,
            dimension_key,
            start_bucket,
            end_bucket_exclusive,
            descending,
        )
    }
}

/// Runs a fork-state read only when the primary store reports missing data.
fn use_store_on_missing<T>(
    result: StorageResult<T>,
    store_read: impl FnOnce() -> StorageResult<T>,
) -> StorageResult<T> {
    match result {
        Ok(value) => Ok(value),
        Err(err) if err.kind() == StorageErrorKind::Missing => store_read(),
        Err(err) => Err(err),
    }
}

/// Converts a fallible optional fork-state read into an optional trait response.
///
/// The storage traits using this helper return `Option`, so store errors are
/// logged and treated as absent data.
fn optional_store_read<T>(context: &'static str, result: StorageResult<Option<T>>) -> Option<T> {
    match result {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(context, error = ?err, "fork-state read failed");
            None
        }
    }
}

fn to_storage_error(err: anyhow::Error) -> StorageError {
    StorageError::custom(err.to_string())
}

#[cfg(test)]
mod tests {
    use sui_types::digests::CheckpointDigest;
    use sui_types::effects::TransactionEvents;
    use sui_types::full_checkpoint_content::ExecutedTransaction;
    use sui_types::messages_checkpoint::CheckpointContents;
    use sui_types::messages_checkpoint::VerifiedCheckpoint;
    use sui_types::storage::error::Error as StorageError;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
    use sui_types::transaction::Transaction as SuiTransaction;

    use crate::runtime::ForkRuntime;
    use crate::store::DataStore;

    use super::*;

    fn checkpoint_with_transaction(
        sequence: u64,
    ) -> (VerifiedCheckpoint, CheckpointContents, ExecutedTransaction) {
        let checkpoint = TestCheckpointBuilder::new(sequence)
            .start_transaction(0)
            .finish_transaction()
            .build_checkpoint();
        let executed = checkpoint
            .transactions
            .into_iter()
            .next()
            .expect("checkpoint should have one transaction");
        (
            VerifiedCheckpoint::new_unchecked(checkpoint.summary),
            checkpoint.contents,
            executed,
        )
    }

    fn signed_transaction(executed: &ExecutedTransaction) -> VerifiedTransaction {
        VerifiedTransaction::new_unchecked(SuiTransaction::from_generic_sig_data(
            executed.transaction.clone(),
            executed.signatures.clone(),
        ))
    }

    #[test]
    fn use_store_on_missing_returns_primary_success_without_calling_store() {
        let value = use_store_on_missing(Ok(7), || panic!("store should not be called"))
            .expect("primary value should be returned");

        assert_eq!(value, 7);
    }

    #[test]
    fn use_store_on_missing_calls_store_only_for_missing_errors() {
        let value = use_store_on_missing(Err(StorageError::missing("missing")), || Ok(9))
            .expect("missing errors should use store");

        assert_eq!(value, 9);
    }

    #[test]
    fn use_store_on_missing_propagates_non_missing_errors() {
        let err = use_store_on_missing::<u8>(Err(StorageError::custom("boom")), || Ok(9))
            .expect_err("custom errors should propagate");

        assert_eq!(err.kind(), StorageErrorKind::Custom);
    }

    #[test]
    fn ledger_indexes_delegate_to_rpc_store() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = ForkRuntime::open(
            temp.path(),
            "custom".to_owned(),
            0,
            CheckpointDigest::new([9; 32]).into(),
        )
        .expect("fork runtime should open");
        let store = DataStore::new_for_testing(temp.path().to_path_buf(), runtime.fork_rpc_store());

        let (checkpoint, contents, executed) = checkpoint_with_transaction(1);
        let transaction = signed_transaction(&executed);
        let digest = *transaction.digest();
        let (tx_sequence_number, tx_offset) = contents
            .enumerate_transactions(checkpoint.data())
            .enumerate()
            .find_map(|(offset, (tx_seq, execution))| {
                (execution.transaction == digest).then_some((tx_seq, offset))
            })
            .expect("checkpoint contents should include transaction");
        let tx_offset = u32::try_from(tx_offset).expect("checkpoint offset fits in u32");

        runtime
            .fork_rpc_store()
            .save_checkpoint(&checkpoint, &contents)
            .expect("checkpoint should persist");
        runtime
            .fork_rpc_store()
            .save_transaction(
                &checkpoint,
                &contents,
                &transaction,
                &executed.effects,
                &TransactionEvents::default(),
            )
            .expect("transaction should persist");

        let reader = ForkRpcReader::new(runtime.reader(), store);
        let row = RpcIndexes::ledger_tx_seq_digest(&reader, tx_sequence_number)
            .expect("ledger lookup should read rpc store")
            .expect("ledger row should exist");
        assert_eq!(row.tx_sequence_number, tx_sequence_number);
        assert_eq!(row.digest, digest);
        assert_eq!(row.tx_offset, tx_offset);
        assert_eq!(row.checkpoint_number, checkpoint.data().sequence_number);

        let multi = RpcIndexes::ledger_tx_seq_digest_multi_get(&reader, &[tx_sequence_number])
            .expect("multi-get should use ledger lookup");
        assert_eq!(multi, vec![Some(row)]);

        let rows = RpcIndexes::ledger_tx_seq_digest_iter(
            &reader,
            tx_sequence_number,
            tx_sequence_number + 1,
            false,
        )
        .expect("ledger iterator should read rpc store")
        .collect::<Result<Vec<_>, _>>()
        .expect("ledger iterator should decode rows");
        assert_eq!(rows, vec![row]);

        let transaction_bitmap_rows =
            RpcIndexes::transaction_bitmap_bucket_iter(&reader, vec![1], 0, 1, false)
                .expect("transaction bitmap iterator should read rpc store")
                .collect::<Result<Vec<_>, _>>()
                .expect("transaction bitmap iterator should decode rows");
        assert!(transaction_bitmap_rows.is_empty());

        let event_bitmap_rows = RpcIndexes::event_bitmap_bucket_iter(&reader, vec![1], 0, 1, false)
            .expect("event bitmap iterator should read rpc store")
            .collect::<Result<Vec<_>, _>>()
            .expect("event bitmap iterator should decode rows");
        assert!(event_bitmap_rows.is_empty());
    }
}
