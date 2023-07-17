// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cmp::max,
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

use lru::LruCache;
use mysten_metrics::monitored_scope;
use parking_lot::RwLock;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::{base_types::TransactionDigest, error::SuiResult};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    committee::EpochId,
    digests::TransactionEffectsDigest,
    transaction::{TransactionDataAPI, VerifiedCertificate},
};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, trace, warn};

use crate::authority::{
    authority_per_epoch_store::AuthorityPerEpochStore,
    authority_store::{InputKey, LockMode},
};
use crate::authority::{AuthorityMetrics, AuthorityStore};
use tap::TapOptional;

#[cfg(test)]
#[path = "unit_tests/transaction_manager_tests.rs"]
mod transaction_manager_tests;

/// Minimum capacity of HashMaps used in TransactionManager.
const MIN_HASHMAP_CAPACITY: usize = 1000;

/// TransactionManager is responsible for managing object dependencies of pending transactions,
/// and publishing a stream of certified transactions (certificates) ready to execute.
/// It receives certificates from Narwhal, validator RPC handlers, and checkpoint executor.
/// Execution driver subscribes to the stream of ready certificates from TransactionManager, and
/// executes them in parallel.
/// The actual execution logic is inside AuthorityState. After a transaction commits and updates
/// storage, committed objects and certificates are notified back to TransactionManager.
pub struct TransactionManager {
    authority_store: Arc<AuthorityStore>,
    tx_ready_certificates: UnboundedSender<(
        VerifiedExecutableTransaction,
        Option<TransactionEffectsDigest>,
    )>,
    metrics: Arc<AuthorityMetrics>,
    inner: RwLock<Inner>,
}

#[derive(Clone, Debug)]
struct PendingCertificate {
    // Certified transaction to be executed.
    certificate: VerifiedExecutableTransaction,
    // When executing from checkpoint, the certified effects digest is provided, so that forks can
    // be detected prior to committing the transaction.
    expected_effects_digest: Option<TransactionEffectsDigest>,
    // Input object locks that have not been acquired, because:
    // 1. The object has not been created yet.
    // 2. The object exists, but this transaction is trying to acquire a r/w lock while the object
    // is held in ro locks by other transaction(s).
    acquiring_locks: BTreeMap<InputKey, LockMode>,
    // Input object locks that have been acquired.
    acquired_locks: BTreeMap<InputKey, LockMode>,
}

/// LockQueue is a queue of transactions waiting or holding a lock on an object.
#[derive(Debug, Default)]
struct LockQueue {
    // Transactions waiting for read-only lock.
    readonly_waiters: BTreeSet<TransactionDigest>,
    // Transactions holding read-only lock that have not finished executions.
    readonly_holders: BTreeSet<TransactionDigest>,
    // Transactions waiting for default lock.
    // Only after there is no more transaction wait or holding read-only locks,
    // can a transaction acquire the default lock.
    // Note that except for immutable objects, a given key may only have one TransactionDigest in
    // the set. Unfortunately we cannot easily verify that this invariant is upheld, because you
    // cannot determine from TransactionData whether an input is mutable or immutable.
    default_waiters: BTreeSet<TransactionDigest>,
}

impl LockQueue {
    fn has_readonly(&self) -> bool {
        !(self.readonly_waiters.is_empty() && self.readonly_holders.is_empty())
    }

    fn is_empty(&self) -> bool {
        self.readonly_waiters.is_empty()
            && self.readonly_holders.is_empty()
            && self.default_waiters.is_empty()
    }
}

struct CacheInner {
    versioned_cache: LruCache<ObjectID, SequenceNumber>,
    // we cache packages separately, because they are more expensive to look up in the db, so we
    // don't want to evict packages in favor of mutable objects.
    unversioned_cache: LruCache<ObjectID, ()>,

    max_size: usize,
    metrics: Arc<AuthorityMetrics>,
}

impl CacheInner {
    fn new(max_size: usize, metrics: Arc<AuthorityMetrics>) -> Self {
        Self {
            versioned_cache: LruCache::unbounded(),
            unversioned_cache: LruCache::unbounded(),
            max_size,
            metrics,
        }
    }
}

impl CacheInner {
    fn shrink(&mut self) {
        while self.versioned_cache.len() > self.max_size {
            self.versioned_cache.pop_lru();
            self.metrics
                .transaction_manager_object_cache_evictions
                .inc();
        }
        while self.unversioned_cache.len() > self.max_size {
            self.unversioned_cache.pop_lru();
            self.metrics
                .transaction_manager_object_cache_evictions
                .inc();
        }
        self.metrics
            .transaction_manager_object_cache_size
            .set(self.versioned_cache.len() as i64);
        self.metrics
            .transaction_manager_package_cache_size
            .set(self.unversioned_cache.len() as i64);
    }

    fn insert(&mut self, object: &InputKey) {
        if let Some(version) = object.1 {
            if let Some((previous_id, previous_version)) =
                self.versioned_cache.push(object.0, version)
            {
                if previous_id == object.0 && previous_version > version {
                    // do not allow highest known version to decrease
                    // This should not be possible unless bugs are introduced elsewhere in this
                    // module.
                    self.versioned_cache.put(object.0, previous_version);
                } else {
                    self.metrics
                        .transaction_manager_object_cache_evictions
                        .inc();
                }
            }
            self.metrics
                .transaction_manager_object_cache_size
                .set(self.versioned_cache.len() as i64);
        } else if let Some((previous_id, _)) = self.unversioned_cache.push(object.0, ()) {
            // lru_cache will does not check if the value being evicted is the same as the value
            // being inserted, so we do need to check if the id is different before counting this
            // as an eviction.
            if previous_id != object.0 {
                self.metrics
                    .transaction_manager_package_cache_evictions
                    .inc();
            }
            self.metrics
                .transaction_manager_package_cache_size
                .set(self.unversioned_cache.len() as i64);
        }
    }

    // Returns Some(true/false) for a definitive result. Returns None if the caller must defer to
    // the db.
    fn is_object_available(&mut self, object: &InputKey) -> Option<bool> {
        if let Some(version) = object.1 {
            if let Some(current) = self.versioned_cache.get(&object.0) {
                self.metrics.transaction_manager_object_cache_hits.inc();
                Some(*current >= version)
            } else {
                self.metrics.transaction_manager_object_cache_misses.inc();
                None
            }
        } else {
            self.unversioned_cache
                .get(&object.0)
                .tap_some(|_| self.metrics.transaction_manager_package_cache_hits.inc())
                .tap_none(|| self.metrics.transaction_manager_package_cache_misses.inc())
                .map(|_| true)
        }
    }
}

struct AvailableObjectsCache {
    cache: CacheInner,
    unbounded_cache_enabled: usize,
}

impl AvailableObjectsCache {
    fn new(metrics: Arc<AuthorityMetrics>) -> Self {
        Self::new_with_size(metrics, 100000)
    }

    fn new_with_size(metrics: Arc<AuthorityMetrics>, size: usize) -> Self {
        Self {
            cache: CacheInner::new(size, metrics),
            unbounded_cache_enabled: 0,
        }
    }

    fn enable_unbounded_cache(&mut self) {
        self.unbounded_cache_enabled += 1;
    }

    fn disable_unbounded_cache(&mut self) {
        assert!(self.unbounded_cache_enabled > 0);
        self.unbounded_cache_enabled -= 1;
    }

    fn insert(&mut self, object: &InputKey) {
        self.cache.insert(object);
        if self.unbounded_cache_enabled == 0 {
            self.cache.shrink();
        }
    }

    fn is_object_available(&mut self, object: &InputKey) -> Option<bool> {
        self.cache.is_object_available(object)
    }
}

struct Inner {
    // Current epoch of TransactionManager.
    epoch: EpochId,

    // Maps input objects to transactions waiting for locks on the object.
    lock_waiters: HashMap<InputKey, LockQueue>,

    // Number of transactions that depend on each object ID. Should match exactly with total
    // number of transactions per object ID prefix in the missing_inputs table.
    // Used for throttling signing and submitting transactions depending on hot objects.
    input_objects: HashMap<ObjectID, usize>,

    // Maps object IDs to the highest observed sequence number of the object. When the value is
    // None, indicates that the object is immutable, corresponding to an InputKey with no sequence
    // number.
    available_objects_cache: AvailableObjectsCache,

    // A transaction enqueued to TransactionManager must be in either pending_certificates or
    // executing_certificates.

    // Maps transaction digests to their content and missing input objects.
    pending_certificates: HashMap<TransactionDigest, PendingCertificate>,
    // Maps executing transaction digests to their acquired input object locks.
    executing_certificates: HashMap<TransactionDigest, BTreeMap<InputKey, LockMode>>,
}

impl Inner {
    fn new(epoch: EpochId, metrics: Arc<AuthorityMetrics>) -> Inner {
        Inner {
            epoch,
            lock_waiters: HashMap::with_capacity(MIN_HASHMAP_CAPACITY),
            input_objects: HashMap::with_capacity(MIN_HASHMAP_CAPACITY),
            available_objects_cache: AvailableObjectsCache::new(metrics),
            pending_certificates: HashMap::with_capacity(MIN_HASHMAP_CAPACITY),
            executing_certificates: HashMap::with_capacity(MIN_HASHMAP_CAPACITY),
        }
    }

    // Checks if there is any transaction waiting on the lock of input_key, and try to
    // update transactions that can acquire the lock.
    // Must ensure input_key is available in storage before calling this function.
    fn try_acquire_lock(
        &mut self,
        input_key: InputKey,
        update_cache: bool,
    ) -> Vec<PendingCertificate> {
        if update_cache {
            self.available_objects_cache.insert(&input_key);
        }

        let mut ready_certificates = Vec::new();

        let Some(lock_queue) = self.lock_waiters.get_mut(&input_key) else {
            // No transaction is waiting on the object yet.
            return ready_certificates;
        };

        // Waiters can acquire lock in either readonly or default mode.
        let mut digests = BTreeSet::new();
        if !lock_queue.readonly_waiters.is_empty() {
            std::mem::swap(&mut digests, &mut lock_queue.readonly_waiters);
            lock_queue.readonly_holders.extend(digests.iter().cloned());
        } else if lock_queue.readonly_holders.is_empty() {
            // Only try to acquire default lock if there is no readonly lock waiter / holder.
            std::mem::swap(&mut digests, &mut lock_queue.default_waiters);
        };
        if lock_queue.is_empty() {
            self.lock_waiters.remove(&input_key);
        }
        if digests.is_empty() {
            return ready_certificates;
        }

        let input_count = self.input_objects.get_mut(&input_key.0).unwrap_or_else(|| {
            panic!(
                "# of transactions waiting on object {:?} cannot be 0",
                input_key.0
            )
        });
        *input_count -= digests.len();
        if *input_count == 0 {
            self.input_objects.remove(&input_key.0);
        }

        for digest in digests {
            // Pending certificate must exist.
            let pending_cert = self.pending_certificates.get_mut(&digest).unwrap();
            let lock_mode = pending_cert.acquiring_locks.remove(&input_key).unwrap();
            assert!(pending_cert
                .acquired_locks
                .insert(input_key, lock_mode)
                .is_none());
            // When a certificate has all locks acquired, it is ready to execute.
            if pending_cert.acquiring_locks.is_empty() {
                let pending_cert = self.pending_certificates.remove(&digest).unwrap();
                ready_certificates.push(pending_cert);
            } else {
                // TODO: we should start logging this at a higher level after some period of
                // time has elapsed.
                trace!(tx_digest = ?digest,acquiring = ?pending_cert.acquiring_locks, "Certificate acquiring locks");
            }
        }

        ready_certificates
    }

    fn maybe_reserve_capacity(&mut self) {
        self.lock_waiters.maybe_reserve_capacity();
        self.input_objects.maybe_reserve_capacity();
        self.pending_certificates.maybe_reserve_capacity();
        self.executing_certificates.maybe_reserve_capacity();
    }

    /// After reaching 1/4 load in hashmaps, decrease capacity to increase load to 1/2.
    fn maybe_shrink_capacity(&mut self) {
        self.lock_waiters.maybe_shrink_capacity();
        self.input_objects.maybe_shrink_capacity();
        self.pending_certificates.maybe_shrink_capacity();
        self.executing_certificates.maybe_shrink_capacity();
    }
}

impl TransactionManager {
    /// If a node restarts, transaction manager recovers in-memory data from pending_certificates,
    /// which contains certificates not yet executed from Narwhal output and RPC.
    /// Transactions from other sources, e.g. checkpoint executor, have own persistent storage to
    /// retry transactions.
    pub(crate) fn new(
        authority_store: Arc<AuthorityStore>,
        epoch_store: &AuthorityPerEpochStore,
        tx_ready_certificates: UnboundedSender<(
            VerifiedExecutableTransaction,
            Option<TransactionEffectsDigest>,
        )>,
        metrics: Arc<AuthorityMetrics>,
    ) -> TransactionManager {
        let transaction_manager = TransactionManager {
            authority_store,
            metrics: metrics.clone(),
            inner: RwLock::new(Inner::new(epoch_store.epoch(), metrics)),
            tx_ready_certificates,
        };
        transaction_manager
            .enqueue(epoch_store.all_pending_execution().unwrap(), epoch_store)
            .expect("Initialize TransactionManager with pending certificates failed.");
        transaction_manager
    }

    /// Enqueues certificates / verified transactions into TransactionManager. Once all of the input objects are available
    /// locally for a certificate, the certified transaction will be sent to execution driver.
    ///
    /// REQUIRED: Shared object locks must be taken before calling enqueueing transactions
    /// with shared objects!
    pub(crate) fn enqueue_certificates(
        &self,
        certs: Vec<VerifiedCertificate>,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult<()> {
        let executable_txns = certs
            .into_iter()
            .map(VerifiedExecutableTransaction::new_from_certificate)
            .collect();
        self.enqueue(executable_txns, epoch_store)
    }

    pub(crate) fn enqueue(
        &self,
        certs: Vec<VerifiedExecutableTransaction>,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult<()> {
        let certs = certs.into_iter().map(|cert| (cert, None)).collect();
        self.enqueue_impl(certs, epoch_store)
    }

    pub(crate) fn enqueue_with_expected_effects_digest(
        &self,
        certs: Vec<(VerifiedExecutableTransaction, TransactionEffectsDigest)>,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult<()> {
        let certs = certs
            .into_iter()
            .map(|(cert, fx)| (cert, Some(fx)))
            .collect();
        self.enqueue_impl(certs, epoch_store)
    }

    fn enqueue_impl(
        &self,
        certs: Vec<(
            VerifiedExecutableTransaction,
            Option<TransactionEffectsDigest>,
        )>,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult<()> {
        // filter out already executed certs
        let certs: Vec<_> = certs
            .into_iter()
            .filter(|(cert, _)| {
                let digest = *cert.digest();
                // skip already executed txes
                if self
                    .authority_store
                    .is_tx_already_executed(&digest)
                    .expect("Failed to check if tx is already executed")
                {
                    // also ensure the transaction will not be retried after restart.
                    let _ = epoch_store.remove_pending_execution(&digest);
                    self.metrics
                        .transaction_manager_num_enqueued_certificates
                        .with_label_values(&["already_executed"])
                        .inc();
                    false
                } else {
                    true
                }
            })
            .collect();

        let mut object_availability: HashMap<InputKey, Option<bool>> = HashMap::new();
        let certs: Vec<_> = certs
            .into_iter()
            .map(|(cert, fx_digest)| {
                let digest = *cert.digest();
                let input_object_kinds = cert
                    .data()
                    .intent_message()
                    .value
                    .input_objects()
                    .expect("input_objects() cannot fail");
                let input_object_locks = self.authority_store.get_input_object_locks(
                    &digest,
                    &input_object_kinds,
                    epoch_store,
                );
                if input_object_kinds.len() != input_object_locks.len() {
                    error!("Duplicated input objects: {:?}", input_object_kinds);
                }
                for key in input_object_locks.keys() {
                    object_availability.insert(*key, None);
                }
                (cert, fx_digest, input_object_locks)
            })
            .collect();

        {
            let mut inner = self.inner.write();
            for (key, value) in object_availability.iter_mut() {
                if let Some(available) = inner.available_objects_cache.is_object_available(key) {
                    *value = Some(available);
                }
            }
            // make sure we don't miss any cache entries while the lock is not held.
            inner.available_objects_cache.enable_unbounded_cache();
        }

        let input_object_cache_misses = object_availability
            .iter()
            .filter_map(|(key, value)| if value.is_none() { Some(*key) } else { None })
            .collect::<Vec<_>>();

        // Checking object availability without holding TM lock to reduce contention.
        // But input objects can become available before TM lock is acquired.
        // So missing objects' availability are checked again after releasing the TM lock.
        let cache_miss_availibility = self
            .authority_store
            .multi_input_objects_exist(input_object_cache_misses.iter().cloned())
            .expect("Checking object existence cannot fail!")
            .into_iter()
            .zip(input_object_cache_misses.into_iter());

        // After this point, the function cannot return early and must run to the end. Otherwise,
        // it can lead to data inconsistencies and potentially some transactions will never get
        // executed.

        // Internal lock is held only for updating the internal state.
        let mut inner = self.inner.write();
        let _scope = monitored_scope("TransactionManager::enqueue::wlock");

        for (available, key) in cache_miss_availibility {
            if available && key.1.is_none() {
                // Mutable objects obtained from cache_miss_availability usually will not be read
                // again, so we do not want to evict other objects in order to insert them into the
                // cache. However, packages will likely be read often, so we do want to insert them
                // even if they cause evictions.
                inner.available_objects_cache.insert(&key);
            }
            object_availability
                .insert(key, Some(available))
                .expect("entry must already exist");
        }

        // Now recheck the cache for anything that became available (via notify_commit) since we
        // read cache_miss_availibility - because the cache is unbounded mode it is guaranteed to
        // contain all notifications that arrived since we released the lock on self.inner.
        for (key, value) in object_availability.iter_mut() {
            if !value.expect("all objects must have been checked by now") {
                if let Some(true) = inner.available_objects_cache.is_object_available(key) {
                    *value = Some(true);
                }
            }
        }

        inner.available_objects_cache.disable_unbounded_cache();

        let mut pending = Vec::new();

        for (cert, expected_effects_digest, input_object_locks) in certs {
            pending.push(PendingCertificate {
                certificate: cert,
                expected_effects_digest,
                acquiring_locks: input_object_locks,
                acquired_locks: BTreeMap::new(),
            });
        }

        for mut pending_cert in pending {
            // Tx lock is not held here, which makes it possible to send duplicated transactions to
            // the execution driver after crash-recovery, when the same transaction is recovered
            // from recovery log and pending certificates table. The transaction will still only
            // execute once, because tx lock is acquired in execution driver and executed effects
            // table is consulted. So this behavior is benigh.
            let digest = *pending_cert.certificate.digest();

            if inner.epoch != pending_cert.certificate.epoch() {
                warn!(
                    "Ignoring enqueued certificate from wrong epoch. Expected={} Certificate={:?}",
                    inner.epoch, pending_cert.certificate
                );
                // also ensure the transaction will not be retried after restart.
                let _ = epoch_store.remove_pending_execution(&digest);
                continue;
            }

            // skip already pending txes
            if inner.pending_certificates.contains_key(&digest) {
                self.metrics
                    .transaction_manager_num_enqueued_certificates
                    .with_label_values(&["already_pending"])
                    .inc();
                continue;
            }
            // skip already executing txes
            if inner.executing_certificates.contains_key(&digest) {
                self.metrics
                    .transaction_manager_num_enqueued_certificates
                    .with_label_values(&["already_executing"])
                    .inc();
                continue;
            }
            // skip already executed txes
            if self.authority_store.is_tx_already_executed(&digest)? {
                // also ensure the transaction will not be retried after restart.
                let _ = epoch_store.remove_pending_execution(&digest);
                self.metrics
                    .transaction_manager_num_enqueued_certificates
                    .with_label_values(&["already_executed"])
                    .inc();
                continue;
            }

            let mut acquiring_locks = BTreeMap::new();
            std::mem::swap(&mut acquiring_locks, &mut pending_cert.acquiring_locks);
            for (key, lock_mode) in acquiring_locks {
                // The transaction needs to wait to acquire locks in two cases:
                let mut acquire = false;
                if !object_availability[&key].unwrap() {
                    // 1. The input object is not yet available.
                    acquire = true;
                    let lock_queue = inner.lock_waiters.entry(key).or_default();
                    match lock_mode {
                        LockMode::Default => {
                            // If the transaction is acquiring the object in Default mode, it must
                            // wait for all ReadOnly locks to be released.
                            assert!(lock_queue.default_waiters.insert(digest));
                        }
                        LockMode::ReadOnly => {
                            assert!(lock_queue.readonly_waiters.insert(digest));
                        }
                    }
                } else {
                    match lock_mode {
                        LockMode::Default => {
                            // 2. The input object is currently locked in ReadOnly mode, and this
                            // transaction is acquiring it in Default mode.
                            if let Some(lock_queue) = inner.lock_waiters.get_mut(&key) {
                                // If there are any ReadOnly locks, the transaction must wait for
                                // them to be released.
                                if lock_queue.has_readonly() {
                                    acquire = true;
                                    assert!(lock_queue.default_waiters.insert(digest));
                                }
                            }
                        }
                        LockMode::ReadOnly => {
                            // Acquired readonly locks need to be tracked until the transaction has
                            // finished execution.
                            let lock_queue = inner.lock_waiters.entry(key).or_default();
                            assert!(lock_queue.readonly_holders.insert(digest));
                        }
                    }
                }
                if acquire {
                    pending_cert.acquiring_locks.insert(key, lock_mode);
                    let input_count = inner.input_objects.entry(key.0).or_default();
                    *input_count += 1;
                } else {
                    pending_cert.acquired_locks.insert(key, lock_mode);
                }
            }

            // Ready transactions can start to execute.
            if pending_cert.acquiring_locks.is_empty() {
                self.metrics
                    .transaction_manager_num_enqueued_certificates
                    .with_label_values(&["ready"])
                    .inc();
                // Send to execution driver for execution.
                self.certificate_ready(&mut inner, pending_cert);
                continue;
            }

            assert!(
                inner
                    .pending_certificates
                    .insert(digest, pending_cert)
                    .is_none(),
                "Duplicated pending certificate {:?}",
                digest
            );

            self.metrics
                .transaction_manager_num_enqueued_certificates
                .with_label_values(&["pending"])
                .inc();
        }

        self.metrics
            .transaction_manager_num_missing_objects
            .set(inner.lock_waiters.len() as i64);
        self.metrics
            .transaction_manager_num_pending_certificates
            .set(inner.pending_certificates.len() as i64);

        inner.maybe_reserve_capacity();

        Ok(())
    }

    /// Notifies TransactionManager that the given objects are available in the objects table.
    /// Useful when transactions associated with the objects are not known, e.g. after checking
    /// object availability from storage, or for testing.
    pub(crate) fn _fastpath_objects_available(
        &self,
        input_keys: Vec<InputKey>,
        epoch_store: &AuthorityPerEpochStore,
    ) {
        let mut inner = self.inner.write();
        let _scope = monitored_scope("TransactionManager::objects_available::wlock");
        self.objects_available_locked(&mut inner, epoch_store, input_keys, false);
        inner.maybe_shrink_capacity();
    }

    #[cfg(test)]
    pub(crate) fn objects_available(
        &self,
        input_keys: Vec<InputKey>,
        epoch_store: &AuthorityPerEpochStore,
    ) {
        let mut inner = self.inner.write();
        let _scope = monitored_scope("TransactionManager::objects_available::wlock");
        self.objects_available_locked(&mut inner, epoch_store, input_keys, true);
        inner.maybe_shrink_capacity();
    }

    fn objects_available_locked(
        &self,
        inner: &mut Inner,
        epoch_store: &AuthorityPerEpochStore,
        input_keys: Vec<InputKey>,
        update_cache: bool,
    ) {
        if inner.epoch != epoch_store.epoch() {
            warn!(
                "Ignoring objects committed from wrong epoch. Expected={} Actual={} \
                 Objects={:?}",
                inner.epoch,
                epoch_store.epoch(),
                input_keys,
            );
            return;
        }

        for input_key in input_keys {
            trace!(?input_key, "object available");
            for ready_cert in inner.try_acquire_lock(input_key, update_cache) {
                self.certificate_ready(inner, ready_cert);
            }
        }

        self.metrics
            .transaction_manager_num_missing_objects
            .set(inner.lock_waiters.len() as i64);
        self.metrics
            .transaction_manager_num_pending_certificates
            .set(inner.pending_certificates.len() as i64);
        self.metrics
            .transaction_manager_num_executing_certificates
            .set(inner.executing_certificates.len() as i64);
    }

    /// Notifies TransactionManager about a transaction that has been committed.
    pub(crate) fn notify_commit(
        &self,
        digest: &TransactionDigest,
        output_object_keys: Vec<InputKey>,
        epoch_store: &AuthorityPerEpochStore,
    ) {
        {
            let mut inner = self.inner.write();
            let _scope = monitored_scope("TransactionManager::notify_commit::wlock");

            if inner.epoch != epoch_store.epoch() {
                warn!("Ignoring committed certificate from wrong epoch. Expected={} Actual={} CertificateDigest={:?}", inner.epoch, epoch_store.epoch(), digest);
                return;
            }

            self.objects_available_locked(&mut inner, epoch_store, output_object_keys, true);

            let Some(acquired_locks) = inner.executing_certificates.remove(digest) else {
                trace!("{:?} not found in executing certificates, likely because it is a system transaction", digest);
                return;
            };
            for (key, lock_mode) in acquired_locks {
                if lock_mode == LockMode::Default {
                    // Holders of default locks are not tracked.
                    continue;
                }
                assert_eq!(lock_mode, LockMode::ReadOnly);
                let lock_queue = inner.lock_waiters.get_mut(&key).unwrap();
                assert!(
                    lock_queue.readonly_holders.remove(digest),
                    "Certificate {:?} not found among readonly lock holders",
                    digest
                );
                for ready_cert in inner.try_acquire_lock(key, true) {
                    self.certificate_ready(&mut inner, ready_cert);
                }
            }
            self.metrics
                .transaction_manager_num_executing_certificates
                .set(inner.executing_certificates.len() as i64);

            inner.maybe_shrink_capacity();
        }

        let _ = epoch_store.remove_pending_execution(digest);
    }

    /// Sends the ready certificate for execution.
    fn certificate_ready(&self, inner: &mut Inner, pending_certificate: PendingCertificate) {
        let cert = pending_certificate.certificate;
        let expected_effects_digest = pending_certificate.expected_effects_digest;
        trace!(tx_digest = ?cert.digest(), "certificate ready");
        // Record as an executing certificate.
        assert_eq!(
            pending_certificate.acquired_locks.len(),
            cert.data()
                .intent_message()
                .value
                .input_objects()
                .unwrap()
                .len()
        );
        assert!(inner
            .executing_certificates
            .insert(*cert.digest(), pending_certificate.acquired_locks)
            .is_none());
        let _ = self
            .tx_ready_certificates
            .send((cert, expected_effects_digest));
        self.metrics.transaction_manager_num_ready.inc();
        self.metrics.execution_driver_dispatch_queue.inc();
    }

    /// Gets the missing input object keys for the given transaction.
    pub(crate) fn get_missing_input(&self, digest: &TransactionDigest) -> Option<Vec<InputKey>> {
        let inner = self.inner.read();
        inner
            .pending_certificates
            .get(digest)
            .map(|cert| cert.acquiring_locks.keys().cloned().collect())
    }

    // Returns the number of transactions waiting on each object ID.
    pub(crate) fn objects_queue_len(&self, keys: Vec<ObjectID>) -> Vec<(ObjectID, usize)> {
        let inner = self.inner.read();
        keys.into_iter()
            .map(|key| {
                (
                    key,
                    inner.input_objects.get(&key).cloned().unwrap_or_default(),
                )
            })
            .collect()
    }

    // Returns the number of transactions pending or being executed right now.
    pub(crate) fn inflight_queue_len(&self) -> usize {
        let inner = self.inner.read();
        inner.pending_certificates.len() + inner.executing_certificates.len()
    }

    // Reconfigures the TransactionManager for a new epoch. Existing transactions will be dropped
    // because they are no longer relevant and may be incorrect in the new epoch.
    pub(crate) fn reconfigure(&self, new_epoch: EpochId) {
        let mut inner = self.inner.write();
        *inner = Inner::new(new_epoch, self.metrics.clone());
    }

    // Verify TM has no pending item for tests.
    #[cfg(test)]
    fn check_empty_for_testing(&self) {
        let inner = self.inner.read();
        assert!(
            inner.lock_waiters.is_empty(),
            "Lock waiters: {:?}",
            inner.lock_waiters
        );
        assert!(
            inner.input_objects.is_empty(),
            "Input objects: {:?}",
            inner.input_objects
        );
        assert!(
            inner.pending_certificates.is_empty(),
            "Pending certificates: {:?}",
            inner.pending_certificates
        );
        assert!(
            inner.executing_certificates.is_empty(),
            "Executing certificates: {:?}",
            inner.executing_certificates
        );
    }
}

trait ResizableHashMap<K, V> {
    fn maybe_reserve_capacity(&mut self);
    fn maybe_shrink_capacity(&mut self);
}

impl<K, V> ResizableHashMap<K, V> for HashMap<K, V>
where
    K: std::cmp::Eq + std::hash::Hash,
{
    /// After reaching 3/4 load in hashmaps, increase capacity to decrease load to 1/2.
    fn maybe_reserve_capacity(&mut self) {
        if self.len() > self.capacity() * 3 / 4 {
            self.reserve(self.capacity() / 2);
        }
    }

    /// After reaching 1/4 load in hashmaps, decrease capacity to increase load to 1/2.
    fn maybe_shrink_capacity(&mut self) {
        if self.len() > MIN_HASHMAP_CAPACITY && self.len() < self.capacity() / 4 {
            self.shrink_to(max(self.capacity() / 2, MIN_HASHMAP_CAPACITY))
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use prometheus::Registry;

    #[test]
    fn test_available_objects_cache() {
        let metrics = Arc::new(AuthorityMetrics::new(&Registry::default()));
        let mut cache = AvailableObjectsCache::new_with_size(metrics, 5);

        // insert 10 unique unversioned objects
        for i in 0..10 {
            let object = ObjectID::new([i; 32]);
            let input_key = InputKey(object, None);
            assert_eq!(cache.is_object_available(&input_key), None);
            cache.insert(&input_key);
            assert_eq!(cache.is_object_available(&input_key), Some(true));
        }

        // first 5 have been evicted
        for i in 0..5 {
            let object = ObjectID::new([i; 32]);
            let input_key = InputKey(object, None);
            assert_eq!(cache.is_object_available(&input_key), None);
        }

        // insert 10 unique versioned objects
        for i in 0..10 {
            let object = ObjectID::new([i; 32]);
            let input_key = InputKey(object, Some((i as u64).into()));
            assert_eq!(cache.is_object_available(&input_key), None);
            cache.insert(&input_key);
            assert_eq!(cache.is_object_available(&input_key), Some(true));
        }

        // first 5 versioned objects have been evicted
        for i in 0..5 {
            let object = ObjectID::new([i; 32]);
            let input_key = InputKey(object, Some((i as u64).into()));
            assert_eq!(cache.is_object_available(&input_key), None);
        }

        // but versioned objects do not cause evictions of unversioned objects
        for i in 5..10 {
            let object = ObjectID::new([i; 32]);
            let input_key = InputKey(object, None);
            assert_eq!(cache.is_object_available(&input_key), Some(true));
        }

        // object 9 is available at version 9
        let object = ObjectID::new([9; 32]);
        let input_key = InputKey(object, Some(9.into()));
        assert_eq!(cache.is_object_available(&input_key), Some(true));
        // but not at version 10
        let input_key = InputKey(object, Some(10.into()));
        assert_eq!(cache.is_object_available(&input_key), Some(false));
        // it is available at version 8 (this case can be used by readonly shared objects)
        let input_key = InputKey(object, Some(8.into()));
        assert_eq!(cache.is_object_available(&input_key), Some(true));
    }
}
