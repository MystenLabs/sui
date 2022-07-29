// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    authority_store::{InternalSequenceNumber, ObjectKey},
    *,
};
use narwhal_executor::ExecutionIndices;
use rocksdb::Options;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::Path;
use sui_storage::default_db_options;
use sui_types::base_types::{ExecutionDigests, SequenceNumber};
use sui_types::batch::{SignedBatch, TxSequenceNumber};
use typed_store::rocks::DBMap;
use typed_store::{reopen, traits::Map};

const OBJECTS_TABLE_NAME: &str = "objects";
const OWNER_INDEX_TABLE_NAME: &str = "owner_index";
const TX_TABLE_NAME: &str = "transactions";
const CERTS_TABLE_NAME: &str = "certificates";
const PENDING_EXECUTION: &str = "pending_execution";
const PARENT_SYNC_TABLE_NAME: &str = "parent_sync";
const EFFECTS_TABLE_NAME: &str = "effects";
const ASSIGNED_OBJECT_VERSIONS_TABLE_NAME: &str = "assigned_object_versions";
const NEXT_OBJECT_VERSIONS_TABLE_NAME: &str = "next_object_versions";
const CONSENSUS_MESSAGE_PROCESSED_TABLE_NAME: &str = "consensus_message_processed";
const EXEC_SEQ_TABLE_NAME: &str = "executed_sequence";
const BATCHES_TABLE_NAME: &str = "batches";
const LAST_CONSENSUS_TABLE_NAME: &str = "last_consensus_index";
const EPOCH_TABLE_NAME: &str = "epochs";

pub struct StoreTables<S> {
    /// This is a map between the object (ID, version) and the latest state of the object, namely the
    /// state that is needed to process new transactions. If an object is deleted its entry is
    /// removed from this map.
    ///
    /// Note that while this map can store all versions of an object, in practice it only stores
    /// the most recent version.
    pub(crate) objects: DBMap<ObjectKey, Object>,

    /// This is a an index of object references to currently existing objects, indexed by the
    /// composite key of the SuiAddress of their owner and the object ID of the object.
    /// This composite index allows an efficient iterator to list all objected currently owned
    /// by a specific user, and their object reference.
    pub(crate) owner_index: DBMap<(Owner, ObjectID), ObjectInfo>,

    /// This is map between the transaction digest and transactions found in the `transaction_lock`.
    pub(crate) transactions: DBMap<TransactionDigest, TransactionEnvelope<S>>,

    /// This is a map between the transaction digest and the corresponding certificate for all
    /// certificates that have been successfully processed by this authority. This set of certificates
    /// along with the genesis allows the reconstruction of all other state, and a full sync to this
    /// authority.
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

    /// Map from each epoch ID to the epoch information. The epoch is either signed by this node,
    /// or is certified (signed by a quorum).
    pub(crate) epochs: DBMap<EpochId, AuthenticatedEpoch>,
}
impl<S: Eq + Debug + Serialize + for<'de> Deserialize<'de>> StoreTables<S> {
    /// If with_secondary_path is set, the DB is opened in read only mode with the path specified
    pub fn open_impl<P: AsRef<Path>>(
        path: P,
        db_options: Option<Options>,
        with_secondary_path: Option<P>,
    ) -> Self {
        let (options, point_lookup) = default_db_options(db_options, None);

        let db = {
            let path = &path;
            let db_options = Some(options.clone());
            let opt_cfs: &[(&str, &rocksdb::Options)] = &[
                (OBJECTS_TABLE_NAME, &point_lookup),
                (TX_TABLE_NAME, &point_lookup),
                (OWNER_INDEX_TABLE_NAME, &options),
                (CERTS_TABLE_NAME, &point_lookup),
                (PENDING_EXECUTION, &options),
                (PARENT_SYNC_TABLE_NAME, &options),
                (EFFECTS_TABLE_NAME, &point_lookup),
                (ASSIGNED_OBJECT_VERSIONS_TABLE_NAME, &options),
                (NEXT_OBJECT_VERSIONS_TABLE_NAME, &options),
                (CONSENSUS_MESSAGE_PROCESSED_TABLE_NAME, &options),
                (EXEC_SEQ_TABLE_NAME, &options),
                (BATCHES_TABLE_NAME, &options),
                (LAST_CONSENSUS_TABLE_NAME, &options),
                (EPOCH_TABLE_NAME, &point_lookup),
            ];
            if let Some(p) = with_secondary_path {
                typed_store::rocks::open_cf_opts_secondary(path, Some(&p), db_options, opt_cfs)
            } else {
                typed_store::rocks::open_cf_opts(path, db_options, opt_cfs)
            }
        }
        .expect("Cannot open DB.");

        let executed_sequence =
            DBMap::reopen(&db, Some(EXEC_SEQ_TABLE_NAME)).expect("Cannot open CF.");

        let (
            objects,
            owner_index,
            transactions,
            certificates,
            pending_execution,
            parent_sync,
            effects,
            assigned_object_versions,
            next_object_versions,
            consensus_message_processed,
            batches,
            last_consensus_index,
            epochs,
        ) = reopen! (
            &db,
            OBJECTS_TABLE_NAME;<ObjectKey, Object>,
            OWNER_INDEX_TABLE_NAME;<(Owner, ObjectID), ObjectInfo>,
            TX_TABLE_NAME;<TransactionDigest, TransactionEnvelope<S>>,
            CERTS_TABLE_NAME;<TransactionDigest, CertifiedTransaction>,
            PENDING_EXECUTION;<InternalSequenceNumber, TransactionDigest>,
            PARENT_SYNC_TABLE_NAME;<ObjectRef, TransactionDigest>,
            EFFECTS_TABLE_NAME;<TransactionDigest, TransactionEffectsEnvelope<S>>,
            ASSIGNED_OBJECT_VERSIONS_TABLE_NAME;<(TransactionDigest, ObjectID), SequenceNumber>,
            NEXT_OBJECT_VERSIONS_TABLE_NAME;<ObjectID, SequenceNumber>,
            CONSENSUS_MESSAGE_PROCESSED_TABLE_NAME;<TransactionDigest, bool>,
            BATCHES_TABLE_NAME;<TxSequenceNumber, SignedBatch>,
            LAST_CONSENSUS_TABLE_NAME;<u64, ExecutionIndices>,
            EPOCH_TABLE_NAME;<EpochId, AuthenticatedEpoch>
        );

        Self {
            objects,
            owner_index,
            transactions,
            certificates,
            pending_execution,
            parent_sync,
            effects,
            assigned_object_versions,
            next_object_versions,
            consensus_message_processed,
            executed_sequence,
            batches,
            last_consensus_index,
            epochs,
        }
    }

    /// Open an authority store by directory path in read-write mode
    pub fn open_read_write<P: AsRef<Path>>(path: P, db_options: Option<Options>) -> Self {
        Self::open_impl(path, db_options, None)
    }

    /// Open an authority store by directory path in read only mode
    pub fn open_read_only<P: AsRef<Path>>(
        path: P,
        secondary_path: P,
        db_options: Option<Options>,
    ) -> Self {
        Self::open_impl(path, db_options, Some(secondary_path))
    }

    // TODO: condense with macros
    pub fn dump(&self, table_name: &str) -> anyhow::Result<BTreeMap<String, String>> {
        Ok(match table_name {
            OBJECTS_TABLE_NAME => {
                self.objects.try_catch_up_with_primary()?;
                self.objects
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                    .collect::<BTreeMap<_, _>>()
            }
            OWNER_INDEX_TABLE_NAME => {
                self.owner_index.try_catch_up_with_primary()?;

                self.owner_index
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                    .collect::<BTreeMap<_, _>>()
            }
            TX_TABLE_NAME => {
                self.transactions.try_catch_up_with_primary()?;
                self.transactions
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                    .collect::<BTreeMap<_, _>>()
            }

            CERTS_TABLE_NAME => {
                self.certificates.try_catch_up_with_primary()?;
                self.certificates
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                    .collect::<BTreeMap<_, _>>()
            }

            PENDING_EXECUTION => {
                self.pending_execution.try_catch_up_with_primary()?;
                self.pending_execution
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                    .collect::<BTreeMap<_, _>>()
            }

            PARENT_SYNC_TABLE_NAME => {
                self.parent_sync.try_catch_up_with_primary()?;
                self.parent_sync
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                    .collect::<BTreeMap<_, _>>()
            }

            EFFECTS_TABLE_NAME => {
                self.effects.try_catch_up_with_primary()?;
                self.effects
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                    .collect::<BTreeMap<_, _>>()
            }

            ASSIGNED_OBJECT_VERSIONS_TABLE_NAME => {
                self.assigned_object_versions.try_catch_up_with_primary()?;
                self.assigned_object_versions
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                    .collect::<BTreeMap<_, _>>()
            }

            NEXT_OBJECT_VERSIONS_TABLE_NAME => {
                self.next_object_versions.try_catch_up_with_primary()?;
                self.next_object_versions
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                    .collect::<BTreeMap<_, _>>()
            }

            CONSENSUS_MESSAGE_PROCESSED_TABLE_NAME => {
                self.consensus_message_processed
                    .try_catch_up_with_primary()?;
                self.consensus_message_processed
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                    .collect::<BTreeMap<_, _>>()
            }

            EXEC_SEQ_TABLE_NAME => {
                self.executed_sequence.try_catch_up_with_primary()?;
                self.executed_sequence
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                    .collect::<BTreeMap<_, _>>()
            }

            BATCHES_TABLE_NAME => {
                self.batches.try_catch_up_with_primary()?;
                self.batches
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                    .collect::<BTreeMap<_, _>>()
            }

            LAST_CONSENSUS_TABLE_NAME => {
                self.last_consensus_index.try_catch_up_with_primary()?;
                self.last_consensus_index
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                    .collect::<BTreeMap<_, _>>()
            }

            EPOCH_TABLE_NAME => {
                self.epochs.try_catch_up_with_primary()?;
                self.epochs
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                    .collect::<BTreeMap<_, _>>()
            }
            _ => anyhow::bail!("No such table name: {}", table_name),
        })
    }
}
