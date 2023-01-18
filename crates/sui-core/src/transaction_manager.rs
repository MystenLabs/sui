// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use parking_lot::Mutex;
use sui_types::storage::ObjectKey;
use sui_types::{base_types::TransactionDigest, error::SuiResult, messages::VerifiedCertificate};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error, warn};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::{AuthorityMetrics, AuthorityStore};

/// TransactionManager is responsible for managing pending certificates and publishes a stream
/// of certificates ready to be executed. It works together with AuthorityState for receiving
/// pending certificates, and getting notified about committed objects. Executing driver
/// subscribes to the stream of ready certificates published by the TransactionManager, and can
/// execute them in parallel.
/// TODO: use TransactionManager for fullnode.
pub struct TransactionManager {
    authority_store: Arc<AuthorityStore>,
    tx_ready_certificates: UnboundedSender<VerifiedCertificate>,
    metrics: Arc<AuthorityMetrics>,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    // Maps missing input objects to transactions in pending_certificates.
    // Note that except for immutable objects, a given key may only have one TransactionDigest in
    // the set. Unfortunately we cannot easily verify that this invariant is upheld, because you
    // cannot determine from TransactionData whether an input is mutable or immutable.
    missing_inputs: BTreeMap<ObjectKey, BTreeSet<TransactionDigest>>,

    // A transaction enqueued to TransactionManager must be in either pending_certificates or
    // executing_certificates.

    // Maps transactions to their missing input objects.
    pending_certificates: BTreeMap<TransactionDigest, BTreeSet<ObjectKey>>,
    // Transactions that have all input objects available, but have not finished execution.
    executing_certificates: BTreeSet<TransactionDigest>,
}

impl TransactionManager {
    /// If a node restarts, transaction manager recovers in-memory data from pending certificates and
    /// other persistent data.
    pub(crate) fn new(
        authority_store: Arc<AuthorityStore>,
        epoch_store: &AuthorityPerEpochStore,
        tx_ready_certificates: UnboundedSender<VerifiedCertificate>,
        metrics: Arc<AuthorityMetrics>,
    ) -> TransactionManager {
        let transaction_manager = TransactionManager {
            authority_store,
            metrics,
            inner: Default::default(),
            tx_ready_certificates,
        };
        transaction_manager
            .enqueue(epoch_store.all_pending_certificates().unwrap(), epoch_store)
            .expect("Initialize TransactionManager with pending certificates failed.");
        transaction_manager
    }

    /// Enqueues certificates into TransactionManager. Once all of the input objects are available
    /// locally for a certificate, the certified transaction will be sent to execution driver.
    ///
    /// REQUIRED: Shared object locks must be taken before calling this function on shared object
    /// transactions!
    ///
    /// TODO: it may be less error prone to take shared object locks inside this function, or
    /// require shared object lock versions get passed in as input. But this function should not
    /// have many callsites. Investigate the alternatives here.
    pub(crate) fn enqueue(
        &self,
        certs: Vec<VerifiedCertificate>,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult<()> {
        let inner = &mut self.inner.lock();
        for cert in certs {
            let digest = *cert.digest();

            if epoch_store
                .is_poison_pill_tx(&digest)
                .expect("db read cannot fail")
            {
                warn!(tx_digest = ?digest, "refusing to enqueue poison pill transaction");
                continue;
            }

            // hold the tx lock until we have finished checking if objects are missing, so that we
            // don't race with a concurrent execution of this tx.
            let _tx_lock = epoch_store.acquire_tx_lock(&digest);

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
            if self.authority_store.effects_exists(&digest)? {
                // also ensure the transaction will not be retried after restart.
                let _ = epoch_store.remove_pending_certificate(&digest);
                self.metrics
                    .transaction_manager_num_enqueued_certificates
                    .with_label_values(&["already_executed"])
                    .inc();
                continue;
            }

            let missing = self
                .authority_store
                .get_missing_input_objects(
                    &digest,
                    &cert.data().intent_message.value.input_objects()?,
                    epoch_store,
                )
                .expect("Are shared object locks set prior to enqueueing certificates?");

            if missing.is_empty() {
                debug!(tx_digest = ?digest, "certificate ready");
                assert!(inner.executing_certificates.insert(digest));
                self.certificate_ready(cert);
                self.metrics
                    .transaction_manager_num_enqueued_certificates
                    .with_label_values(&["ready"])
                    .inc();
                continue;
            }

            // A missing input object in TransactionManager will definitely be notified via
            // objects_committed(), when the object actually gets committed, because:
            // 1. Assume rocksdb is strongly consistent, writing the object to the objects
            // table must happen after not finding the object in get_missing_input_objects().
            // 2. Notification via objects_committed() will happen after an object is written
            // into the objects table.
            // 3. TransactionManager is protected by a mutex. The notification via
            // objects_committed() can only arrive after the current enqueue() call finishes.
            debug!(tx_digest = ?digest, ?missing, "certificate waiting on missing objects");

            for objkey in missing.iter() {
                debug!(?objkey, ?digest, "adding missing object entry");
                assert!(
                    inner
                        .missing_inputs
                        .entry(*objkey)
                        .or_default()
                        .insert(digest),
                    "Duplicated certificate {:?} for missing object {:?}",
                    digest,
                    objkey
                );
            }

            assert!(
                inner
                    .pending_certificates
                    .insert(digest, missing.into_iter().collect())
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
        Ok(())
    }

    /// Notifies TransactionManager that the given objects have been committed.
    pub(crate) fn objects_committed(
        &self,
        object_keys: Vec<ObjectKey>,
        epoch_store: &AuthorityPerEpochStore,
    ) {
        let mut ready_digests = Vec::new();

        {
            let inner = &mut self.inner.lock();
            for object_key in object_keys {
                let Some(digests) = inner.missing_inputs.remove(&object_key) else {
                    continue;
                };

                for digest in digests.iter() {
                    let set = inner.pending_certificates.entry(*digest).or_default();
                    set.remove(&object_key);
                    // This certificate has no missing input. It is ready to execute.
                    if set.is_empty() {
                        debug!(tx_digest = ?digest, "certificate ready");
                        inner.pending_certificates.remove(digest);
                        assert!(inner.executing_certificates.insert(*digest));
                        ready_digests.push(*digest);
                    } else {
                        debug!(tx_digest = ?digest, missing = ?set, "certificate waiting on missing");
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

        for digest in ready_digests.iter() {
            // NOTE: failing and ignoring the certificate is fine, if it will be retried at a higher level.
            // Otherwise, this has to crash.
            let cert = match epoch_store.get_pending_certificate(digest) {
                Ok(Some(cert)) => cert,
                Ok(None) => {
                    error!(tx_digest = ?digest,
                        "Ready certificate not found in the pending table",
                    );
                    continue;
                }
                Err(e) => {
                    error!(tx_digest = ?digest,
                        "Failed to read pending table: {e}",
                    );

                    continue;
                }
            };
            self.certificate_ready(cert);
        }
    }

    /// Notifies TransactionManager about a certificate that has been executed.
    pub(crate) fn certificate_executed(
        &self,
        digest: &TransactionDigest,
        epoch_store: &AuthorityPerEpochStore,
    ) {
        {
            let inner = &mut self.inner.lock();
            inner.executing_certificates.remove(digest);
            self.metrics
                .transaction_manager_num_executing_certificates
                .set(inner.executing_certificates.len() as i64);
        }
        let _ = epoch_store.remove_pending_certificate(digest);
    }

    /// Sends the ready certificate for execution.
    fn certificate_ready(&self, certificate: VerifiedCertificate) {
        self.metrics.transaction_manager_num_ready.inc();
        let _ = self.tx_ready_certificates.send(certificate);
    }
}
