// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use sui_types::{
    base_types::TransactionDigest, committee::EpochId, error::SuiResult,
    messages::VerifiedCertificate,
};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, warn};

use crate::authority::{authority_store::ObjectKey, AuthorityMetrics, AuthorityStore};

/// TransactionManager has the following responsibilities:
/// - Ensure only a single certificate is in one of pending / executing and executed state.
/// Notably, it avoids re-execution of certificates.
/// - Receive pending certificates from AuthorityState, accept notifications about
/// committed objects, discover pending certificates ready to run, and publish ready certificates
/// to the execution driver.
///
/// If a source has a stream of certificates to be executed, it should follow the pattern:
/// 1. Call enqueue_to_execute() to record and lock the execution of a certificate.
/// 2. Acknowledge that the certificate will be executed to the source (e.g. Narwhal, checkpoint).
///
/// TODO: use TransactionManager for fullnode / handle_certificate_with_effects().
pub(crate) struct TransactionManager {
    authority_store: Arc<AuthorityStore>,
    missing_inputs: BTreeMap<ObjectKey, (EpochId, TransactionDigest)>,
    pending_certificates: BTreeMap<(EpochId, TransactionDigest), BTreeSet<ObjectKey>>,
    executing_certificates: BTreeMap<(EpochId, TransactionDigest), VerifiedCertificate>,
    tx_ready_certificates: UnboundedSender<VerifiedCertificate>,
    metrics: Arc<AuthorityMetrics>,
}

impl TransactionManager {
    /// If a node restarts, transaction manager recovers data from the pending_certificates table.
    pub(crate) async fn new(
        authority_store: Arc<AuthorityStore>,
        tx_ready_certificates: UnboundedSender<VerifiedCertificate>,
        metrics: Arc<AuthorityMetrics>,
    ) -> TransactionManager {
        let mut transaction_manager = TransactionManager {
            authority_store,
            metrics,
            missing_inputs: BTreeMap::new(),
            pending_certificates: BTreeMap::new(),
            executing_certificates: BTreeMap::new(),
            tx_ready_certificates,
        };
        transaction_manager
            .add(
                transaction_manager
                    .authority_store
                    .all_pending_certificates()
                    .unwrap(),
            )
            .expect("Initialize TransactionManager with pending certificates failed.");
        transaction_manager
    }

    /// Enqueues certificates into TransactionManager. Once all of the input objects are available
    /// locally for a certificate, the certified transaction will be sent to execution driver.
    ///
    /// This is  a no-op for certificates that are executing or have finished execution.
    ///
    /// Takes shared object locks if needed, and persists the pending certificate for crash
    /// recovery. Once this function returns success, the certificate will be guaranteed to
    /// execute to completion.
    pub(crate) async fn enqueue_to_execute(
        &mut self,
        certs: Vec<VerifiedCertificate>,
    ) -> SuiResult<()> {
        for cert in &certs {
            // Skip processing if the certificate is already enqueued.
            if self
                .pending_certificates
                .contains_key(&(cert.epoch(), *cert.digest()))
            {
                continue;
            }
            // Skip processing if the certificate is executing.
            // Checking this before checking the effects table is correct, because a certificate
            // is added to the executing_certificates table  before it is sent to execution driver,
            // and removed after the transaction finished execution and effects are written.
            if self
                .executing_certificates
                .contains_key(&(cert.epoch(), *cert.digest()))
            {
                continue;
            }
            // A certificate has been executed if it is available in the effects table.
            if self.authority_store.effects_exists(cert.digest())? {
                continue;
            }

            // Commit the necessary updates to the authority store.
            if cert.contains_shared_object() {
                self.authority_store
                    .record_pending_shared_object_certificate(cert)
                    .await?;
            } else {
                self.authority_store
                    .record_pending_owned_object_certificate(cert)
                    .await?;
            }
        }
        self.add(certs)?;
        Ok(())
    }

    /// Adds the pending certificates into TransactionManager, assuming the necessary persistent
    /// data have been written into pending certificates, and shared locks tables if needed.
    fn add(&mut self, certs: Vec<VerifiedCertificate>) -> SuiResult<()> {
        for cert in certs {
            // Skip processing if the certificate is already enqueued.
            if self
                .pending_certificates
                .contains_key(&(cert.epoch(), *cert.digest()))
            {
                continue;
            }
            let missing = self
                .authority_store
                .get_missing_input_objects(cert.digest(), &cert.data().data.input_objects()?)
                .expect("Are shared object locks set prior to enqueueing certificates?");
            if missing.is_empty() {
                self.certificate_ready(cert);
                continue;
            }
            let cert_key = (cert.epoch(), *cert.digest());
            for obj_key in missing {
                // A missing input object in TransactionManager will definitely be notified via
                // commit() of the certificate that outputs the object, because:
                // 1. Assume rocksdb is strongly consistent, writing the object to the objects
                // table must happen after not finding the object in get_missing_input_objects().
                // 2. Notification via commit() will happen after an object is written into the
                // objects table.
                // 3. TransactionManager is protected by a mutex. The notification via commit()
                // can only arrive after the current enqueue() call finishes.
                assert!(self.missing_inputs.insert(obj_key, cert_key).is_none());
                self.pending_certificates
                    .entry(cert_key)
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

    /// Notifies TransactionManager that the given objects have been committed and are available.
    pub(crate) fn objects_avaiable(&mut self, object_keys: Vec<ObjectKey>) {
        for object_key in object_keys {
            let cert_key = if let Some(key) = self.missing_inputs.remove(&object_key) {
                key
            } else {
                continue;
            };
            let set = self.pending_certificates.entry(cert_key).or_default();
            set.remove(&object_key);
            // This certificate has no missing input. It is ready to execute.
            if set.is_empty() {
                self.pending_certificates.remove(&cert_key);
                // NOTE: failing and ignoring the certificate is fine, if it will be retried at a higher level.
                // Otherwise, this has to crash.
                let cert = match self
                    .authority_store
                    .get_pending_certificate(cert_key.0, &cert_key.1)
                {
                    Ok(Some(cert)) => cert,
                    Ok(None) => {
                        error!(tx_digest = ?cert_key,
                            "Ready certificate not found in the pending table",
                        );
                        continue;
                    }
                    Err(e) => {
                        error!(tx_digest = ?cert_key,
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

    /// Notifies TransactionManager that the given transaction and its output objects have been
    /// committed. All certificates must call this function after execution.
    pub(crate) fn commit(&mut self, epoch: EpochId, digest: &TransactionDigest) -> SuiResult<()> {
        // Remove the pending certificate and associated shared object locks from storage.
        let certificate = self
            .executing_certificates
            .remove(&(epoch, *digest))
            .unwrap();
        self.authority_store
            .commit_pending_certificate(&certificate)
    }

    /// Marks the given certificate as ready to be executed.
    fn certificate_ready(&mut self, certificate: VerifiedCertificate) {
        self.metrics.transaction_manager_num_ready.inc();
        assert!(self
            .executing_certificates
            .insert(
                (certificate.epoch(), *certificate.digest()),
                certificate.clone()
            )
            .is_none());
        if let Err(e) = self.tx_ready_certificates.send(certificate) {
            warn!("Execution driver has shut down. {e}");
        }
    }
}
