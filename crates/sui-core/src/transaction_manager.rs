// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, RwLock},
};

use futures::{stream::FuturesUnordered, StreamExt};
use sui_types::{base_types::TransactionDigest, error::SuiResult, messages::VerifiedCertificate};
use tokio::sync::{mpsc::UnboundedSender, Mutex};
use tokio::task::JoinHandle;
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
    enqueue_tasks: Mutex<FuturesUnordered<JoinHandle<()>>>,
}

#[derive(Default)]
struct Inner {
    missing_inputs: BTreeMap<ObjectKey, TransactionDigest>,
    pending_certificates: BTreeMap<TransactionDigest, BTreeSet<ObjectKey>>,
}

static MAX_PENDING_ENQUEUE_TASKS: usize = 64;

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
            tx_ready_certificates,
            inner: Default::default(),
            enqueue_tasks: Default::default(),
        };
        transaction_manager
            .enqueue_impl(
                transaction_manager
                    .authority_store
                    .epoch_store()
                    .all_pending_certificates()
                    .unwrap(),
            )
            .await;
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
    pub(crate) async fn enqueue(self: Arc<Self>, certs: Vec<VerifiedCertificate>) -> SuiResult<()> {
        let this = self.clone();
        let enqueue_tasks = &mut this.enqueue_tasks.lock().await;
        while enqueue_tasks.len() >= MAX_PENDING_ENQUEUE_TASKS {
            enqueue_tasks.next().await;
        }
        enqueue_tasks.push(tokio::spawn(async move { self.enqueue_impl(certs).await }));
        Ok(())
    }

    async fn enqueue_impl(&self, certs: Vec<VerifiedCertificate>) {
        // Skip processing of any certificates that are already enqueued.
        let certs: Vec<_> = {
            let inner = self.inner.read().unwrap();
            certs
                .into_iter()
                .filter(|cert| !inner.pending_certificates.contains_key(cert.digest()))
                .collect()
        };

        let mut missing_inputs = Vec::new();
        for cert in certs {
            let digest = *cert.digest();
            // hold the tx lock until we have finished checking if objects are missing, so that we
            // don't race with a concurrent execution of this tx.
            let epoch_store = self.authority_store.epoch_store();
            let _tx_lock = epoch_store.acquire_tx_lock(&digest);

            // skip txes that are executed already
            if self
                .authority_store
                .effects_exists(&digest)
                .expect("db op cannot fail")
            {
                continue;
            }
            let missing = self
                .authority_store
                .get_missing_input_objects(
                    &digest,
                    &cert.data().data.input_objects().expect("db op cannot fail"),
                )
                .await
                .expect("Are shared object locks set prior to enqueueing certificates?");

            if missing.is_empty() {
                debug!(tx_digest = ?digest, "certificate ready");
                self.certificate_ready(cert);
                continue;
            } else {
                debug!(tx_digest = ?digest, ?missing, "certificate waiting on missing objects");
            }

            for obj_key in missing {
                missing_inputs.push((obj_key, digest));
            }
        }

        let inner = &mut self.inner.write().unwrap();
        for (obj_key, digest) in missing_inputs.iter() {
            // A missing input object in TransactionManager will definitely be notified via
            // objects_committed(), when the object actually gets committed, because:
            // 1. Assume rocksdb is strongly consistent, writing the object to the objects
            // table must happen after not finding the object in get_missing_input_objects().
            // 2. Notification via objects_committed() will happen after an object is written
            // into the objects table.
            // 3. TransactionManager is protected by a mutex. The notification via
            // objects_committed() can only arrive after the current enqueue() call finishes.
            // TODO: verify the key does not already exist.
            inner.missing_inputs.insert(*obj_key, *digest);
            inner
                .pending_certificates
                .entry(*digest)
                .or_default()
                .insert(*obj_key);
        }

        self.metrics
            .transaction_manager_num_missing_objects
            .set(inner.missing_inputs.len() as i64);
        self.metrics
            .transaction_manager_num_pending_certificates
            .set(inner.pending_certificates.len() as i64);
    }

    /// Notifies TransactionManager that the given objects have been committed.
    pub(crate) fn objects_committed(&self, object_keys: Vec<ObjectKey>) {
        let mut ready_digests = Vec::new();

        {
            let inner = &mut self.inner.write().unwrap();
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

    /// Marks the given certificate as ready to be executed.
    fn certificate_ready(&self, certificate: VerifiedCertificate) {
        self.metrics.transaction_manager_num_ready.inc();
        let _ = self.tx_ready_certificates.send(certificate);
    }
}
