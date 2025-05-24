// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use parking_lot::RwLock;
use std::{
    cmp::Reverse,
    collections::{hash_map::Entry, BinaryHeap, HashMap},
    time::Duration,
};
use sui_config::node::AuthorityOverloadConfig;
use sui_types::{
    base_types::FullObjectID,
    digests::TransactionDigest,
    error::{SuiError, SuiResult},
    fp_bail, fp_ensure,
    message_envelope::Message,
    transaction::{SenderSignedData, TransactionDataAPI},
};
use tokio::time::Instant;
use tracing::info;

#[derive(Default, Debug)]
struct TransactionQueue {
    digests: HashMap<TransactionDigest, Instant>,
    ages: BinaryHeap<(Reverse<Instant>, TransactionDigest)>,
}

/// Tracks the current age of transactions depending on each object.
/// This is used to detect congestion on hot shared objects.
pub(crate) struct OverloadTracker {
    // Stores age info for all transactions depending on each object.
    // Used for throttling signing and submitting transactions depending on hot objects.
    object_waiting_queue: RwLock<HashMap<FullObjectID, TransactionQueue>>,
}

impl OverloadTracker {
    pub(crate) fn new() -> Self {
        Self {
            object_waiting_queue: RwLock::new(HashMap::new()),
        }
    }

    pub(crate) fn add_pending_certificate(&self, tx_data: &SenderSignedData) {
        let tx_digest = tx_data.digest();
        let mutable_shared_objects = Self::get_mutable_shared_objects(tx_data);
        let mut object_waiting_queue = self.object_waiting_queue.write();
        let instant = Instant::now();
        for object_id in mutable_shared_objects {
            let queue = object_waiting_queue.entry(object_id).or_default();
            queue.insert(tx_digest, instant);
        }
    }

    pub(crate) fn remove_pending_certificate(&self, tx_data: &SenderSignedData) {
        let mutable_shared_objects = Self::get_mutable_shared_objects(tx_data);
        let mut object_waiting_queue = self.object_waiting_queue.write();
        for object_id in mutable_shared_objects {
            if let Some(entry) = object_waiting_queue.get_mut(&object_id) {
                entry.remove(&tx_data.digest());
                if entry.is_empty() {
                    object_waiting_queue.remove(&object_id);
                }
            }
        }
    }

    fn get_mutable_shared_objects(tx_data: &SenderSignedData) -> Vec<FullObjectID> {
        tx_data
            .transaction_data()
            .shared_input_objects()
            .into_iter()
            .filter_map(|r| {
                r.mutable
                    .then_some(FullObjectID::new(r.id, Some(r.initial_shared_version)))
            })
            .collect()
    }

    pub(crate) fn check_execution_overload(
        &self,
        overload_config: &AuthorityOverloadConfig,
        tx_data: &SenderSignedData,
        inflight_queue_len: usize,
    ) -> SuiResult {
        // Too many transactions are pending execution.
        fp_ensure!(
            inflight_queue_len < overload_config.max_transaction_manager_queue_length,
            SuiError::TooManyTransactionsPendingExecution {
                queue_len: inflight_queue_len,
                threshold: overload_config.max_transaction_manager_queue_length,
            }
        );

        let mutable_shared_objects = Self::get_mutable_shared_objects(tx_data);
        let queue_len_and_age = self.objects_queue_len_and_age(mutable_shared_objects);
        for (object_id, queue_len, txn_age) in queue_len_and_age {
            // When this occurs, most likely transactions piled up on a shared object.
            if queue_len >= overload_config.max_transaction_manager_per_object_queue_length {
                info!(
                    "Overload detected on object {:?} with {} pending transactions",
                    object_id, queue_len
                );
                fp_bail!(SuiError::TooManyTransactionsPendingOnObject {
                    object_id: object_id.id(),
                    queue_len,
                    threshold: overload_config.max_transaction_manager_per_object_queue_length,
                });
            }
            if let Some(age) = txn_age {
                // Check that we don't have a txn that has been waiting for a long time in the queue.
                if age >= overload_config.max_txn_age_in_queue {
                    info!(
                        "Overload detected on object {:?} with oldest transaction pending for {}ms",
                        object_id,
                        age.as_millis()
                    );
                    fp_bail!(SuiError::TooOldTransactionPendingOnObject {
                        object_id: object_id.id(),
                        txn_age_sec: age.as_secs(),
                        threshold: overload_config.max_txn_age_in_queue.as_secs(),
                    });
                }
            }
        }

        Ok(())
    }

    // Returns the number of transactions waiting on each object ID, as well as the age of the oldest transaction in the queue.
    fn objects_queue_len_and_age(
        &self,
        keys: Vec<FullObjectID>,
    ) -> Vec<(FullObjectID, usize, Option<Duration>)> {
        let input_objects = self.object_waiting_queue.read();
        keys.into_iter()
            .map(|key| {
                let default_map = TransactionQueue::default();
                let txns = input_objects.get(&key).unwrap_or(&default_map);
                (
                    key,
                    txns.len(),
                    txns.first().map(|(time, _)| time.elapsed()),
                )
            })
            .collect()
    }
}

impl TransactionQueue {
    fn len(&self) -> usize {
        self.digests.len()
    }

    fn is_empty(&self) -> bool {
        self.digests.is_empty()
    }

    /// Insert the digest into the queue with the given time. If the digest is
    /// already in the queue, this is a no-op.
    fn insert(&mut self, digest: TransactionDigest, time: Instant) {
        if let Entry::Vacant(entry) = self.digests.entry(digest) {
            entry.insert(time);
            self.ages.push((Reverse(time), digest));
        }
    }

    /// Remove the digest from the queue. Returns the time the digest was
    /// inserted into the queue, if it was present.
    ///
    /// After removing the digest, first() will return the new oldest entry
    /// in the queue (which may be unchanged).
    fn remove(&mut self, digest: &TransactionDigest) -> Option<Instant> {
        let when = self.digests.remove(digest)?;

        // This loop removes all previously inserted entries that no longer
        // correspond to live entries in self.digests. When the loop terminates,
        // the top of the heap will be the oldest live entry.
        // Amortized complexity of `remove` is O(lg(n)).
        while !self.ages.is_empty() {
            let first = self.ages.peek().expect("heap cannot be empty");

            // We compare the exact time of the entry, because there may be an
            // entry in the heap that was previously inserted and removed from
            // digests, and we want to ignore it. (see test_transaction_queue_remove_in_order)
            if self.digests.get(&first.1) == Some(&first.0 .0) {
                break;
            }

            self.ages.pop();
        }

        Some(when)
    }

    /// Return the oldest entry in the queue.
    fn first(&self) -> Option<(Instant, TransactionDigest)> {
        self.ages.peek().map(|(time, digest)| (time.0, *digest))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::{Rng, RngCore};

    #[test]
    #[cfg_attr(msim, ignore)]
    fn test_transaction_queue() {
        let mut queue = TransactionQueue::default();

        // insert and remove an item
        let time = Instant::now();
        let digest = TransactionDigest::new([1; 32]);
        queue.insert(digest, time);
        assert_eq!(queue.first(), Some((time, digest)));
        queue.remove(&digest);
        assert_eq!(queue.first(), None);

        // remove a non-existent item
        assert_eq!(queue.remove(&digest), None);
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn test_transaction_queue_remove_in_order() {
        // insert two items, remove them in insertion order
        let time1 = Instant::now();
        let digest1 = TransactionDigest::new([1; 32]);
        let time2 = time1 + Duration::from_secs(1);
        let digest2 = TransactionDigest::new([2; 32]);

        let mut queue = TransactionQueue::default();
        queue.insert(digest1, time1);
        queue.insert(digest2, time2);

        assert_eq!(queue.first(), Some((time1, digest1)));
        assert_eq!(queue.remove(&digest1), Some(time1));
        assert_eq!(queue.first(), Some((time2, digest2)));
        assert_eq!(queue.remove(&digest2), Some(time2));
        assert_eq!(queue.first(), None);
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn test_transaction_queue_remove_in_reverse_order() {
        // insert two items, remove them in reverse order
        let time1 = Instant::now();
        let digest1 = TransactionDigest::new([1; 32]);
        let time2 = time1 + Duration::from_secs(1);
        let digest2 = TransactionDigest::new([2; 32]);

        let mut queue = TransactionQueue::default();
        queue.insert(digest1, time1);
        queue.insert(digest2, time2);

        assert_eq!(queue.first(), Some((time1, digest1)));
        assert_eq!(queue.remove(&digest2), Some(time2));

        // after removing digest2, digest1 is still the first item
        assert_eq!(queue.first(), Some((time1, digest1)));
        assert_eq!(queue.remove(&digest1), Some(time1));

        assert_eq!(queue.first(), None);
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn test_transaction_queue_reinsert() {
        // insert two items
        let time1 = Instant::now();
        let digest1 = TransactionDigest::new([1; 32]);
        let time2 = time1 + Duration::from_secs(1);
        let digest2 = TransactionDigest::new([2; 32]);

        let mut queue = TransactionQueue::default();
        queue.insert(digest1, time1);
        queue.insert(digest2, time2);

        // remove the second item
        queue.remove(&digest2);
        assert_eq!(queue.first(), Some((time1, digest1)));

        // insert the second item again
        let time3 = time2 + Duration::from_secs(1);
        queue.insert(digest2, time3);

        // remove the first item
        queue.remove(&digest1);

        // time3 should be in first()
        assert_eq!(queue.first(), Some((time3, digest2)));
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn test_transaction_queue_double_insert() {
        let time1 = Instant::now();
        let digest1 = TransactionDigest::new([1; 32]);
        let time2 = time1 + Duration::from_secs(1);
        let digest2 = TransactionDigest::new([2; 32]);
        let time3 = time2 + Duration::from_secs(1);

        let mut queue = TransactionQueue::default();
        queue.insert(digest1, time1);
        queue.insert(digest2, time2);
        queue.insert(digest2, time3);

        // re-insertion of digest2 should not change its time
        assert_eq!(queue.first(), Some((time1, digest1)));
        queue.remove(&digest1);
        assert_eq!(queue.first(), Some((time2, digest2)));
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn transaction_queue_random_test() {
        let mut rng = rand::thread_rng();
        let mut digests = Vec::new();
        for _ in 0..100 {
            let mut digest = [0; 32];
            rng.fill_bytes(&mut digest);
            digests.push(TransactionDigest::new(digest));
        }

        let mut verifier = HashMap::new();
        let mut queue = TransactionQueue::default();

        let mut now = Instant::now();

        // first insert some random digests so that the queue starts
        // out well-populated
        for _ in 0..70 {
            now += Duration::from_secs(1);
            let digest = digests[rng.gen_range(0..digests.len())];
            let time = now;
            queue.insert(digest, time);
            verifier.entry(digest).or_insert(time);
        }

        // Do random operations on both the queue and the verifier, and
        // verify that the two structures always agree
        for _ in 0..100000 {
            // advance time
            now += Duration::from_secs(1);

            // pick a random digest
            let digest = digests[rng.gen_range(0..digests.len())];

            // either insert or remove it
            if rng.gen_bool(0.5) {
                let time = now;
                queue.insert(digest, time);
                verifier.entry(digest).or_insert(time);
            } else {
                let time = verifier.remove(&digest);
                assert_eq!(queue.remove(&digest), time);
            }

            assert_eq!(
                queue.first(),
                verifier
                    .iter()
                    .min_by_key(|(_, time)| **time)
                    .map(|(digest, time)| (*time, *digest))
            );
        }
    }
}
