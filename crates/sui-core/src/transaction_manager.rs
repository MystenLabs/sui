// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
};

use parking_lot::RwLock;
use sui_types::{
    base_types::ObjectID,
    committee::EpochId,
    messages::{TransactionDataAPI, VerifiedCertificate, VerifiedExecutableTransaction},
};
use sui_types::{base_types::TransactionDigest, error::SuiResult};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error, warn};

use crate::authority::{
    authority_per_epoch_store::AuthorityPerEpochStore, authority_store::InputKey,
};
use crate::authority::{AuthorityMetrics, AuthorityStore};

/// TransactionManager is responsible for managing object dependencies of pending transactions,
/// and publishing a stream of certified transactions (certificates) ready to execute.
/// It receives certificates from Narwhal, RPC, and checkpoint executor.
/// Executing driver subscribes to the stream of ready certificates from TransactionManager, and
/// executes them in parallel.
/// The actual execution logic is in AuthorityState. After a transaction commits and updates
/// storage, committed objects are notified back to TransactionManager.
pub struct TransactionManager {
    authority_store: Arc<AuthorityStore>,
    tx_ready_certificates: UnboundedSender<VerifiedExecutableTransaction>,
    metrics: Arc<AuthorityMetrics>,
    inner: RwLock<Inner>,
}

struct PendingCertificate {
    certificate: VerifiedExecutableTransaction,
    missing: BTreeSet<InputKey>,
}

#[derive(Default)]
struct Inner {
    // Current epoch of TransactionManager.
    epoch: EpochId,

    // Maps missing input objects to transactions in pending_certificates.
    // Note that except for immutable objects, a given key may only have one TransactionDigest in
    // the set. Unfortunately we cannot easily verify that this invariant is upheld, because you
    // cannot determine from TransactionData whether an input is mutable or immutable.
    missing_inputs: HashMap<InputKey, BTreeSet<TransactionDigest>>,

    // Number of transactions that depend on each object ID. Should match exactly with total
    // number of transactions per object ID prefix in the missing_inputs table.
    // Used for throttling signing and submitting transactions depending on hot objects.
    input_objects: HashMap<ObjectID, usize>,

    // A transaction enqueued to TransactionManager must be in either pending_certificates or
    // executing_certificates.

    // Maps transaction digests to their content and missing input objects.
    pending_certificates: HashMap<TransactionDigest, PendingCertificate>,
    // Transactions that have all input objects available, but have not finished execution.
    executing_certificates: HashSet<TransactionDigest>,
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
    /// Transactions from other sources, e.g. checkpoint executor, do not write to the table.
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
    /// REQUIRED: Shared object locks must be taken before calling this function on transactions
    /// containing shared objects!
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
        // First, determine missing input objects without lock.
        let mut pending = Vec::new();
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
            let input_object_kinds = cert.data().intent_message.value.input_objects()?;
            let input_object_keys = self.authority_store.get_input_object_keys(
                &digest,
                &input_object_kinds,
                epoch_store,
            );
            if input_object_kinds.len() != input_object_keys.len() {
                error!("Duplicated input objects: {:?}", input_object_kinds);
            }
            pending.push(PendingCertificate {
                certificate: cert,
                missing: input_object_keys
                    .into_iter()
                    .filter(|key| {
                        !self
                            .authority_store
                            .input_object_exists(key)
                            .expect("Checking object existence cannot fail!")
                    })
                    .collect(),
            });
        }

        // After this point, the function cannot return early and must run to the end. Otherwise,
        // it can lead to data inconsistencies and potentially some transactions will never get
        // executed.

        let mut missing_input_objects = Vec::new();

        // Internal lock is held only for updating the internal state.
        let mut inner = self.inner.write();

        for pending_cert in pending {
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
            if self.authority_store.is_tx_already_executed(&digest)? {
                // also ensure the transaction will not be retried after restart.
                let _ = epoch_store.remove_pending_execution(&digest);
                self.metrics
                    .transaction_manager_num_enqueued_certificates
                    .with_label_values(&["already_executed"])
                    .inc();
                continue;
            }
            // Ready transactions can start to execute.
            if pending_cert.missing.is_empty() {
                self.metrics
                    .transaction_manager_num_enqueued_certificates
                    .with_label_values(&["ready"])
                    .inc();
                self.certificate_ready(pending_cert.certificate);
                continue;
            }

            missing_input_objects.extend(pending_cert.missing.iter().cloned());
            for input in &pending_cert.missing {
                assert!(
                    inner
                        .missing_inputs
                        .entry(*input)
                        .or_default()
                        .insert(digest),
                    "Duplicated certificate {:?} for missing object {:?}",
                    digest,
                    input
                );
                let input_count = inner.input_objects.entry(input.0).or_default();
                *input_count += 1;
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

        // Rechecking missing input objects is needed, because some objects could have become
        // available between the 1st time objects existence were checked, and the objects getting
        // added into the missing_input table.
        // In the likely common case, missing_input_objects is empty and no check is needed.
        if !missing_input_objects.is_empty() {
            let available_objects: Vec<_> = missing_input_objects
                .into_iter()
                .filter(|key| {
                    self.authority_store
                        .input_object_exists(key)
                        .expect("Checking object existence cannot fail!")
                })
                .collect();
            self.objects_available(available_objects, epoch_store);
        }

        Ok(())
    }

    /// Notifies TransactionManager that the given objects are available in the objects table.
    pub(crate) fn objects_available(
        &self,
        input_keys: Vec<InputKey>,
        epoch_store: &AuthorityPerEpochStore,
    ) {
        let mut ready_digests = Vec::new();

        let inner = &mut self.inner.write();
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
            let Some(digests) = inner.missing_inputs.remove(&input_key) else {
                continue;
            };

            // Clean up object ID count table.
            let input_count = inner.input_objects.get_mut(&input_key.0).unwrap();
            *input_count -= digests.len();
            if *input_count == 0 {
                inner.input_objects.remove(&input_key.0);
            }

            for digest in digests {
                // Pending certificate must exist.
                let pending_cert = inner.pending_certificates.get_mut(&digest).unwrap();
                assert!(pending_cert.missing.remove(&input_key));
                // When a certificate has no missing input, it is ready to execute.
                if pending_cert.missing.is_empty() {
                    debug!(tx_digest = ?digest, "certificate ready");
                    let pending_cert = inner.pending_certificates.remove(&digest).unwrap();
                    assert!(inner.executing_certificates.insert(digest));
                    ready_digests.push(digest);
                    self.certificate_ready(pending_cert.certificate);
                } else {
                    debug!(tx_digest = ?digest, missing = ?pending_cert.missing, "Certificate waiting on missing inputs");
                }
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

    /// Notifies TransactionManager about a certificate that has been executed.
    pub(crate) fn certificate_executed(
        &self,
        digest: &TransactionDigest,
        epoch_store: &AuthorityPerEpochStore,
    ) {
        {
            let inner = &mut self.inner.write();
            if inner.epoch != epoch_store.epoch() {
                warn!("Ignoring committed certificate from wrong epoch. Expected={} Actual={} CertificateDigest={:?}", inner.epoch, epoch_store.epoch(), digest);
                return;
            }
            inner.executing_certificates.remove(digest);
            self.metrics
                .transaction_manager_num_executing_certificates
                .set(inner.executing_certificates.len() as i64);
        }
        let _ = epoch_store.remove_pending_execution(digest);
    }

    /// Sends the ready certificate for execution.
    fn certificate_ready(&self, certificate: VerifiedExecutableTransaction) {
        self.metrics.transaction_manager_num_ready.inc();
        let _ = self.tx_ready_certificates.send(certificate);
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

    // Reconfigures the TransactionManager for a new epoch. Existing transactions will be dropped
    // because they are no longer relevant and may be incorrect in the new epoch.
    pub(crate) fn reconfigure(&self, new_epoch: EpochId) {
        let mut inner = self.inner.write();
        *inner = Inner::new(new_epoch);
    }
}
