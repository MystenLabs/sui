// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use sui_types::{
    digests::{TransactionDigest, TransactionEffectsDigest},
    error::SuiResult,
    storage::ObjectKey,
};

/// The result of reading an object for execution. Because shared objects may be deleted, one
/// possible result of calling ExecutionCache::notify_read_objects_for_execution on a shared object
/// is that ObjectReadResult::Deleted is returned.
pub enum ObjectReadResult {
    Object(Arc<Object>),
    Deleted,
}

/// ExecutionCache is intended to provide an in-memory, write-behind or write-through cache used
/// by transaction signing and execution.
///
/// Note that except where specified below, we do not provide any durability guarantees.
/// Crash recovery is done by re-execution of transactions when necessary.
#[async_trait]
pub trait ExecutionCache {
    /// Read the inputs for a transaction that the validator was asked to sign.
    /// tx_digest is provided so that the inputs can be cached with the tx_digest and returned with
    /// a single hash map lookup when notify_read_objects_for_execution is called later.
    async fn notify_read_objects_for_signing(
        &self,
        tx_digest: &TransactionDigest,
        objects: &[InputObjectKind],
        timeout: Duration,
    ) -> SuiResult<Vec<Arc<Object>>>;

    /// Attempt to acquire locks on the mutable_input_objects for the transaction.
    /// This must not be called until after ownership checks have passed.
    ///
    /// If all locks can be acquired, return success.
    ///
    /// Note that if any lock cannot be acquired, some other locks may be left in the locked state.
    /// No reverts are guaranteed to be performed. This is safe, because the only case in which
    /// there can be contention is if the user has equivocated (signed conflicting transactions).
    /// In this case, the user is not guaranteed to be able to execute either transaction anyway.
    ///
    /// Further, if two conflicting transactions are racing with each other to acquire locks, there
    /// is no guarantee that either one will succeed, for the same reason.
    ///
    /// This method is durable: After this method returns, no other transaction
    /// can ever acquire any of the locks, even if we crash immediately after returning.
    async fn lock_transaction(
        &self,
        signed_transaction: VerifiedSignedTransaction,
        mutable_input_objects: &[ObjectRef],
    ) -> SuiResult;

    /// Read the inputs for a given transaction.
    /// As this method has no timeout, it should only be used for certificate execution, because
    /// certificate inputs are guaranteed to exist eventually.
    ///
    /// When this function returns, it is guaranteed that a read of any child object of any of the
    /// specified inputs will return the correct version - in other words, this reader cannot
    /// observe a write of a root object unless all the writes of that object's children are also
    /// observable.
    ///
    /// The tx_digest is provided here to support the following optimization: All the owned input objects
    /// will likely have been loaded during transaction signing, and can be stored as a group with
    /// the transaction_digest as the key, allowing the lookup to proceed with only a single hash
    /// map lookup. (additional lookups may be necessary for shared inputs, since the versions are
    /// not known at signing time).
    async fn notify_read_objects_for_execution(
        &self,
        tx_digest: &TransactionDigest,
        objects: &[ObjectKey],
    ) -> SuiResult<Vec<ObjectReadResult>>;

    /// Read a child object. The version_bound is the highest version that should be observable by
    /// this reader. It must be derived from the root-owner's version by the runtime.
    /// This must be called after a notify_read_objects() call for the root object has returned, in
    /// order to guarantee that all writes to the child object for versions <= version_bound are
    /// visible.
    ///
    /// This method is synchronous because it is called by the object runtime during execution.
    fn read_child_object(
        &self,
        tx_digest: &TransactionDigest,
        object: &ObjectID,
        version_bound: SequenceNumber,
    ) -> SuiResult<Arc<Object>>;

    /// Advise the cache that the specified objects may be used soon.
    /// The intended use case is to prefetch owned object inputs of shared-object transactions as
    /// soon as those transactions are observed via consensus.
    ///
    /// This is an optional method, since it's just an optimization.
    fn prefetch_objects(&self, tx_digest: &TransactionDigest, objects: &[ObjectKey]) {}

    /// Write the output of a transaction.
    /// Because of the child object consistency rule (readers that observe parents must observe all
    /// children of that parent, up to the parent's version bound), implementations of this method
    /// must not write any top-level (address-owned or shared) objects before they have written all
    /// of the object-owned objects in the `objects` list.
    ///
    /// In the future, we may modify this method to expose finer-grained information about
    /// parent/child relationships. (This may be especially necessary for distributed object
    /// storage, but is unlikely to be an issue before we tackle that problem).
    ///
    /// This function should normally return synchronously. However, it is async in order to
    /// allow the cache to implement backpressure. If writes cannot be flushed to durable storage
    /// as quickly as they are arriving via this method, then we may have to wait for the write to
    /// complete.
    ///
    /// This function may evict the mutable input objects (and successfully received objects) of
    /// transaction from the cache, since they cannot be read by any other transaction.
    ///
    /// Any write performed by this method immediately notifies any waiter that has previously
    /// called notify_read_objects_for_execution or notify_read_objects_for_signing for the object
    /// in question.
    async fn write_transaction_outputs(
        &self,
        inner_temporary_store: InnerTemporaryStore,
        effects: &TransactionEffects,
        transaction: &VerifiedTransaction,
        epoch_id: EpochId,
    ) -> SuiResult;

    /// Read the effects digest for the given tx. When this (or any of the other effects methods)
    /// returns, the effects and other outputs of the transaction must be durable.
    ///
    /// Intended to be called before returning a SignedTransactionEffects, or before sending a
    /// checkpoint signature to consensus.
    async fn notify_read_effects_digest(
        &self,
        tx_digest: &TransactionDigest,
    ) -> SuiResult<TransactionEffectsDigest>;

    /// Read the effects for the given tx.
    async fn read_effects(
        &self,
        tx_digest: &TransactionDigest,
    ) -> SuiResult<Option<TransactionEffects>>;

    /// See comments on notify_read_effects_digest.
    async fn notify_read_effects(
        &self,
        tx_digest: &TransactionDigest,
    ) -> SuiResult<TransactionEffects> {
        let effects_digest = self.notify_read_effects_digest(tx_digest).await?;
        self.read_effects(tx_digest, effects_digest)
            .await
            .map(|effects| effects.expect("effects must exist"))
    }
}
