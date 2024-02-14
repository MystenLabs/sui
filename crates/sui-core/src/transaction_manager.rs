// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cmp::max,
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use indexmap::IndexMap;
use lru::LruCache;
use mysten_metrics::monitored_scope;
use parking_lot::RwLock;
use sui_types::{
    base_types::TransactionDigest, error::SuiResult, fp_ensure, message_envelope::Message,
};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    committee::EpochId,
    digests::TransactionEffectsDigest,
    error::SuiError,
    storage::InputKey,
    transaction::{TransactionDataAPI, VerifiedCertificate},
};
use sui_types::{executable_transaction::VerifiedExecutableTransaction, fp_bail};
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::Instant;
use tracing::{error, info, instrument, trace, warn};

use crate::authority::AuthorityMetrics;
use crate::{
    authority::authority_per_epoch_store::AuthorityPerEpochStore,
    execution_cache::ExecutionCacheRead,
};
use sui_types::transaction::SenderSignedData;
use tap::TapOptional;

#[cfg(test)]
#[path = "unit_tests/transaction_manager_tests.rs"]
mod transaction_manager_tests;

/// Minimum capacity of HashMaps used in TransactionManager.
const MIN_HASHMAP_CAPACITY: usize = 1000;

// Reject a transaction if transaction manager queue length is above this threshold.
// 100_000 = 10k TPS * 5s resident time in transaction manager (pending + executing) * 2.
pub(crate) const MAX_TM_QUEUE_LENGTH: usize = 100_000;

// Reject a transaction if the number of pending transactions depending on the object
// is above the threshold.
pub(crate) const MAX_PER_OBJECT_QUEUE_LENGTH: usize = 200;

/// TransactionManager is responsible for managing object dependencies of pending transactions,
/// and publishing a stream of certified transactions (certificates) ready to execute.
/// It receives certificates from Narwhal, validator RPC handlers, and checkpoint executor.
/// Execution driver subscribes to the stream of ready certificates from TransactionManager, and
/// executes them in parallel.
/// The actual execution logic is inside AuthorityState. After a transaction commits and updates
/// storage, committed objects and certificates are notified back to TransactionManager.
pub struct TransactionManager {
    cache_read: Arc<dyn ExecutionCacheRead>,
    tx_ready_certificates: UnboundedSender<PendingCertificate>,
    metrics: Arc<AuthorityMetrics>,
    inner: RwLock<Inner>,
}

#[derive(Clone, Debug)]
pub struct PendingCertificateStats {
    // The time this certificate enters transaction manager.
    pub enqueue_time: Instant,
    // The time this certificate becomes ready for execution.
    pub ready_time: Option<Instant>,
}

#[derive(Clone, Debug)]
pub struct PendingCertificate {
    // Certified transaction to be executed.
    pub certificate: VerifiedExecutableTransaction,
    // When executing from checkpoint, the certified effects digest is provided, so that forks can
    // be detected prior to committing the transaction.
    pub expected_effects_digest: Option<TransactionEffectsDigest>,
    // The input object this certifiate is waiting for to become available in order to be executed.
    pub waiting_input_objects: BTreeSet<InputKey>,
    // Stores stats about this transaction.
    pub stats: PendingCertificateStats,
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
        if let Some(version) = object.version() {
            if let Some((previous_id, previous_version)) =
                self.versioned_cache.push(object.id(), version)
            {
                if previous_id == object.id() && previous_version > version {
                    // do not allow highest known version to decrease
                    // This should not be possible unless bugs are introduced elsewhere in this
                    // module.
                    self.versioned_cache.put(object.id(), previous_version);
                } else {
                    self.metrics
                        .transaction_manager_object_cache_evictions
                        .inc();
                }
            }
            self.metrics
                .transaction_manager_object_cache_size
                .set(self.versioned_cache.len() as i64);
        } else if let Some((previous_id, _)) = self.unversioned_cache.push(object.id(), ()) {
            // lru_cache will does not check if the value being evicted is the same as the value
            // being inserted, so we do need to check if the id is different before counting this
            // as an eviction.
            if previous_id != object.id() {
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
        if let Some(version) = object.version() {
            if let Some(current) = self.versioned_cache.get(&object.id()) {
                self.metrics.transaction_manager_object_cache_hits.inc();
                Some(*current >= version)
            } else {
                self.metrics.transaction_manager_object_cache_misses.inc();
                None
            }
        } else {
            self.unversioned_cache
                .get(&object.id())
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

    // Maps missing input objects to transactions in pending_certificates.
    missing_inputs: HashMap<InputKey, BTreeSet<TransactionDigest>>,

    // Stores age info for all transactions depending on each object.
    // Used for throttling signing and submitting transactions depending on hot objects.
    // An `IndexMap` is used to ensure that the insertion order is preserved.
    input_objects: HashMap<ObjectID, IndexMap<TransactionDigest, Instant>>,

    // Maps object IDs to the highest observed sequence number of the object. When the value is
    // None, indicates that the object is immutable, corresponding to an InputKey with no sequence
    // number.
    available_objects_cache: AvailableObjectsCache,

    // A transaction enqueued to TransactionManager must be in either pending_certificates or
    // executing_certificates.

    // Maps transaction digests to their content and missing input objects.
    pending_certificates: HashMap<TransactionDigest, PendingCertificate>,

    // Transactions that have all input objects available, but have not finished execution.
    executing_certificates: HashSet<TransactionDigest>,
}

impl Inner {
    fn new(epoch: EpochId, metrics: Arc<AuthorityMetrics>) -> Inner {
        Inner {
            epoch,
            missing_inputs: HashMap::with_capacity(MIN_HASHMAP_CAPACITY),
            input_objects: HashMap::with_capacity(MIN_HASHMAP_CAPACITY),
            available_objects_cache: AvailableObjectsCache::new(metrics),
            pending_certificates: HashMap::with_capacity(MIN_HASHMAP_CAPACITY),
            executing_certificates: HashSet::with_capacity(MIN_HASHMAP_CAPACITY),
        }
    }

    // Checks if there is any transaction waiting on `input_key`. Returns all the pending
    // transactions that are ready to be executed.
    // Must ensure input_key is available in storage before calling this function.
    fn find_ready_transactions(
        &mut self,
        input_key: InputKey,
        update_cache: bool,
        metrics: &Arc<AuthorityMetrics>,
    ) -> Vec<PendingCertificate> {
        if update_cache {
            self.available_objects_cache.insert(&input_key);
        }

        let mut ready_certificates = Vec::new();

        let Some(digests) = self.missing_inputs.remove(&input_key) else {
            // No transaction is waiting on the object yet.
            return ready_certificates;
        };

        let input_txns = self
            .input_objects
            .get_mut(&input_key.id())
            .unwrap_or_else(|| {
                panic!(
                    "# of transactions waiting on object {:?} cannot be 0",
                    input_key.id()
                )
            });
        for digest in digests.iter() {
            let age_opt = input_txns.shift_remove(digest);
            // The digest of the transaction must be inside the map.
            assert!(age_opt.is_some());
            metrics
                .transaction_manager_transaction_queue_age_s
                .observe(age_opt.unwrap().elapsed().as_secs_f64());
        }

        if input_txns.is_empty() {
            self.input_objects.remove(&input_key.id());
        }

        for digest in digests {
            // Pending certificate must exist.
            let pending_cert = self.pending_certificates.get_mut(&digest).unwrap();
            assert!(pending_cert.waiting_input_objects.remove(&input_key));
            // When a certificate has all its input objects, it is ready to execute.
            if pending_cert.waiting_input_objects.is_empty() {
                let pending_cert = self.pending_certificates.remove(&digest).unwrap();
                ready_certificates.push(pending_cert);
            } else {
                // TODO: we should start logging this at a higher level after some period of
                // time has elapsed.
                trace!(tx_digest = ?digest,missing = ?pending_cert.waiting_input_objects, "Certificate waiting on missing inputs");
            }
        }

        ready_certificates
    }

    fn maybe_reserve_capacity(&mut self) {
        self.missing_inputs.maybe_reserve_capacity();
        self.input_objects.maybe_reserve_capacity();
        self.pending_certificates.maybe_reserve_capacity();
        self.executing_certificates.maybe_reserve_capacity();
    }

    /// After reaching 1/4 load in hashmaps, decrease capacity to increase load to 1/2.
    fn maybe_shrink_capacity(&mut self) {
        self.missing_inputs.maybe_shrink_capacity();
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
        cache_read: Arc<dyn ExecutionCacheRead>,
        epoch_store: &AuthorityPerEpochStore,
        tx_ready_certificates: UnboundedSender<PendingCertificate>,
        metrics: Arc<AuthorityMetrics>,
    ) -> TransactionManager {
        let transaction_manager = TransactionManager {
            cache_read,
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
    #[instrument(level = "trace", skip_all)]
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

    #[instrument(level = "trace", skip_all)]
    pub(crate) fn enqueue(
        &self,
        certs: Vec<VerifiedExecutableTransaction>,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult<()> {
        let certs = certs.into_iter().map(|cert| (cert, None)).collect();
        self.enqueue_impl(certs, epoch_store)
    }

    #[instrument(level = "trace", skip_all)]
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
                    .cache_read
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
        let mut receiving_objects: HashSet<InputKey> = HashSet::new();
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
                let mut input_object_keys =
                    epoch_store.get_input_object_keys(&digest, &input_object_kinds);

                if input_object_kinds.len() != input_object_keys.len() {
                    error!("Duplicated input objects: {:?}", input_object_kinds);
                }

                let receiving_object_entries =
                    cert.data().intent_message().value.receiving_objects();
                for entry in receiving_object_entries {
                    let key = InputKey::VersionedObject {
                        id: entry.0,
                        version: entry.1,
                    };
                    receiving_objects.insert(key);
                    input_object_keys.insert(key);
                }

                for key in input_object_keys.iter() {
                    object_availability.insert(*key, None);
                }

                (cert, fx_digest, input_object_keys)
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
        // So missing objects' availability are checked again after acquiring TM lock.
        let cache_miss_availability = self
            .cache_read
            .multi_input_objects_available(
                &input_object_cache_misses,
                receiving_objects,
                epoch_store.epoch(),
            )
            .expect("Checking object existence cannot fail!")
            .into_iter()
            .zip(input_object_cache_misses);

        // After this point, the function cannot return early and must run to the end. Otherwise,
        // it can lead to data inconsistencies and potentially some transactions will never get
        // executed.

        // Internal lock is held only for updating the internal state.
        let mut inner = self.inner.write();

        let _scope = monitored_scope("TransactionManager::enqueue::wlock");

        for (available, key) in cache_miss_availability {
            if available && key.version().is_none() {
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
        // read cache_miss_availability - because the cache is unbounded mode it is guaranteed to
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
        let pending_cert_enqueue_time = Instant::now();

        for (cert, expected_effects_digest, input_object_keys) in certs {
            pending.push(PendingCertificate {
                certificate: cert,
                expected_effects_digest,
                waiting_input_objects: input_object_keys,
                stats: PendingCertificateStats {
                    enqueue_time: pending_cert_enqueue_time,
                    ready_time: None,
                },
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
            if inner.executing_certificates.contains(&digest) {
                self.metrics
                    .transaction_manager_num_enqueued_certificates
                    .with_label_values(&["already_executing"])
                    .inc();
                continue;
            }
            // skip already executed txes
            if self.cache_read.is_tx_already_executed(&digest)? {
                // also ensure the transaction will not be retried after restart.
                let _ = epoch_store.remove_pending_execution(&digest);
                self.metrics
                    .transaction_manager_num_enqueued_certificates
                    .with_label_values(&["already_executed"])
                    .inc();
                continue;
            }

            let mut waiting_input_objects = BTreeSet::new();
            std::mem::swap(
                &mut waiting_input_objects,
                &mut pending_cert.waiting_input_objects,
            );
            for key in waiting_input_objects {
                if !object_availability[&key].unwrap() {
                    // The input object is not yet available.
                    pending_cert.waiting_input_objects.insert(key);

                    assert!(
                        inner.missing_inputs.entry(key).or_default().insert(digest),
                        "Duplicated certificate {:?} for missing object {:?}",
                        digest,
                        key
                    );
                    let input_txns = inner.input_objects.entry(key.id()).or_default();
                    input_txns.insert(digest, pending_cert_enqueue_time);
                }
            }

            // Ready transactions can start to execute.
            if pending_cert.waiting_input_objects.is_empty() {
                self.metrics
                    .transaction_manager_num_enqueued_certificates
                    .with_label_values(&["ready"])
                    .inc();
                pending_cert.stats.ready_time = Some(Instant::now());
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
            .set(inner.missing_inputs.len() as i64);
        self.metrics
            .transaction_manager_num_pending_certificates
            .set(inner.pending_certificates.len() as i64);

        inner.maybe_reserve_capacity();

        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn objects_available(
        &self,
        input_keys: Vec<InputKey>,
        epoch_store: &AuthorityPerEpochStore,
    ) {
        let mut inner = self.inner.write();
        let _scope = monitored_scope("TransactionManager::objects_available::wlock");
        self.objects_available_locked(&mut inner, epoch_store, input_keys, true, Instant::now());
        inner.maybe_shrink_capacity();
    }

    #[instrument(level = "trace", skip_all)]
    fn objects_available_locked(
        &self,
        inner: &mut Inner,
        epoch_store: &AuthorityPerEpochStore,
        input_keys: Vec<InputKey>,
        update_cache: bool,
        available_time: Instant,
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
            for mut ready_cert in
                inner.find_ready_transactions(input_key, update_cache, &self.metrics)
            {
                ready_cert.stats.ready_time = Some(available_time);
                self.certificate_ready(inner, ready_cert);
            }
        }

        self.metrics
            .transaction_manager_num_missing_objects
            .set(inner.missing_inputs.len() as i64);
        self.metrics
            .transaction_manager_num_pending_certificates
            .set(inner.pending_certificates.len() as i64);
        self.metrics
            .transaction_manager_num_executing_certificates
            .set(inner.executing_certificates.len() as i64);
    }

    /// Notifies TransactionManager about a transaction that has been committed.
    #[instrument(level = "trace", skip_all)]
    pub(crate) fn notify_commit(
        &self,
        digest: &TransactionDigest,
        output_object_keys: Vec<InputKey>,
        epoch_store: &AuthorityPerEpochStore,
    ) {
        {
            let commit_time = Instant::now();
            let mut inner = self.inner.write();
            let _scope = monitored_scope("TransactionManager::notify_commit::wlock");

            if inner.epoch != epoch_store.epoch() {
                warn!("Ignoring committed certificate from wrong epoch. Expected={} Actual={} CertificateDigest={:?}", inner.epoch, epoch_store.epoch(), digest);
                return;
            }

            self.objects_available_locked(
                &mut inner,
                epoch_store,
                output_object_keys,
                true,
                commit_time,
            );

            if !inner.executing_certificates.remove(digest) {
                trace!("{:?} not found in executing certificates, likely because it is a system transaction", digest);
                return;
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
        trace!(tx_digest = ?pending_certificate.certificate.digest(), "certificate ready");
        assert_eq!(pending_certificate.waiting_input_objects.len(), 0);
        // Record as an executing certificate.
        assert!(inner
            .executing_certificates
            .insert(*pending_certificate.certificate.digest()));
        self.metrics.txn_ready_rate_tracker.lock().record();
        let _ = self.tx_ready_certificates.send(pending_certificate);
        self.metrics.transaction_manager_num_ready.inc();
        self.metrics.execution_driver_dispatch_queue.inc();
    }

    /// Gets the missing input object keys for the given transaction.
    pub(crate) fn get_missing_input(&self, digest: &TransactionDigest) -> Option<Vec<InputKey>> {
        let inner = self.inner.read();
        inner
            .pending_certificates
            .get(digest)
            .map(|cert| cert.waiting_input_objects.clone().into_iter().collect())
    }

    // Returns the number of transactions waiting on each object ID, as well as the age of the oldest transaction in the queue.
    pub(crate) fn objects_queue_len_and_age(
        &self,
        keys: Vec<ObjectID>,
    ) -> Vec<(ObjectID, usize, Option<Duration>)> {
        let inner = self.inner.read();
        keys.into_iter()
            .map(|key| {
                let default_map = IndexMap::new();
                let txns = inner.input_objects.get(&key).unwrap_or(&default_map);
                (
                    key,
                    txns.len(),
                    txns.first().map(|(_, time)| time.elapsed()),
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

    pub(crate) fn check_execution_overload(
        &self,
        txn_age_threshold: Duration,
        tx_data: &SenderSignedData,
    ) -> SuiResult {
        // Too many transactions are pending execution.
        let inflight_queue_len = self.inflight_queue_len();
        fp_ensure!(
            inflight_queue_len < MAX_TM_QUEUE_LENGTH,
            SuiError::TooManyTransactionsPendingExecution {
                queue_len: inflight_queue_len,
                threshold: MAX_TM_QUEUE_LENGTH,
            }
        );
        tx_data.digest();

        for (object_id, queue_len, txn_age) in self.objects_queue_len_and_age(
            tx_data
                .transaction_data()
                .input_objects()?
                .into_iter()
                .map(|r| r.object_id())
                .collect(),
        ) {
            // When this occurs, most likely transactions piled up on a shared object.
            if queue_len >= MAX_PER_OBJECT_QUEUE_LENGTH {
                info!(
                    "Overload detected on object {:?} with {} pending transactions",
                    object_id, queue_len
                );
                fp_bail!(SuiError::TooManyTransactionsPendingOnObject {
                    object_id,
                    queue_len,
                    threshold: MAX_PER_OBJECT_QUEUE_LENGTH,
                });
            }
            if let Some(age) = txn_age {
                // Check that we don't have a txn that has been waiting for a long time in the queue.
                if age >= txn_age_threshold {
                    info!("Overload detected on object {:?} with oldest transaction pending for {} secs", object_id, age.as_secs());
                    fp_bail!(SuiError::TooOldTransactionPendingOnObject {
                        object_id,
                        txn_age_sec: age.as_secs(),
                        threshold: txn_age_threshold.as_secs(),
                    });
                }
            }
        }
        Ok(())
    }

    // Verify TM has no pending item for tests.
    #[cfg(test)]
    fn check_empty_for_testing(&self) {
        let inner = self.inner.read();
        assert!(
            inner.missing_inputs.is_empty(),
            "Missing inputs: {:?}",
            inner.missing_inputs
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

trait ResizableHashSet<K> {
    fn maybe_reserve_capacity(&mut self);
    fn maybe_shrink_capacity(&mut self);
}

impl<K> ResizableHashSet<K> for HashSet<K>
where
    K: std::cmp::Eq + std::hash::Hash,
{
    /// After reaching 3/4 load in hashset, increase capacity to decrease load to 1/2.
    fn maybe_reserve_capacity(&mut self) {
        if self.len() > self.capacity() * 3 / 4 {
            self.reserve(self.capacity() / 2);
        }
    }

    /// After reaching 1/4 load in hashset, decrease capacity to increase load to 1/2.
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
    #[cfg_attr(msim, ignore)]
    fn test_available_objects_cache() {
        let metrics = Arc::new(AuthorityMetrics::new(&Registry::default()));
        let mut cache = AvailableObjectsCache::new_with_size(metrics, 5);

        // insert 10 unique unversioned objects
        for i in 0..10 {
            let object = ObjectID::new([i; 32]);
            let input_key = InputKey::Package { id: object };
            assert_eq!(cache.is_object_available(&input_key), None);
            cache.insert(&input_key);
            assert_eq!(cache.is_object_available(&input_key), Some(true));
        }

        // first 5 have been evicted
        for i in 0..5 {
            let object = ObjectID::new([i; 32]);
            let input_key = InputKey::Package { id: object };
            assert_eq!(cache.is_object_available(&input_key), None);
        }

        // insert 10 unique versioned objects
        for i in 0..10 {
            let object = ObjectID::new([i; 32]);
            let input_key = InputKey::VersionedObject {
                id: object,
                version: (i as u64).into(),
            };
            assert_eq!(cache.is_object_available(&input_key), None);
            cache.insert(&input_key);
            assert_eq!(cache.is_object_available(&input_key), Some(true));
        }

        // first 5 versioned objects have been evicted
        for i in 0..5 {
            let object = ObjectID::new([i; 32]);
            let input_key = InputKey::VersionedObject {
                id: object,
                version: (i as u64).into(),
            };
            assert_eq!(cache.is_object_available(&input_key), None);
        }

        // but versioned objects do not cause evictions of unversioned objects
        for i in 5..10 {
            let object = ObjectID::new([i; 32]);
            let input_key = InputKey::Package { id: object };
            assert_eq!(cache.is_object_available(&input_key), Some(true));
        }

        // object 9 is available at version 9
        let object = ObjectID::new([9; 32]);
        let input_key = InputKey::VersionedObject {
            id: object,
            version: 9.into(),
        };
        assert_eq!(cache.is_object_available(&input_key), Some(true));
        // but not at version 10
        let input_key = InputKey::VersionedObject {
            id: object,
            version: 10.into(),
        };
        assert_eq!(cache.is_object_available(&input_key), Some(false));
        // it is available at version 8 (this case can be used by readonly shared objects)
        let input_key = InputKey::VersionedObject {
            id: object,
            version: 8.into(),
        };
        assert_eq!(cache.is_object_available(&input_key), Some(true));
    }
}
