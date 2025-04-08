// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::{AuthorityPerEpochStore, LockDetails};
use dashmap::mapref::entry::Entry as DashMapEntry;
use dashmap::DashMap;
use mysten_common::*;
use sui_types::base_types::{ObjectID, ObjectRef};
use sui_types::digests::TransactionDigest;
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::object::Object;
use sui_types::storage::ObjectStore;
use sui_types::transaction::VerifiedSignedTransaction;
use tracing::{debug, info, instrument, trace};

use super::writeback_cache::WritebackCache;

type RefCount = usize;

pub(super) struct ObjectLocks {
    // When acquire transaction locks, lock entries are briefly inserted into this map. The map
    // exists to provide atomic test-and-set operations on the locks. After all locks have been inserted
    // into the map, they are written to the db, and then all locks are removed from the map.
    //
    // After a transaction has been executed, newly created objects are available to be locked.
    // But, because of crash recovery, we cannot rule out that a lock may already exist in the db for
    // those objects. Therefore we do a db read for each object we are locking.
    //
    // TODO: find a strategy to allow us to avoid db reads for each object.
    locked_transactions: DashMap<ObjectRef, (RefCount, LockDetails)>,
}

impl ObjectLocks {
    pub fn new() -> Self {
        Self {
            locked_transactions: DashMap::new(),
        }
    }

    pub(crate) fn get_transaction_lock(
        &self,
        obj_ref: &ObjectRef,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult<Option<LockDetails>> {
        // We don't consult the in-memory state here. We are only interested in state that
        // has been committed to the db. This is because in memory state is reverted
        // if the transaction is not successfully locked.
        epoch_store.tables()?.get_locked_transaction(obj_ref)
    }

    /// Attempts to atomically test-and-set a transaction lock on an object.
    /// If the lock is already set to a conflicting transaction, an error is returned.
    /// If the lock is not set, or is already set to the same transaction, the lock is
    /// set.
    pub(crate) fn try_set_transaction_lock(
        &self,
        obj_ref: &ObjectRef,
        new_lock: LockDetails,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult {
        // entry holds a lock on the dashmap shard, so this function operates atomicly
        let entry = self.locked_transactions.entry(*obj_ref);

        // TODO: currently, the common case for this code is that we will miss the cache
        // and read from the db. It is difficult to implement negative caching, since we
        // may have restarted, in which case there could be locks in the db that we do
        // not have in the cache. We may want to explore strategies for proving there
        // cannot be a lock in the db that we do not know about. Two possibilities are:
        //
        // 1. Read all locks into memory at startup (and keep them there). The lifetime
        //    of locks is relatively short in the common case, so this might be feasible.
        // 2. Find some strategy to distinguish between the cases where we are re-executing
        //    old transactions after restarting vs executing transactions that we have never
        //    seen before. The output objects of novel transactions cannot previously have
        //    been locked on this validator.
        //
        // Solving this is not terribly important as it is not in the execution path, and
        // hence only improves the latency of transaction signing, not transaction execution
        let prev_lock = match entry {
            DashMapEntry::Vacant(vacant) => {
                let tables = epoch_store.tables()?;
                if let Some(lock_details) = tables.get_locked_transaction(obj_ref)? {
                    trace!("read lock from db: {:?}", lock_details);
                    vacant.insert((1, lock_details));
                    lock_details
                } else {
                    trace!("set lock: {:?}", new_lock);
                    vacant.insert((1, new_lock));
                    new_lock
                }
            }
            DashMapEntry::Occupied(mut occupied) => {
                occupied.get_mut().0 += 1;
                occupied.get().1
            }
        };

        if prev_lock != new_lock {
            debug!(
                "lock conflict detected for {:?}: {:?} != {:?}",
                obj_ref, prev_lock, new_lock
            );
            Err(SuiError::ObjectLockConflict {
                obj_ref: *obj_ref,
                pending_transaction: prev_lock,
            })
        } else {
            Ok(())
        }
    }

    pub(crate) fn clear(&self) {
        info!("clearing old transaction locks");
        self.locked_transactions.clear();
    }

    fn verify_live_object(obj_ref: &ObjectRef, live_object: &Object) -> SuiResult {
        debug_assert_eq!(obj_ref.0, live_object.id());
        if obj_ref.1 != live_object.version() {
            debug!(
                "object version unavailable for consumption: {:?} (current: {})",
                obj_ref,
                live_object.version()
            );
            return Err(SuiError::UserInputError {
                error: UserInputError::ObjectVersionUnavailableForConsumption {
                    provided_obj_ref: *obj_ref,
                    current_version: live_object.version(),
                },
            });
        }

        let live_digest = live_object.digest();
        if obj_ref.2 != live_digest {
            return Err(SuiError::UserInputError {
                error: UserInputError::InvalidObjectDigest {
                    object_id: obj_ref.0,
                    expected_digest: live_digest,
                },
            });
        }

        Ok(())
    }

    fn clear_cached_locks(&self, locks: &[(ObjectRef, LockDetails)]) {
        for (obj_ref, lock) in locks {
            let entry = self.locked_transactions.entry(*obj_ref);
            let mut occupied = match entry {
                DashMapEntry::Vacant(_) => {
                    debug_fatal!("lock must exist for object: {:?}", obj_ref);
                    continue;
                }
                DashMapEntry::Occupied(occupied) => occupied,
            };

            if occupied.get().1 == *lock {
                occupied.get_mut().0 -= 1;
                if occupied.get().0 == 0 {
                    trace!("clearing lock: {:?}", lock);
                    occupied.remove();
                }
            } else {
                // this is impossible because the only case in which we overwrite a
                // lock is when the lock is from a previous epoch. but we are holding
                // execution_lock, so the epoch cannot have changed.
                panic!("lock was changed since we set it");
            }
        }
    }

    fn multi_get_objects_must_exist(
        cache: &WritebackCache,
        object_ids: &[ObjectID],
    ) -> SuiResult<Vec<Object>> {
        let objects = cache.multi_get_objects(object_ids);
        let mut result = Vec::with_capacity(objects.len());
        for (i, object) in objects.into_iter().enumerate() {
            if let Some(object) = object {
                result.push(object);
            } else {
                return Err(SuiError::UserInputError {
                    error: UserInputError::ObjectNotFound {
                        object_id: object_ids[i],
                        version: None,
                    },
                });
            }
        }
        Ok(result)
    }

    #[instrument(level = "debug", skip_all)]
    pub(crate) fn acquire_transaction_locks(
        &self,
        cache: &WritebackCache,
        epoch_store: &AuthorityPerEpochStore,
        owned_input_objects: &[ObjectRef],
        tx_digest: TransactionDigest,
        signed_transaction: Option<VerifiedSignedTransaction>,
    ) -> SuiResult {
        let object_ids = owned_input_objects.iter().map(|o| o.0).collect::<Vec<_>>();
        let live_objects = Self::multi_get_objects_must_exist(cache, &object_ids)?;

        // Only live objects can be locked
        for (obj_ref, live_object) in owned_input_objects.iter().zip(live_objects.iter()) {
            Self::verify_live_object(obj_ref, live_object)?;
        }

        let mut locks_to_write: Vec<(_, LockDetails)> =
            Vec::with_capacity(owned_input_objects.len());

        // Sort the objects before locking. This is not required by the protocol (since it's okay to
        // reject any equivocating tx). However, this does prevent a confusing error on the client.
        // Consider the case:
        //   TX1: [o1, o2];
        //   TX2: [o2, o1];
        // If two threads race to acquire these locks, they might both acquire the first object, then
        // error when trying to acquire the second. The error returned to the client would say that there
        // is a conflicting tx on that object, but in fact neither object was locked and the tx was never
        // signed. If one client then retries, they will succeed (counterintuitively).
        let owned_input_objects = {
            let mut o = owned_input_objects.to_vec();
            o.sort_by_key(|o| o.0);
            o
        };

        // Note that this function does not have to operate atomically. If there are two racing threads,
        // then they are either trying to lock the same transaction (in which case both will succeed),
        // or they are trying to lock the same object in two different transactions, in which case
        // the sender has equivocated, and we are under no obligation to help them form a cert.
        for obj_ref in owned_input_objects.iter() {
            match self.try_set_transaction_lock(obj_ref, tx_digest, epoch_store) {
                Ok(()) => locks_to_write.push((*obj_ref, tx_digest)),
                Err(e) => {
                    // revert all pending writes and return error
                    // Note that reverting is not required for liveness, since a well formed and un-equivocating
                    // txn cannot fail to acquire locks.
                    // However, reverting is easy enough to do in this implementation that we do it anyway.
                    self.clear_cached_locks(&locks_to_write);
                    return Err(e);
                }
            }
        }

        // commit all writes to DB
        epoch_store
            .tables()?
            .write_transaction_locks(signed_transaction, locks_to_write.iter().cloned())?;

        // remove pending locks from unbounded storage
        self.clear_cached_locks(&locks_to_write);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::execution_cache::{
        writeback_cache::writeback_cache_tests::Scenario, ExecutionCacheWrite,
    };

    #[tokio::test]
    async fn test_transaction_locks_are_exclusive() {
        telemetry_subscribers::init_for_testing();
        Scenario::iterate(|mut s| async move {
            s.with_created(&[1, 2, 3]);
            s.do_tx().await;

            s.with_mutated(&[1, 2, 3]);
            s.do_tx().await;

            let new1 = s.obj_ref(1);
            let new2 = s.obj_ref(2);
            let new3 = s.obj_ref(3);

            s.with_mutated(&[1, 2, 3]); // begin forming a tx but never execute it
            let outputs = s.take_outputs();

            let tx1 = s.make_signed_transaction(&outputs.transaction);

            s.cache
                .acquire_transaction_locks(&s.epoch_store, &[new1, new2], *tx1.digest(), Some(tx1))
                .expect("locks should be available");

            // this tx doesn't use the actual objects in question, but we just need something
            // to insert into the table.
            s.with_created(&[4, 5]);
            let tx2 = s.take_outputs().transaction.clone();
            let tx2 = s.make_signed_transaction(&tx2);

            // both locks are held by tx1, so this should fail
            s.cache
                .acquire_transaction_locks(
                    &s.epoch_store,
                    &[new1, new2],
                    *tx2.digest(),
                    Some(tx2.clone()),
                )
                .unwrap_err();

            // new3 is lockable, but new2 is not, so this should fail
            s.cache
                .acquire_transaction_locks(
                    &s.epoch_store,
                    &[new3, new2],
                    *tx2.digest(),
                    Some(tx2.clone()),
                )
                .unwrap_err();

            // new3 is unlocked
            s.cache
                .acquire_transaction_locks(
                    &s.epoch_store,
                    &[new3],
                    *tx2.digest(),
                    Some(tx2.clone()),
                )
                .expect("new3 should be unlocked");
        })
        .await;
    }

    #[tokio::test]
    async fn test_transaction_locks_are_durable() {
        telemetry_subscribers::init_for_testing();
        Scenario::iterate(|mut s| async move {
            s.with_created(&[1, 2]);
            s.do_tx().await;

            let old2 = s.obj_ref(2);

            s.with_mutated(&[1, 2]);
            s.do_tx().await;

            let new1 = s.obj_ref(1);
            let new2 = s.obj_ref(2);

            s.with_mutated(&[1, 2]); // begin forming a tx but never execute it
            let outputs = s.take_outputs();

            let tx = s.make_signed_transaction(&outputs.transaction);

            // fails because we are referring to an old object
            s.cache
                .acquire_transaction_locks(
                    &s.epoch_store,
                    &[new1, old2],
                    *tx.digest(),
                    Some(tx.clone()),
                )
                .unwrap_err();

            // succeeds because the above call releases the lock on new1 after failing
            // to get the lock on old2
            s.cache
                .acquire_transaction_locks(
                    &s.epoch_store,
                    &[new1, new2],
                    *tx.digest(),
                    Some(tx.clone()),
                )
                .expect("new1 should be unlocked after revert");
        })
        .await;
    }

    #[tokio::test]
    async fn test_acquire_transaction_locks_revert() {
        telemetry_subscribers::init_for_testing();
        Scenario::iterate(|mut s| async move {
            s.with_created(&[1, 2]);
            s.do_tx().await;

            let old2 = s.obj_ref(2);

            s.with_mutated(&[1, 2]);
            s.do_tx().await;

            let new1 = s.obj_ref(1);
            let new2 = s.obj_ref(2);

            s.with_mutated(&[1, 2]); // begin forming a tx but never execute it
            let outputs = s.take_outputs();

            let tx = s.make_signed_transaction(&outputs.transaction);

            // fails because we are referring to an old object
            s.cache
                .acquire_transaction_locks(
                    &s.epoch_store,
                    &[new1, old2],
                    *tx.digest(),
                    Some(tx.clone()),
                )
                .unwrap_err();

            // this tx doesn't use the actual objects in question, but we just need something
            // to insert into the table.
            s.with_created(&[4, 5]);
            let tx2 = s.take_outputs().transaction.clone();
            let tx2 = s.make_signed_transaction(&tx2);

            // succeeds because the above call releases the lock on new1 after failing
            // to get the lock on old2
            s.cache
                .acquire_transaction_locks(&s.epoch_store, &[new1, new2], *tx2.digest(), Some(tx2))
                .expect("new1 should be unlocked after revert");
        })
        .await;
    }

    #[tokio::test]
    async fn test_acquire_transaction_locks_is_sync() {
        telemetry_subscribers::init_for_testing();
        Scenario::iterate(|mut s| async move {
            s.with_created(&[1, 2]);
            s.do_tx().await;

            let objects: Vec<_> = vec![s.object(1), s.object(2)]
                .into_iter()
                .map(|o| o.compute_object_reference())
                .collect();

            s.with_mutated(&[1, 2]);
            let outputs = s.take_outputs();

            let tx2 = s.make_signed_transaction(&outputs.transaction);
            // assert that acquire_transaction_locks is sync in non-simtest, which causes the
            // fail_point_async! macros above to be elided
            s.cache
                .acquire_transaction_locks(
                    &s.epoch_store,
                    &objects,
                    *tx2.digest(),
                    Some(tx2.clone()),
                )
                .unwrap();
        })
        .await;
    }
}
