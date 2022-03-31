// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! lock_service is a single-threaded atomic Sui Object locking service.
//! Object locks have three phases:
//! 1. (object has no lock, doesn't exist)
//! 2. None (object has an empty lock, but exists. The state when a new object is created)
//! 3. Locked (object has a Transaction digest in the lock, so it's only usable by that transaction)
//!
//! The cycle goes from None (object creation) -> Locked -> deleted/doesn't exist after a Transaction.
//!
//! Lock state is persisted in RocksDB and should be consistent.
//!
//! Communication with the lock service happens through a MPSC queue/channel.

use rocksdb::Options;
use std::path::Path;
use tracing::debug;
use typed_store::rocks::DBMap;
use typed_store::{reopen, traits::Map};

use sui_types::base_types::{ObjectRef, TransactionDigest};
use sui_types::error::SuiError;

/// Atomic Sui Object locking service.
/// Primary abstraction is an atomic op to acquire a lock on a given set of objects.
/// Atomicity relies on single threaded loop and only one instance per authority.
struct LockService {
    /// This is a map between object references of currently active objects that can be mutated,
    /// and the transaction that they are lock on for use by this specific authority. Where an object
    /// lock exists for an object version, but no transaction has been seen using it the lock is set
    /// to None. The safety of consistent broadcast depend on each honest authority never changing
    /// the lock once it is set. After a certificate for this object is processed it can be
    /// forgotten.
    transaction_lock: DBMap<ObjectRef, Option<TransactionDigest>>,
}

// TODO: Create method needs to make sure only one instance or thread of this is running per authority
// If not for multiple authorities per process, it should really be one per process.
impl LockService {
    /// Open or create a new LockService database
    fn try_open_db<P: AsRef<Path>>(path: P, db_options: Option<Options>) -> Result<Self, SuiError> {
        let mut options = db_options.unwrap_or_default();

        /* The table cache is locked for updates and this determines the number
           of shareds, ie 2^10. Increase in case of lock contentions.
        */
        let row_cache = rocksdb::Cache::new_lru_cache(300_000).expect("Cache is ok");
        options.set_row_cache(&row_cache);
        options.set_table_cache_num_shard_bits(10);
        options.set_compression_type(rocksdb::DBCompressionType::None);

        let mut point_lookup = options.clone();
        point_lookup.optimize_for_point_lookup(1024 * 1024);
        point_lookup.set_memtable_whole_key_filtering(true);

        let transform = rocksdb::SliceTransform::create("bytes_8_to_16", |key| &key[8..16], None);
        point_lookup.set_prefix_extractor(transform);
        point_lookup.set_memtable_prefix_bloom_ratio(0.2);

        let db = {
            let path = &path;
            let db_options = Some(options.clone());
            let opt_cfs: &[(&str, &rocksdb::Options)] = &[("transaction_lock", &point_lookup)];
            typed_store::rocks::open_cf_opts(path, db_options, opt_cfs)
        }
        .map_err(SuiError::StorageError)?;

        let transaction_lock =
            reopen!(&db, "transaction_lock";<ObjectRef, Option<TransactionDigest>>);

        Ok(Self { transaction_lock })
    }

    /// Acquires a lock for a transaction on the given objects if they have all been initialized previously
    /// to None state.  The locks are all set to the given transacton digest.
    /// Otherwise, SuiError(TransactionLockDoesNotExist, ConflictingTransaction) is returned.
    fn acquire_locks(
        &self,
        owned_input_objects: &[ObjectRef],
        tx_digest: TransactionDigest,
    ) -> Result<(), SuiError> {
        let mut locks_to_write = Vec::new();
        let locks = self.transaction_lock.multi_get(owned_input_objects)?;

        for (i, lock) in locks.iter().enumerate() {
            // The object / version must exist, and therefore lock initialized.
            let lock = lock.ok_or(SuiError::TransactionLockDoesNotExist)?;

            if let Some(previous_tx_digest) = lock {
                // Lock already set to different transaction
                if previous_tx_digest != tx_digest {
                    // TODO: add metrics here
                    debug!(prev_tx_digest =? previous_tx_digest,
                          cur_tx_digest =? tx_digest,
                          "Conflicting transaction!  Lock state changed in unexpected way");
                    return Err(SuiError::ConflictingTransaction {
                        pending_transaction: previous_tx_digest,
                    });
                }
            } else {
                // Only write the locks that need to be written (are uninitialized)
                let obj_ref = owned_input_objects[i];
                locks_to_write.push((obj_ref, Some(tx_digest)));
            }
        }

        if !locks_to_write.is_empty() {
            self.transaction_lock
                .batch()
                .insert_batch(&self.transaction_lock, locks_to_write)?
                .write()?;
        }

        Ok(())
    }

    /// Initialize a lock to None (but exists) for a given list of ObjectRefs.
    /// If the lock already exists and is locked to a transaction, then return TransactionLockExists
    fn initialize_locks(&self, objects: &[ObjectRef]) -> Result<(), SuiError> {
        // Use a multiget for efficiency
        let locks = self.transaction_lock.multi_get(objects)?;

        // If any locks exist and are not None, return errors for them
        let existing_locks: Vec<ObjectRef> = locks
            .iter()
            .zip(objects)
            .filter_map(|(lock_opt, objref)| {
                if let Some(Some(_tx_digest)) = lock_opt {
                    Some(*objref)
                } else {
                    None
                }
            })
            .collect();
        if !existing_locks.is_empty() {
            return Err(SuiError::TransactionLockExists {
                refs: existing_locks,
            });
        }

        // Insert where locks don't exist already
        let refs_to_insert = locks.iter().zip(objects).filter_map(|(lock_opt, objref)| {
            if lock_opt.is_none() {
                Some((objref, None))
            } else {
                None
            }
        });
        self.transaction_lock.multi_insert(refs_to_insert)?;

        Ok(())
    }

    /// Removes locks for a given list of ObjectRefs.
    fn delete_locks(&self, objects: &[ObjectRef]) -> Result<(), SuiError> {
        self.transaction_lock.multi_remove(objects)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_types::base_types::{ObjectDigest, ObjectID, ObjectRef, TransactionDigest};
    use sui_types::error::SuiError;

    fn init_lockservice() -> LockService {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("DB_{:?}", ObjectID::random()));
        std::fs::create_dir(&path).unwrap();

        LockService::try_open_db(path, None).expect("Could not create LockDB")
    }

    #[test]
    // Test acquire_locks() and initialize_locks()
    fn test_lockdb_acquire_init_multiple() {
        let ls = init_lockservice();

        let ref1: ObjectRef = (ObjectID::random(), 1.into(), ObjectDigest::random());
        let ref2: ObjectRef = (ObjectID::random(), 1.into(), ObjectDigest::random());
        let ref3: ObjectRef = (ObjectID::random(), 1.into(), ObjectDigest::random());

        let tx1 = TransactionDigest::random();
        let tx2 = TransactionDigest::random();

        // Should not be able to acquire lock for uninitialized locks
        assert!(matches!(
            ls.acquire_locks(&[ref1, ref2], tx1),
            Err(SuiError::TransactionLockDoesNotExist)
        ));

        // Initialize 2 locks
        ls.initialize_locks(&[ref1, ref2]).unwrap();

        // Should not be able to acquire lock if not all objects initialized
        assert_eq!(
            ls.acquire_locks(&[ref1, ref2, ref3], tx1),
            Err(SuiError::TransactionLockDoesNotExist)
        );

        // Should be able to acquire lock if all objects initialized
        ls.acquire_locks(&[ref1, ref2], tx1).unwrap();

        // Should get TransactionLockExists if try to initialize already locked object
        assert!(matches!(
            ls.initialize_locks(&[ref2, ref3]),
            Err(SuiError::TransactionLockExists { .. })
        ));

        // Should not be able to acquire lock for diff tx if already locked
        ls.initialize_locks(&[ref3]).unwrap();
        assert!(matches!(
            ls.acquire_locks(&[ref2, ref3], tx2),
            Err(SuiError::ConflictingTransaction { .. })
        ));
    }

    #[test]
    fn test_lockdb_remove_multiple() {
        let ls = init_lockservice();

        let ref1: ObjectRef = (ObjectID::random(), 1.into(), ObjectDigest::random());
        let ref2: ObjectRef = (ObjectID::random(), 1.into(), ObjectDigest::random());

        let tx1 = TransactionDigest::random();

        // Initialize 2 locks
        ls.initialize_locks(&[ref1, ref2]).unwrap();

        // Should be able to acquire lock if all objects initialized
        ls.acquire_locks(&[ref1, ref2], tx1).unwrap();

        // Cannot initialize them again since they are locked already
        assert!(matches!(
            ls.initialize_locks(&[ref1, ref2]),
            Err(SuiError::TransactionLockExists { .. })
        ));

        // Now remove the locks
        ls.delete_locks(&[ref1, ref2]).unwrap();

        // Now initialization should succeed
        ls.initialize_locks(&[ref1, ref2]).unwrap();
    }
}
