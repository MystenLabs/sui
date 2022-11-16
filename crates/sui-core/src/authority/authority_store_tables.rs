// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{authority_store::ObjectKey, *};
use narwhal_executor::ExecutionIndices;
use rocksdb::Options;
use serde::{Deserialize, Serialize};
use std::path::Path;
use sui_storage::default_db_options;
use sui_types::base_types::{ExecutionDigests, SequenceNumber};
use sui_types::batch::{SignedBatch, TxSequenceNumber};
use sui_types::messages::TrustedCertificate;
use typed_store::rocks::{DBMap, DBOptions};
use typed_store::traits::TypedStoreDebug;

use sui_types::message_envelope::TrustedEnvelope;
use typed_store_derive::DBMapUtils;

/// AuthorityEpochTables contains tables that contain data that is only valid within an epoch.
#[derive(DBMapUtils)]
pub struct AuthorityEpochTables<S> {
    /// This is map between the transaction digest and transactions found in the `transaction_lock`.
    #[default_options_override_fn = "transactions_table_default_config"]
    pub(crate) transactions: DBMap<TransactionDigest, TrustedEnvelope<SenderSignedData, S>>,

    /// Hold the lock for shared objects. These locks are written by a single task: upon receiving a valid
    /// certified transaction from consensus, the authority assigns a lock to each shared objects of the
    /// transaction. Note that all authorities are guaranteed to assign the same lock to these objects.
    /// TODO: These two maps should be merged into a single one (no reason to have two).
    pub(crate) assigned_object_versions: DBMap<(TransactionDigest, ObjectID), SequenceNumber>,
    pub(crate) next_object_versions: DBMap<ObjectID, SequenceNumber>,

    /// Certificates that have been received from clients or received from consensus, but not yet
    /// executed. Entries are cleared after execution.
    /// This table is critical for crash recovery, because usually the consensus output progress
    /// is updated after a certificate is committed into this table.
    ///
    /// If theory, this table may be superseded by storing consensus and checkpoint execution
    /// progress. But it is more complex, because it would be necessary to track inflight
    /// executions not ordered by indices. For now, tracking inflight certificates as a map
    /// seems easier.
    pub(crate) pending_certificates: DBMap<TransactionDigest, TrustedCertificate>,

    /// Track which transactions have been processed in handle_consensus_transaction. We must be
    /// sure to advance next_object_versions exactly once for each transaction we receive from
    /// consensus. But, we may also be processing transactions from checkpoints, so we need to
    /// track this state separately.
    ///
    /// Entries in this table can be garbage collected whenever we can prove that we won't receive
    /// another handle_consensus_transaction call for the given digest. This probably means at
    /// epoch change.
    pub(crate) consensus_message_processed: DBMap<ConsensusTransactionKey, bool>,

    /// Map stores pending transactions that this authority submitted to consensus
    pub(crate) pending_consensus_transactions: DBMap<ConsensusTransactionKey, ConsensusTransaction>,

    /// This is an inverse index for consensus_message_processed - it allows to select
    /// all transactions at the specific consensus range
    ///
    /// The consensus position for the transaction is defined as first position at which valid
    /// certificate for this transaction is seen in consensus
    pub(crate) consensus_message_order: DBMap<ExecutionIndices, TransactionDigest>,

    /// The following table is used to store a single value (the corresponding key is a constant). The value
    /// represents the index of the latest consensus message this authority processed. This field is written
    /// by a single process acting as consensus (light) client. It is used to ensure the authority processes
    /// every message output by consensus (and in the right order).
    pub(crate) last_consensus_index: DBMap<u64, ExecutionIndicesWithHash>,

    /// This table lists all checkpoint boundaries in the consensus sequence
    ///
    /// The key in this table is incremental index and value is corresponding narwhal
    /// consensus output index
    pub(crate) checkpoint_boundary: DBMap<u64, u64>,
}

impl<S> AuthorityEpochTables<S>
where
    S: std::fmt::Debug + Serialize + for<'de> Deserialize<'de>,
{
    pub fn path(epoch: EpochId, parent_path: &Path) -> PathBuf {
        parent_path.join(format!("epoch_{}", epoch))
    }

    pub fn open(epoch: EpochId, parent_path: &Path, db_options: Option<Options>) -> Self {
        Self::open_tables_read_write(Self::path(epoch, parent_path), db_options, None)
    }

    pub fn open_readonly(epoch: EpochId, parent_path: &Path) -> AuthorityEpochTablesReadOnly<S> {
        Self::get_read_only_handle(Self::path(epoch, parent_path), None, None)
    }
}

/// AuthorityPerpetualTables contains data that must be preserved from one epoch to the next.
#[derive(DBMapUtils)]
pub struct AuthorityPerpetualTables<S> {
    /// This is a map between the object (ID, version) and the latest state of the object, namely the
    /// state that is needed to process new transactions.
    ///
    /// Note that while this map can store all versions of an object, we will eventually
    /// prune old object versions from the db.
    ///
    /// IMPORTANT: object versions must *only* be pruned if they appear as inputs in some
    /// TransactionEffects. Simply pruning all objects but the most recent is an error!
    /// This is because there can be partially executed transactions whose effects have not yet
    /// been written out, and which must be retried. But, they cannot be retried unless their input
    /// objects are still accessible!
    #[default_options_override_fn = "objects_table_default_config"]
    pub(crate) objects: DBMap<ObjectKey, Object>,

    /// This is a an index of object references to currently existing objects, indexed by the
    /// composite key of the SuiAddress of their owner and the object ID of the object.
    /// This composite index allows an efficient iterator to list all objected currently owned
    /// by a specific user, and their object reference.
    pub(crate) owner_index: DBMap<(Owner, ObjectID), ObjectInfo>,

    /// This is a map between the transaction digest and the corresponding certificate for all
    /// certificates that have been successfully processed by this authority. This set of certificates
    /// along with the genesis allows the reconstruction of all other state, and a full sync to this
    /// authority.
    #[default_options_override_fn = "certificates_table_default_config"]
    pub(crate) certificates: DBMap<TransactionDigest, TrustedCertificate>,

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
    #[default_options_override_fn = "effects_table_default_config"]
    pub(crate) effects: DBMap<TransactionDigest, TransactionEffectsEnvelope<S>>,

    // Tables used for authority batch structure
    // TODO: executed_sequence and batches both conceptually belong in AuthorityEpochTables,
    // but we currently require that effects and executed_sequence are written atomically.
    // See https://github.com/MystenLabs/sui/pull/4395 for the reason why.
    //
    // This can be addressed when we do the WAL rework. Something similar to the following flow
    // would be required:
    // 1. First execute the tx and store the outputs in an intermediate location.
    // 2. Note that execution has finished (e.g. in the WAL.)
    // 3. Write intermediate outputs to their permanent locations.
    // 4. Mark the tx as finished in the WAL.
    // 5. Crucially: If step 3 is interrupted, we must restart at step 3 based solely on the fact
    //    that the WAL indicates the tx is not written yet. This fixes the root cause of the issue,
    //    which is that we currently exit early if effects have been written.
    /// A sequence on all executed certificates and effects.
    pub executed_sequence: DBMap<TxSequenceNumber, ExecutionDigests>,

    /// A sequence of batches indexing into the sequence of executed transactions.
    pub batches: DBMap<TxSequenceNumber, SignedBatch>,

    pub reconfig_state: DBMap<u64, ReconfigState>,
}

impl<S> AuthorityPerpetualTables<S>
where
    S: std::fmt::Debug + Serialize + for<'de> Deserialize<'de>,
{
    pub fn path(parent_path: &Path) -> PathBuf {
        parent_path.join("perpetual")
    }

    pub fn open(parent_path: &Path, db_options: Option<Options>) -> Self {
        Self::open_tables_read_write(Self::path(parent_path), db_options, None)
    }

    pub fn open_readonly(parent_path: &Path) -> AuthorityPerpetualTablesReadOnly<S> {
        Self::get_read_only_handle(Self::path(parent_path), None, None)
    }

    /// Read an object and return it, or Err(ObjectNotFound) if the object was not found.
    pub fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        let obj_entry = self
            .objects
            .iter()
            .skip_prior_to(&ObjectKey::max_for_id(object_id))?
            .next();

        let obj = match obj_entry {
            Some((ObjectKey(obj_id, _), obj)) if obj_id == *object_id => obj,
            _ => return Ok(None),
        };

        // Note that the two reads in this function are (obviously) not atomic, and the
        // object may be deleted after we have read it. Hence we check get_latest_parent_entry
        // last, so that the write to self.parent_sync gets the last word.
        //
        // TODO: verify this race is ok.
        //
        // I believe it is - Even if the reads were atomic, calls to this function would still
        // race with object deletions (the object could be deleted between when the function is
        // called and when the first read takes place, which would be indistinguishable to the
        // caller with the case in which the object is deleted in between the two reads).
        let parent_entry = self.get_latest_parent_entry(*object_id)?;

        match parent_entry {
            None => {
                error!(
                    ?object_id,
                    "Object is missing parent_sync entry, data store is inconsistent"
                );
                Ok(None)
            }
            Some((obj_ref, _)) if obj_ref.2.is_alive() => Ok(Some(obj)),
            _ => Ok(None),
        }
    }

    pub fn get_latest_parent_entry(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<(ObjectRef, TransactionDigest)>, SuiError> {
        let mut iterator = self
            .parent_sync
            .iter()
            // Make the max possible entry for this object ID.
            .skip_prior_to(&(object_id, SequenceNumber::MAX, ObjectDigest::MAX))?;

        Ok(iterator.next().and_then(|(obj_ref, tx_digest)| {
            if obj_ref.0 == object_id {
                Some((obj_ref, tx_digest))
            } else {
                None
            }
        }))
    }

    pub fn get_sui_system_state_object(&self) -> SuiResult<SuiSystemState> {
        let sui_system_object = self
            .get_object(&SUI_SYSTEM_STATE_OBJECT_ID)?
            .expect("Sui System State object must always exist");
        let move_object = sui_system_object
            .data
            .try_as_move()
            .expect("Sui System State object must be a Move object");
        let result = bcs::from_bytes::<SuiSystemState>(move_object.contents())
            .expect("Sui System State object deserialization cannot fail");
        Ok(result)
    }

    pub fn get_epoch(&self) -> SuiResult<EpochId> {
        Ok(self.get_sui_system_state_object()?.epoch)
    }

    pub fn database_is_empty(&self) -> SuiResult<bool> {
        Ok(self
            .objects
            .iter()
            .skip_to(&ObjectKey::ZERO)?
            .next()
            .is_none())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionIndicesWithHash {
    pub index: ExecutionIndices,
    pub hash: u64,
}

// These functions are used to initialize the DB tables
fn objects_table_default_config() -> DBOptions {
    default_db_options(None, None).1
}
fn transactions_table_default_config() -> DBOptions {
    default_db_options(None, None).1
}
fn certificates_table_default_config() -> DBOptions {
    default_db_options(None, None).1
}
fn effects_table_default_config() -> DBOptions {
    default_db_options(None, None).1
}
