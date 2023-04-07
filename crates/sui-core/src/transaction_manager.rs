// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

use mysten_metrics::monitored_scope;
use parking_lot::RwLock;
use sui_types::{
    base_types::ObjectID,
    committee::EpochId,
    messages::{TransactionDataAPI, VerifiedCertificate, VerifiedExecutableTransaction},
};
use sui_types::{base_types::TransactionDigest, error::SuiResult};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error, trace, warn};

use crate::authority::{
    authority_per_epoch_store::AuthorityPerEpochStore,
    authority_store::{InputKey, LockMode},
};
use crate::authority::{AuthorityMetrics, AuthorityStore};

/// TransactionManager is responsible for managing object dependencies of pending transactions,
/// and publishing a stream of certified transactions (certificates) ready to execute.
/// It receives certificates from Narwhal, validator RPC handlers, and checkpoint executor.
/// Execution driver subscribes to the stream of ready certificates from TransactionManager, and
/// executes them in parallel.
/// The actual execution logic is inside AuthorityState. After a transaction commits and updates
/// storage, committed objects and certificates are notified back to TransactionManager.
pub struct TransactionManager {
    authority_store: Arc<AuthorityStore>,
    tx_ready_certificates: UnboundedSender<VerifiedExecutableTransaction>,
    metrics: Arc<AuthorityMetrics>,
    inner: RwLock<Inner>,
}

#[derive(Clone, Debug)]
struct PendingCertificate {
    // Certified transaction to be executed.
    certificate: VerifiedExecutableTransaction,
    // Input object locks that have not been acquired, because:
    // 1. The object has not been created yet.
    // 2. The object exists, but this transaction is trying to acquire a r/w lock while the object
    // is held in ro locks by other transaction(s).
    acquiring_locks: BTreeMap<InputKey, LockMode>,
    // Input object locks that have been acquired.
    acquired_locks: BTreeMap<InputKey, LockMode>,
}

/// LockQueue is a queue of transactions waiting or holding a lock on an object.
#[derive(Default)]
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
    fn has_no_readonly(&self) -> bool {
        self.readonly_waiters.is_empty() && self.readonly_holders.is_empty()
    }

    fn is_empty(&self) -> bool {
        self.readonly_waiters.is_empty()
            && self.readonly_holders.is_empty()
            && self.default_waiters.is_empty()
    }
}

#[derive(Default)]
struct Inner {
    // Current epoch of TransactionManager.
    epoch: EpochId,

    // Maps input objects to transactions waiting for locks on the object.
    lock_waiters: HashMap<InputKey, LockQueue>,

    // Number of transactions that depend on each object ID. Should match exactly with total
    // number of transactions per object ID prefix in the missing_inputs table.
    // Used for throttling signing and submitting transactions depending on hot objects.
    input_objects: HashMap<ObjectID, usize>,

    // A transaction enqueued to TransactionManager must be in either pending_certificates or
    // executing_certificates.

    // Maps transaction digests to their content and missing input objects.
    pending_certificates: HashMap<TransactionDigest, PendingCertificate>,
    // Maps executing transaction digests to their acquired input object locks.
    executing_certificates: HashMap<TransactionDigest, BTreeMap<InputKey, LockMode>>,
}

impl Inner {
    fn new(epoch: EpochId) -> Inner {
        Inner {
            epoch,
            ..Default::default()
        }
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
        tx_ready_certificates: UnboundedSender<VerifiedExecutableTransaction>,
        metrics: Arc<AuthorityMetrics>,
    ) -> TransactionManager {
        let transaction_manager = TransactionManager {
            authority_store,
            metrics,
            inner: RwLock::new(Inner::new(epoch_store.epoch())),
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
        let mut pending = Vec::new();
        // Check input objects availability, before taking TM lock.
        let mut object_availability: HashMap<InputKey, bool> = HashMap::new();
        for cert in certs {
            let digest = *cert.digest();
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
            let input_object_kinds = cert.data().intent_message().value.input_objects()?;
            let input_object_locks = self.authority_store.get_input_object_locks(
                &digest,
                &input_object_kinds,
                epoch_store,
            );
            if input_object_kinds.len() != input_object_locks.len() {
                error!("Duplicated input objects: {:?}", input_object_kinds);
            }

            for key in input_object_locks.keys() {
                // Checking object availability without holding TM lock to reduce contention.
                // But input objects can become available before TM lock is acquired.
                // So missing objects' availability are checked again after releasing the TM lock.
                if !object_availability.contains_key(key) {
                    object_availability.insert(
                        *key,
                        self.authority_store
                            .input_object_exists(key)
                            .expect("Checking object existence cannot fail!"),
                    );
                }
            }

            pending.push(PendingCertificate {
                certificate: cert,
                acquiring_locks: input_object_locks,
                acquired_locks: BTreeMap::new(),
            });
        }

        // After this point, the function cannot return early and must run to the end. Otherwise,
        // it can lead to data inconsistencies and potentially some transactions will never get
        // executed.

        // Internal lock is held only for updating the internal state.
        let mut inner = self.inner.write();
        let _scope = monitored_scope("TransactionManager::enqueue::wlock");

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
                let lock_queue = inner.lock_waiters.entry(key).or_default();
                if !object_availability[&key] {
                    // 1. The input object is not yet available.
                    acquire = true;
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
                            if !lock_queue.has_no_readonly() {
                                acquire = true;
                                assert!(lock_queue.default_waiters.insert(digest));
                            }
                        }
                        LockMode::ReadOnly => {
                            // Acquired readonly locks need to be tracked until the transaction has
                            // finished execution.
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

        // Unnecessary to keep holding the lock while re-checking input object existence.
        drop(inner);

        // An object will not remain forever as a missing input in TransactionManager,
        // if the object is or later becomes available in storage, because:
        // 1. At this point the object either exists in storage or not.
        // 2. If the object exists in storage, it will be found via input_object_exists() below.
        // 3. If it is not and but becomes available eventually, the transaction commit logic will
        //    call objects_available() on the object.
        // 4. If the node crashes after the object is created but before the transaction consuming
        //    it finishes, on restart the object will be found for the transaction.

        // Rechecking previously missing input objects is necessary, because some objects could
        // have become available between the initial availability check and here.
        // In the likely common case, all objects in object_availability are available and no
        // additional check is needed here.
        let additional_available_objects: Vec<_> = object_availability
            .iter()
            .filter_map(|(key, available)| {
                // Previously available object does not need to be rechecked.
                if *available {
                    return None;
                }
                if self
                    .authority_store
                    .input_object_exists(key)
                    .expect("Checking object existence cannot fail!")
                {
                    Some(*key)
                } else {
                    None
                }
            })
            .collect();
        if !additional_available_objects.is_empty() {
            self.objects_available(additional_available_objects, epoch_store);
        }

        Ok(())
    }

    /// Notifies TransactionManager that the given objects are available in the objects table.
    pub(crate) fn objects_available(
        &self,
        input_keys: Vec<InputKey>,
        epoch_store: &AuthorityPerEpochStore,
    ) {
        let mut inner = self.inner.write();
        let _scope = monitored_scope("TransactionManager::objects_available::wlock");
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
            let Some(lock_queue) = inner.lock_waiters.get_mut(&input_key) else {
                // No transaction is waiting on the object yet.
                continue;
            };

            // Waiters can acquire lock in eitehr readonly or default mode.
            let mut digests = BTreeSet::new();
            if !lock_queue.readonly_waiters.is_empty() {
                std::mem::swap(&mut digests, &mut lock_queue.readonly_waiters);
                lock_queue.readonly_holders.extend(digests.iter().cloned());
            } else if lock_queue.readonly_holders.is_empty() {
                // Only acquire default lock if there is no readonly lock waiter / holder.
                std::mem::swap(&mut digests, &mut lock_queue.default_waiters);
            };
            if lock_queue.is_empty() {
                inner.lock_waiters.remove(&input_key);
            }

            self.lock_acquired(&mut inner, digests, input_key);
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

    /// Notifies TransactionManager about a certificate that has been executed.
    pub(crate) fn certificate_executed(
        &self,
        digest: &TransactionDigest,
        epoch_store: &AuthorityPerEpochStore,
    ) {
        {
            let mut inner = self.inner.write();
            let _scope = monitored_scope("TransactionManager::certificate_executed::wlock");
            if inner.epoch != epoch_store.epoch() {
                warn!("Ignoring committed certificate from wrong epoch. Expected={} Actual={} CertificateDigest={:?}", inner.epoch, epoch_store.epoch(), digest);
                return;
            }
            let Some(acquired_locks) = inner.executing_certificates.remove(digest) else {
                panic!("Certificate {:?} not found in executing certificates", digest);
            };
            for (key, lock_mode) in &acquired_locks {
                if lock_mode == &LockMode::Default {
                    // Holders of default locks are not tracked.
                    continue;
                }
                assert_eq!(lock_mode, &LockMode::ReadOnly);
                let lock_queue = inner.lock_waiters.get_mut(key).unwrap();
                assert!(
                    lock_queue.readonly_holders.remove(digest),
                    "Certificate {:?} not found among readonly lock holders",
                    digest
                );
                if lock_queue.has_no_readonly() {
                    let lock_queue = inner.lock_waiters.remove(key).unwrap();
                    self.lock_acquired(&mut inner, lock_queue.default_waiters, *key);
                }
            }
            self.metrics
                .transaction_manager_num_executing_certificates
                .set(inner.executing_certificates.len() as i64);
        }
        let _ = epoch_store.remove_pending_execution(digest);
    }

    /// Sends the ready certificate for execution.
    fn certificate_ready(&self, inner: &mut Inner, pending_certificate: PendingCertificate) {
        let cert = pending_certificate.certificate;
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
        let _ = self.tx_ready_certificates.send(cert);
        self.metrics.transaction_manager_num_ready.inc();
        self.metrics.execution_driver_dispatch_queue.inc();
    }

    // Updates transactions to acquire lock with input_key.
    fn lock_acquired(
        &self,
        inner: &mut Inner,
        digests: BTreeSet<TransactionDigest>,
        input_key: InputKey,
    ) {
        if digests.is_empty() {
            return;
        }

        let input_count = inner.input_objects.get_mut(&input_key.0).unwrap();
        *input_count -= digests.len();
        if *input_count == 0 {
            inner.input_objects.remove(&input_key.0);
        }

        for digest in digests {
            // Pending certificate must exist.
            let pending_cert = inner.pending_certificates.get_mut(&digest).unwrap();
            let lock_mode = pending_cert.acquiring_locks.remove(&input_key).unwrap();
            assert!(pending_cert
                .acquired_locks
                .insert(input_key, lock_mode)
                .is_none());
            // When a certificate has all locks acquired, it is ready to execute.
            if pending_cert.acquiring_locks.is_empty() {
                let pending_cert = inner.pending_certificates.remove(&digest).unwrap();
                self.certificate_ready(inner, pending_cert);
            } else {
                // TODO: we should start logging this at a higher level after some period of
                // time has elapsed.
                debug!(tx_digest = ?digest,acquiring = ?pending_cert.acquiring_locks, "Certificate acquiring locks");
            }
        }
    }

    /// Gets the missing input object keys for the given transaction.
    pub(crate) fn get_missing_input(&self, digest: &TransactionDigest) -> Option<Vec<InputKey>> {
        let inner = self.inner.read();
        inner
            .pending_certificates
            .get(digest)
            .map(|cert| cert.acquiring_locks.keys().cloned().into_iter().collect())
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

    // Returns the number of certificates pending execution or being executed by the execution driver right now.
    pub(crate) fn execution_queue_len(&self) -> usize {
        let inner = self.inner.read();
        inner.pending_certificates.len() + inner.executing_certificates.len()
    }

    // Reconfigures the TransactionManager for a new epoch. Existing transactions will be dropped
    // because they are no longer relevant and may be incorrect in the new epoch.
    pub(crate) fn reconfigure(&self, new_epoch: EpochId) {
        let mut inner = self.inner.write();
        *inner = Inner::new(new_epoch);
    }
}
