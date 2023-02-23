// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::authority_store::LockDetails;
use rocksdb::Options;
use std::path::Path;
use sui_storage::default_db_options;
use sui_types::base_types::SequenceNumber;
use sui_types::error::SuiError::StorageError;
use sui_types::messages::TrustedCertificate;
use typed_store::metrics::SamplingInterval;
use typed_store::rocks::{DBMap, DBOptions, MetricConf, ReadWriteOptions};
use typed_store::traits::{Map, TableSummary, TypedStoreDebug};

use typed_store_derive::DBMapUtils;

const CURRENT_EPOCH_KEY: u64 = 0;

/// AuthorityPerpetualTables contains data that must be preserved from one epoch to the next.
#[derive(DBMapUtils)]
pub struct AuthorityPerpetualTables {
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
    pub objects: DBMap<ObjectKey, Object>,

    /// This is a map between object references of currently active objects that can be mutated,
    /// and the transaction that they are lock on for use by this specific authority. Where an object
    /// lock exists for an object version, but no transaction has been seen using it the lock is set
    /// to None. The safety of consistent broadcast depend on each honest authority never changing
    /// the lock once it is set. After a certificate for this object is processed it can be
    /// forgotten.
    #[default_options_override_fn = "owned_object_transaction_locks_table_default_config"]
    pub(crate) owned_object_transaction_locks: DBMap<ObjectRef, Option<LockDetails>>,

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
    pub parent_sync: DBMap<ObjectRef, TransactionDigest>,

    /// A map between the transaction digest of a certificate to the effects of its execution.
    /// We store effects into this table in two different cases:
    /// 1. When a transaction is synced through state_sync, we store the effects here. These effects
    /// are known to be final in the network, but may not have been executed locally yet.
    /// 2. When the transaction is executed locally on this node, we store the effects here. This means that
    /// it's possible to store the same effects twice (once for the synced transaction, and once for the executed).
    /// It's also possible for the effects to be reverted if the transaction didn't make it into the epoch.
    #[default_options_override_fn = "effects_table_default_config"]
    pub(crate) effects: DBMap<TransactionEffectsDigest, TransactionEffects>,

    /// Transactions that have been executed locally on this node. We need this table since the `effects` table
    /// doesn't say anything about the execution status of the transaction on this node. When we wait for transactions
    /// to be executed, we wait for them to appear in this table. When we revert transactions, we remove them from both
    /// tables.
    pub(crate) executed_effects: DBMap<TransactionDigest, TransactionEffectsDigest>,

    pub(crate) synced_transactions: DBMap<TransactionDigest, TrustedCertificate>,

    /// When transaction is executed via checkpoint executor, we store association here
    pub(crate) executed_transactions_to_checkpoint:
        DBMap<TransactionDigest, (EpochId, CheckpointSequenceNumber)>,

    /// A singleton table that stores the current epoch number. This is used only for the purpose of
    /// crash recovery so that when we restart we know which epoch we are at. This is needed because
    /// there will be moments where the on-chain epoch doesn't match with the per-epoch table epoch.
    /// This number should match the epoch of the per-epoch table in the authority store.
    current_epoch: DBMap<u64, u64>,
}

impl AuthorityPerpetualTables {
    pub fn path(parent_path: &Path) -> PathBuf {
        parent_path.join("perpetual")
    }

    pub fn open(parent_path: &Path, db_options: Option<Options>) -> Self {
        Self::open_tables_read_write(
            Self::path(parent_path),
            MetricConf::with_sampling(SamplingInterval::new(Duration::from_secs(60), 0)),
            db_options,
            None,
        )
    }

    pub fn open_readonly(parent_path: &Path) -> AuthorityPerpetualTablesReadOnly {
        Self::get_read_only_handle(Self::path(parent_path), None, None, MetricConf::default())
    }

    pub fn get_object_by_ref(&self, object_ref: &ObjectRef) -> SuiResult<Option<Object>> {
        self.objects
            .get(&ObjectKey(object_ref.0, object_ref.1))
            .map_err(StorageError)
    }

    /// Read an object and return it, or Ok(None) if the object was not found.
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

    // This is used by indexer to find the correct version of dynamic field child object.
    // We do not store the version of the child object, but because of lamport timestamp,
    // we know the child must have version number less then or eq to the parent.
    pub fn find_object_lt_or_eq_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Option<Object> {
        let Ok(iter) = self.objects
            .iter()
            .skip_prior_to(&ObjectKey(object_id, version))else {
            return None
        };
        iter.reverse().next().map(|(_, o)| o)
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

    pub fn get_recovery_epoch_at_restart(&self) -> SuiResult<EpochId> {
        Ok(self
            .current_epoch
            .get(&CURRENT_EPOCH_KEY)?
            .expect("Must have current epoch."))
    }

    pub fn set_recovery_epoch(&self, epoch: EpochId) -> SuiResult {
        self.current_epoch.insert(&CURRENT_EPOCH_KEY, &epoch)?;
        Ok(())
    }

    pub fn database_is_empty(&self) -> SuiResult<bool> {
        Ok(self
            .objects
            .iter()
            .skip_to(&ObjectKey::ZERO)?
            .next()
            .is_none())
    }

    pub fn iter_live_object_set(&self) -> LiveSetIter<'_> {
        LiveSetIter {
            iter: self.parent_sync.keys(),
            prev: None,
        }
    }
}

pub struct LiveSetIter<'a> {
    iter: <DBMap<ObjectRef, TransactionDigest> as Map<'a, ObjectRef, TransactionDigest>>::Keys,
    prev: Option<ObjectRef>,
}

impl Iterator for LiveSetIter<'_> {
    type Item = ObjectRef;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(next) = self.iter.next() {
                let prev = self.prev;
                self.prev = Some(next);

                match prev {
                    Some(prev) if prev.0 != next.0 && prev.2.is_alive() => return Some(prev),
                    _ => continue,
                }
            }

            return match self.prev {
                Some(prev) if prev.2.is_alive() => {
                    self.prev = None;
                    Some(prev)
                }
                _ => None,
            };
        }
    }
}

// These functions are used to initialize the DB tables
fn owned_object_transaction_locks_table_default_config() -> DBOptions {
    default_db_options(None, None).1
}
fn objects_table_default_config() -> DBOptions {
    let db_options = default_db_options(None, None).1;
    DBOptions {
        options: db_options.options,
        rw_options: ReadWriteOptions {
            ignore_range_deletions: true,
        },
    }
}
fn certificates_table_default_config() -> DBOptions {
    default_db_options(None, None).1
}
fn effects_table_default_config() -> DBOptions {
    default_db_options(None, None).1
}
