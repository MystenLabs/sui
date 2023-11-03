// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use config::{AuthorityIdentifier, Committee, WorkerCache};
use fastcrypto::traits::VerifyingKey;
use mysten_metrics::metered_channel::Sender;
use network::{client::NetworkClient, PrimaryToWorkerClient};
use storage::PayloadStore;
use sui_protocol_config::ProtocolConfig;
use tracing::{debug, info};
use types::{
    error::{DagError, DagResult},
    BatchDigest, Header, HeaderAPI, HeaderSignature, SignedHeader, WorkerSynchronizeMessage,
};

use crate::metrics::PrimaryMetrics;

pub(crate) struct Verifier {
    // The id of this primary.
    authority_id: AuthorityIdentifier,
    // Committee of the current epoch.
    committee: Committee,
    protocol_config: ProtocolConfig,
    // The worker information cache.
    worker_cache: WorkerCache,
    // Client for fetching payloads.
    client: NetworkClient,
    // The persistent store of the available batch digests produced either via our own workers
    // or others workers.
    payload_store: PayloadStore,
    // Streams verified headers to `Core`.
    tx_verified_headers: Sender<SignedHeader>,
    // Contains Synchronizer specific metrics among other Primary metrics.
    metrics: Arc<PrimaryMetrics>,
}

impl Verifier {
    pub fn new(
        authority_id: AuthorityIdentifier,
        committee: Committee,
        protocol_config: ProtocolConfig,
        worker_cache: WorkerCache,
        client: NetworkClient,
        payload_store: PayloadStore,
        tx_verified_headers: Sender<SignedHeader>,
        metrics: Arc<PrimaryMetrics>,
    ) -> Self {
        Self {
            authority_id,
            committee,
            protocol_config,
            worker_cache,
            client,
            payload_store,
            tx_verified_headers,
            metrics,
        }
    }

    // TODO(narwhalceti): move sending header to tx_verified_headers out of the function.
    pub async fn verify(&self, signed_header: &SignedHeader) -> DagResult<()> {
        // Run basic header validations.
        signed_header
            .header()
            .validate(&self.committee, &self.worker_cache)?;

        // Verify header signature.
        let Some(authority) = self.committee.authority(&signed_header.header().author()) else {
            return Err(DagError::UnknownAuthority(format!(
                "Unknown author {}",
                signed_header.header().author()
            )));
        };
        let peer_pubkey = authority.network_key();
        let signature = HeaderSignature::try_from(signed_header.signature()).map_err(|e| {
            info!(
                "Failed to parse header signature {}: {e}",
                signed_header.header()
            );
            DagError::InvalidSignature
        })?;
        peer_pubkey
            .verify(signed_header.header().digest().as_ref(), &signature)
            .map_err(|e| {
                info!(
                    "Failed to verify header signature {}: {e}",
                    signed_header.header()
                );
                DagError::InvalidSignature
            })?;

        // Verify existence and validity of batches.
        self.sync_batches_internal(signed_header.header()).await?;

        Ok(())
    }

    async fn sync_batches_internal(&self, header: &Header) -> DagResult<()> {
        if header.author() == self.authority_id {
            debug!("skipping sync_batches for header {header}: no need to sync payload from own workers");
            return Ok(());
        }

        let mut batches = HashMap::<u32, Vec<BatchDigest>>::new();
        for (digest, (worker_id, _)) in header.payload().iter() {
            batches.entry(*worker_id).or_default().push(*digest);
        }

        // Build Synchronize requests to workers.
        let protocol_key = self
            .committee
            .authority(&self.authority_id)
            .unwrap()
            .protocol_key();
        let mut fut = Vec::new();
        for (worker_id, digests) in batches {
            let worker_name = self
                .worker_cache
                .worker(protocol_key, &worker_id)
                .expect("Author of valid header is not in the worker cache")
                .name;
            let digests = digests.clone();
            let message = WorkerSynchronizeMessage {
                digests: digests.clone(),
                target: header.author(),
                is_certified: false,
            };
            let client = self.client.clone();
            let worker_name = worker_name.clone();
            fut.push(async move {
                client
                    .synchronize(worker_name, message)
                    .await
                    .map_err(|_| DagError::ShuttingDown)?;
                for digest in &digests {
                    self.payload_store
                        .write(digest, &worker_id)
                        .map_err(DagError::StoreError)?
                }
                Ok::<(), DagError>(())
            });
        }

        // Wait until results are back.
        futures::future::try_join_all(fut)
            .await
            .map(|_| ())
            .map_err(|e| DagError::NetworkError(format!("error synchronizing batches: {e:?}")))
    }
}
