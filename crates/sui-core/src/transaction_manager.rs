// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use sui_storage::node_sync_store::NodeSyncStore;
use sui_types::{
    base_types::TransactionDigest, committee::EpochId, error::SuiResult,
    messages::VerifiedCertificate,
};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, warn};
use typed_store::Map;

use crate::authority::{authority_store::ObjectKey, AuthorityMetrics, AuthorityStore};

/// TransactionManager is responsible for managing pending certificates and publishes a stream
/// of certificates ready to be executed. It works together with AuthorityState for receiving
/// pending certificates, and getting notified about committed objects. Executing driver
/// subscribes to the stream of ready certificates published by the TransactionManager, and can
/// execute them in parallel.
/// TODO: use TransactionManager for fullnode.
pub(crate) struct TransactionManager {
    authority_store: Arc<AuthorityStore>,
    missing_inputs: BTreeMap<ObjectKey, TransactionDigest>,
    pending_certificates: BTreeMap<TransactionDigest, BTreeSet<ObjectKey>>,
    tx_ready_certificates: UnboundedSender<VerifiedCertificate>,
    metrics: Arc<AuthorityMetrics>,
}

impl TransactionManager {
    /// If a node restarts, transaction manager recovers in-memory data from pending certificates and
    /// other persistent data.
    pub(crate) async fn new(
        authority_store: Arc<AuthorityStore>,
        node_sync_store: Arc<NodeSyncStore>,
        epoch: EpochId,
        tx_ready_certificates: UnboundedSender<VerifiedCertificate>,
        metrics: Arc<AuthorityMetrics>,
    ) -> TransactionManager {
        let mut transaction_manager = TransactionManager {
            authority_store,
            metrics,
            missing_inputs: BTreeMap::new(),
            pending_certificates: BTreeMap::new(),
            tx_ready_certificates,
        };

        // Temporary change to recover pending certificates from 0.15.1, then clearing those data.
        let mut pending_certs = Vec::new();
        for (_, (_, digest)) in transaction_manager
            .authority_store
            .epoch_tables()
            .pending_execution
            .iter()
        {
            if let Ok(Some(cert)) = node_sync_store.get_cert(epoch, &digest) {
                pending_certs.push(cert);
            } else {
                match transaction_manager.authority_store.effects_exists(&digest) {
                    Ok(true) => {}
                    Ok(false) => {
                        warn!(tx_digest=?digest, "Pending certificate is not found in node_sync_store!")
                    }
                    Err(e) => {
                        warn!(tx_digest=?digest, "Pending certificate cannot be recovered from node_sync_store! {e}")
                    }
                }
            }
        }
        transaction_manager
            .authority_store
            .store_pending_certificates(&pending_certs)
            .unwrap();
        transaction_manager
            .authority_store
            .epoch_tables()
            .pending_execution
            .clear()
            .unwrap();

        transaction_manager
            .enqueue(
                transaction_manager
                    .authority_store
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
    pub(crate) async fn enqueue(&mut self, certs: Vec<VerifiedCertificate>) -> SuiResult<()> {
        for cert in certs {
            let digest = *cert.digest();
            // hold the tx lock until we have finished checking if objects are missing, so that we
            // don't race with a concurrent execution of this tx.
            let _tx_lock = self.authority_store.acquire_tx_lock(&digest);

            // Skip processing if the certificate is already enqueued.
            if self.pending_certificates.contains_key(&digest) {
                continue;
            }
            // skip txes that are executed already
            if self.authority_store.effects_exists(&digest)? {
                continue;
            }
            let missing = self
                .authority_store
                .get_missing_input_objects(cert.digest(), &cert.signed_data.data.input_objects()?)
                .expect("Are shared object locks set prior to enqueueing certificates?");
            if missing.is_empty() {
                self.certificate_ready(cert);
                continue;
            }

            for obj_key in missing {
                // TODO: verify the key does not already exist.
                self.missing_inputs.insert(obj_key, digest);
                self.pending_certificates
                    .entry(digest)
                    .or_default()
                    .insert(obj_key);
            }
        }
        self.metrics
            .transaction_manager_num_missing_objects
            .set(self.missing_inputs.len() as i64);
        self.metrics
            .transaction_manager_num_pending_certificates
            .set(self.pending_certificates.len() as i64);
        Ok(())
    }

    /// Notifies TransactionManager that the given objects have been committed.
    // TODO: investigate switching TransactionManager to use notify_read() on objects table.
    pub(crate) fn objects_committed(&mut self, object_keys: Vec<ObjectKey>) {
        for object_key in object_keys {
            let Some(digest) =  self.missing_inputs.remove(&object_key) else {
                continue;
            };
            let set = self.pending_certificates.entry(digest).or_default();
            set.remove(&object_key);
            // This certificate has no missing input. It is ready to execute.
            if set.is_empty() {
                self.pending_certificates.remove(&digest);
                // NOTE: failing and ignoring the certificate is fine, if it will be retried at a higher level.
                // Otherwise, this has to crash.
                let cert = match self.authority_store.get_pending_certificate(&digest) {
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
        self.metrics
            .transaction_manager_num_missing_objects
            .set(self.missing_inputs.len() as i64);
        self.metrics
            .transaction_manager_num_pending_certificates
            .set(self.pending_certificates.len() as i64);
    }

    /// Run a periodic scanning task that checks if any input object is in fact already committed.
    /// This can discover more ready transactions.
    /// TODO: rely on notify_read() for objects or the mutex on TransactionManager instead, and
    /// remove this periodic scanner.
    pub(crate) fn scan_ready_transactions(&mut self) {
        let mut available_inputs = Vec::new();
        for (object_key, _) in self.missing_inputs.iter() {
            match self
                .authority_store
                .object_exists(&object_key.0, object_key.1)
            {
                Ok(true) => available_inputs.push(*object_key),
                Err(e) => warn!("Failed to check if object exists: {e}"),
                _ => {}
            }
        }
        if available_inputs.is_empty() {
            return;
        }
        self.metrics
            .transaction_manager_objects_notified_via_scan
            .add(available_inputs.len() as i64);
        self.objects_committed(available_inputs);
    }

    /// Marks the given certificate as ready to be executed.
    fn certificate_ready(&self, certificate: VerifiedCertificate) {
        self.metrics.transaction_manager_num_ready.inc();
        let _ = self.tx_ready_certificates.send(certificate);
    }
}
