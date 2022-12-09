// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use sui_types::{base_types::TransactionDigest, error::SuiResult, messages::VerifiedCertificate};
use tokio::sync::{mpsc::UnboundedSender, RwLock};
use tracing::{debug, error};

use crate::authority::{authority_store::ObjectKey, AuthorityMetrics, AuthorityStore};

/// TransactionManager is responsible for managing pending certificates and publishes a stream
/// of certificates ready to be executed. It works together with AuthorityState for receiving
/// pending certificates, and getting notified about committed objects. Executing driver
/// subscribes to the stream of ready certificates published by the TransactionManager, and can
/// execute them in parallel.
/// TODO: use TransactionManager for fullnode.
pub(crate) struct TransactionManager {
    authority_store: Arc<AuthorityStore>,
    tx_ready_certificates: UnboundedSender<VerifiedCertificate>,
    metrics: Arc<AuthorityMetrics>,
    inner: RwLock<Inner>,
}

#[derive(Default)]
struct Inner {
    // Maps missing input objects to transactions in pending_certificates.
    missing_inputs: BTreeMap<ObjectKey, TransactionDigest>,

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
    pub(crate) async fn new(
        authority_store: Arc<AuthorityStore>,
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
            .enqueue(
                transaction_manager
                    .authority_store
                    .epoch_store()
                    .all_pending_certificates()
                    .unwrap(),
            )
            .await
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
    pub(crate) async fn enqueue(&self, certs: Vec<VerifiedCertificate>) -> SuiResult<()> {
        let inner = &mut self.inner.write().await;
        for cert in certs {
            let digest = *cert.digest();
            // hold the tx lock until we have finished checking if objects are missing, so that we
            // don't race with a concurrent execution of this tx.
            let epoch_store = self.authority_store.epoch_store();
            let _tx_lock = epoch_store.acquire_tx_lock(&digest);

            // skip already pending txes
            if inner.pending_certificates.contains_key(&digest) {
                continue;
            }
            // skip already executing txes
            if inner.executing_certificates.contains(&digest) {
                continue;
            }
            // skip already executed txes
            if self.authority_store.effects_exists(&digest)? {
                // also ensure the transaction will not be retried after restart.
                let _ = self
                    .authority_store
                    .epoch_store()
                    .remove_pending_certificate(&digest);
                continue;
            }

            let missing = self
                .authority_store
                .get_missing_input_objects(
                    &digest,
                    &cert.data().intent_message.value.input_objects()?,
                )
                .await
                .expect("Are shared object locks set prior to enqueueing certificates?");

            if missing.is_empty() {
                debug!(tx_digest = ?digest, "certificate ready");
                assert!(inner.executing_certificates.insert(digest));
                self.certificate_ready(cert);
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
            inner
                .missing_inputs
                .extend(missing.clone().into_iter().map(|obj_key| (obj_key, digest)));
            inner
                .pending_certificates
                .entry(digest)
                .or_default()
                .extend(missing);
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
    pub(crate) async fn objects_committed(&self, object_keys: Vec<ObjectKey>) {
        let mut ready_digests = Vec::new();

        {
            let inner = &mut self.inner.write().await;
            for object_key in object_keys {
                let Some(digest) = inner.missing_inputs.remove(&object_key) else {
                    continue;
                };
                let set = inner.pending_certificates.entry(digest).or_default();
                set.remove(&object_key);
                // This certificate has no missing input. It is ready to execute.
                if set.is_empty() {
                    debug!(tx_digest = ?digest, "certificate ready");
                    inner.pending_certificates.remove(&digest);
                    assert!(inner.executing_certificates.insert(digest));
                    ready_digests.push(digest);
                } else {
                    debug!(tx_digest = ?digest, missing = ?set, "certificate waiting on missing");
                }
            }

            self.metrics
                .transaction_manager_num_missing_objects
                .set(inner.missing_inputs.len() as i64);
            self.metrics
                .transaction_manager_num_pending_certificates
                .set(inner.pending_certificates.len() as i64);
        }

        for digest in ready_digests.iter() {
            // NOTE: failing and ignoring the certificate is fine, if it will be retried at a higher level.
            // Otherwise, this has to crash.
            let cert = match self
                .authority_store
                .epoch_store()
                .get_pending_certificate(digest)
            {
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
    pub(crate) async fn certificate_executed(&self, digest: &TransactionDigest) {
        {
            let inner = &mut self.inner.write().await;
            inner.executing_certificates.remove(digest);
        }
        let _ = self
            .authority_store
            .epoch_store()
            .remove_pending_certificate(digest);
    }

    /// Sends the ready certificate for execution.
    fn certificate_ready(&self, certificate: VerifiedCertificate) {
        self.metrics.transaction_manager_num_ready.inc();
        let _ = self.tx_ready_certificates.send(certificate);
    }
}
