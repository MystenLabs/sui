// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    authority_store::{InternalSequenceNumber, ObjectKey},
    *,
};
use narwhal_executor::ExecutionIndices;
use sui_types::base_types::{ExecutionDigests, SequenceNumber};
use sui_types::batch::{SignedBatch, TxSequenceNumber};
use typed_store::rocks::DBMap;
use typed_store::traits::DBMapTableUtil;
use typed_store_macros::DBMapUtils;

#[derive(DBMapUtils)]
pub struct AuthorityStoreTables<S> {
    /// This is a map between the object (ID, version) and the latest state of the object, namely the
    /// state that is needed to process new transactions. If an object is deleted its entry is
    /// removed from this map.
    ///
    /// Note that while this map can store all versions of an object, in practice it only stores
    /// the most recent version.
    #[options(optimization = "point_lookup")]
    pub(crate) objects: DBMap<ObjectKey, Object>,

    /// This is a an index of object references to currently existing objects, indexed by the
    /// composite key of the SuiAddress of their owner and the object ID of the object.
    /// This composite index allows an efficient iterator to list all objected currently owned
    /// by a specific user, and their object reference.
    pub(crate) owner_index: DBMap<(Owner, ObjectID), ObjectInfo>,

    /// This is map between the transaction digest and transactions found in the `transaction_lock`.
    #[options(optimization = "point_lookup")]
    pub(crate) transactions: DBMap<TransactionDigest, TransactionEnvelope<S>>,

    /// This is a map between the transaction digest and the corresponding certificate for all
    /// certificates that have been successfully processed by this authority. This set of certificates
    /// along with the genesis allows the reconstruction of all other state, and a full sync to this
    /// authority.
    #[options(optimization = "point_lookup")]
    pub(crate) certificates: DBMap<TransactionDigest, CertifiedTransaction>,

    /// The pending execution table holds a sequence of transactions that are present
    /// in the certificates table, but may not have yet been executed, and should be executed.
    /// The source of these certificates might be (1) the checkpoint proposal process (2) the
    /// gossip processes (3) the shared object post-consensus task. An active authority process
    /// reads this table and executes the certificates. The order is a hint as to their
    /// causal dependencies. Note that there is no guarantee digests are unique. Once executed, and
    /// effects are written the entry should be deleted.
    pub(crate) pending_execution: DBMap<InternalSequenceNumber, TransactionDigest>,

    /// The map between the object ref of objects processed at all versions and the transaction
    /// digest of the certificate that lead to the creation of this version of the object.
    ///
    /// When an object is deleted we include an entry into this table for its next version and
    /// a digest of ObjectDigest::deleted(), along with a link to the transaction that deleted it.
    pub(crate) parent_sync: DBMap<ObjectRef, TransactionDigest>,

    /// A map between the transaction digest of a certificate that was successfully processed
    /// (ie in `certificates`) and the effects its execution has on the authority state. This
    /// structure is used to ensure we do not double process a certificate, and that we can return
    /// the same response for any call after the first (ie. make certificate processing idempotent).
    #[options(optimization = "point_lookup")]
    pub(crate) effects: DBMap<TransactionDigest, TransactionEffectsEnvelope<S>>,

    /// Hold the lock for shared objects. These locks are written by a single task: upon receiving a valid
    /// certified transaction from consensus, the authority assigns a lock to each shared objects of the
    /// transaction. Note that all authorities are guaranteed to assign the same lock to these objects.
    /// TODO: These two maps should be merged into a single one (no reason to have two).
    pub(crate) assigned_object_versions: DBMap<(TransactionDigest, ObjectID), SequenceNumber>,
    pub(crate) next_object_versions: DBMap<ObjectID, SequenceNumber>,

    /// Track which transactions have been processed in handle_consensus_transaction. We must be
    /// sure to advance next_object_versions exactly once for each transaction we receive from
    /// consensus. But, we may also be processing transactions from checkpoints, so we need to
    /// track this state separately.
    ///
    /// Entries in this table can be garbage collected whenever we can prove that we won't receive
    /// another handle_consensus_transaction call for the given digest. This probably means at
    /// epoch change.
    pub(crate) consensus_message_processed: DBMap<TransactionDigest, bool>,

    // Tables used for authority batch structure
    /// A sequence on all executed certificates and effects.
    pub executed_sequence: DBMap<TxSequenceNumber, ExecutionDigests>,

    /// A sequence of batches indexing into the sequence of executed transactions.
    pub batches: DBMap<TxSequenceNumber, SignedBatch>,

    /// The following table is used to store a single value (the corresponding key is a constant). The value
    /// represents the index of the latest consensus message this authority processed. This field is written
    /// by a single process acting as consensus (light) client. It is used to ensure the authority processes
    /// every message output by consensus (and in the right order).
    pub(crate) last_consensus_index: DBMap<u64, ExecutionIndices>,
}
