// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::utils;
use anyhow::Result;
use config::{AuthorityIdentifier, Committee, WorkerCache, WorkerId};
use consensus::dag::{Dag, ValidatorDagError};
use fastcrypto::hash::Hash;
use futures::future::try_join_all;
use itertools::Either;
use network::PrimaryToWorkerRpc;
use std::{collections::HashMap, sync::Arc};
use storage::{CertificateStore, PayloadToken};
use store::{rocks::TypedStoreError, Store};
use tokio::sync::mpsc;

use tracing::{debug, instrument, warn};
use types::{BatchDigest, Certificate, CertificateDigest, Header, HeaderDigest, Round};

#[cfg(test)]
#[path = "tests/block_remover_tests.rs"]
pub mod block_remover_tests;

/// BlockRemover is responsible for removing blocks identified by
/// their certificate digest from across our system. On high level
/// It will make sure that the DAG is updated, internal storage where
/// there certificates and headers are stored, and the corresponding
/// batches as well.
pub struct BlockRemover {
    /// The id of this primary.
    authority_id: AuthorityIdentifier,
    /// The network's committee
    committee: Committee,

    /// The worker information cache.
    worker_cache: WorkerCache,

    /// Storage that keeps the Certificates by their digest id.
    certificate_store: CertificateStore,

    /// Storage that keeps the headers by their digest id
    header_store: HeaderStore,

    /// The persistent storage for payload markers from workers.
    payload_store: PayloadStore,

    /// The Dag structure for managing the stored certificates
    dag: Option<Arc<Dag>>,

    /// Network driver allowing to send messages.
    worker_network: anemo::Network,

    /// Outputs all the successfully deleted certificates
    tx_committed_certificates: mpsc::Sender<(Round, Vec<Certificate>)>,
}

impl BlockRemover {
    #[must_use]
    pub fn new(
        authority_id: AuthorityIdentifier,
        committee: Committee,
        worker_cache: WorkerCache,
        certificate_store: CertificateStore,
        header_store: HeaderStore,
        payload_store: PayloadStore,
        dag: Option<Arc<Dag>>,
        worker_network: anemo::Network,
        tx_committed_certificates: mpsc::Sender<(Round, Vec<Certificate>)>,
    ) -> BlockRemover {
        Self {
            authority_id,
            committee,
            worker_cache,
            certificate_store,
            header_store,
            payload_store,
            dag,
            worker_network,
            tx_committed_certificates,
        }
    }

    /// Deletes all batches from worker storage that are part of the given certificates.
    /// Returns an error unless *all* batches were successfully deleted.
    #[instrument(level = "debug", skip_all, fields(digests = ?digests), err)]
    pub async fn remove_blocks(&self, digests: Vec<CertificateDigest>) -> Result<()> {
        // Look up certificates requested for removal.
        let certificates = self.certificate_store.read_all(digests.clone())?;
        let non_found_digests: Vec<CertificateDigest> = certificates
            .iter()
            .zip(digests.clone())
            .filter_map(|(c, digest)| if c.is_none() { Some(digest) } else { None })
            .collect();
        if !non_found_digests.is_empty() {
            warn!("Some certificates are missing, will ignore for removal: {non_found_digests:?}");
        }
        let found_certificates: Vec<Certificate> = certificates.into_iter().flatten().collect();

        // Send delete requests to individual workers.
        let mut worker_requests = Vec::new();
        let batches_by_worker =
            utils::map_certificate_batches_by_worker(found_certificates.as_slice());
        for (worker_id, batch_digests) in batches_by_worker.iter() {
            let worker_name = self
                .worker_cache
                .worker(
                    self.committee
                        .authority(&self.authority_id)
                        .unwrap()
                        .protocol_key(),
                    worker_id,
                )
                .expect("Worker id not found")
                .name;

            debug!(
                "Sending DeleteBatches request for batch digests {batch_digests:?} to worker {worker_name}"
            );
            worker_requests.push(
                self.worker_network
                    .delete_batches(worker_name, batch_digests.clone()),
            );
        }
        try_join_all(worker_requests).await?;

        // If batch deletion on workers succeeded, clean up related state.
        self.cleanup_internal_state(found_certificates, batches_by_worker)
            .await?;

        Ok(())
    }

    #[instrument(level = "debug", skip_all, err)]
    async fn cleanup_internal_state(
        &self,
        certificates: Vec<Certificate>,
        batches_by_worker: HashMap<WorkerId, Vec<BatchDigest>>,
    ) -> Result<(), Either<TypedStoreError, ValidatorDagError>> {
        let header_digests: Vec<HeaderDigest> =
            certificates.iter().map(|c| c.header().digest()).collect();

        self.header_store
            .remove_all(header_digests)
            .map_err(Either::Left)?;

        // delete batch from the payload store as well
        let mut batches_to_cleanup: Vec<(BatchDigest, WorkerId)> = Vec::new();
        for (worker_id, batch_digests) in batches_by_worker {
            batch_digests.into_iter().for_each(|d| {
                batches_to_cleanup.push((d, worker_id));
            })
        }
        self.payload_store
            .remove_all(batches_to_cleanup)
            .map_err(Either::Left)?;

        // NOTE: delete certificates in the end since if we need to repeat the request
        // we want to be able to find them in storage.
        let certificate_digests: Vec<CertificateDigest> =
            certificates.as_slice().iter().map(|c| c.digest()).collect();
        if let Some(dag) = &self.dag {
            dag.remove(&certificate_digests)
                .await
                .map_err(Either::Right)?
        }

        self.certificate_store
            .delete_all(certificate_digests)
            .map_err(Either::Left)?;

        // Now output all the removed certificates
        if !certificates.is_empty() {
            let all_certs = certificates.clone();
            // Unwrap safe since list is not empty.
            let highest_round = certificates
                .iter()
                .map(|c| c.header().round())
                .max()
                .unwrap();

            // We signal that these certificates must have been committed by the external consensus
            self.tx_committed_certificates
                .send((highest_round, all_certs))
                .await
                .expect("Couldn't forward removed certificates to channel");
        }

        debug!("Successfully cleaned up certificates: {:?}", certificates);

        Ok(())
    }
}
