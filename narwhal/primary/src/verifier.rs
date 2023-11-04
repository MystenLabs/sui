// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use config::{AuthorityIdentifier, Committee, WorkerCache};
use fastcrypto::traits::VerifyingKey;
use mysten_metrics::metered_channel::Sender;
use network::{client::NetworkClient, PrimaryToWorkerClient, RetryConfig};
use storage::PayloadStore;
use sui_protocol_config::ProtocolConfig;
use tracing::{debug, info};
use types::{
    error::{DagError, DagResult},
    Header, HeaderAPI, HeaderSignature, SignedHeader, WorkerSynchronizeMessage,
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

    pub async fn verify(&self, signed_header: SignedHeader) -> DagResult<()> {
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

        self.sync_batches_internal(signed_header.header()).await?;

        self.tx_verified_headers
            .send(signed_header)
            .await
            .map_err(|_| DagError::ShuttingDown)?;

        Ok(())
    }

    async fn sync_batches_internal(self: &Self, header: &Header) -> DagResult<()> {
        if header.author() == self.authority_id {
            debug!("skipping sync_batches for header {header}: no need to sync payload from own workers");
            return Ok(());
        }

        let mut missing = HashMap::new();
        for (digest, (worker_id, _)) in header.payload().iter() {
            // Check whether we have the batch. If one of our worker has the batch, the primary stores the pair
            // (digest, worker_id) in its own storage. It is important to verify that we received the batch
            // from the correct worker id to prevent the following attack:
            //      1. A Bad node sends a batch X to 2f good nodes through their worker #0.
            //      2. The bad node proposes a malformed block containing the batch X and claiming it comes
            //         from worker #1.
            //      3. The 2f good nodes do not need to sync and thus don't notice that the header is malformed.
            //         The bad node together with the 2f good nodes thus certify a block containing the batch X.
            //      4. The last good node will never be able to sync as it will keep sending its sync requests
            //         to workers #1 (rather than workers #0). Also, clients will never be able to retrieve batch
            //         X as they will be querying worker #1.
            if !self.payload_store.contains(*digest, *worker_id)? {
                missing
                    .entry(*worker_id)
                    .or_insert_with(Vec::new)
                    .push(*digest);
            }
        }

        // Build Synchronize requests to workers.
        let protocol_key = self
            .committee
            .authority(&self.authority_id)
            .unwrap()
            .protocol_key();
        let mut synchronize_handles = Vec::new();
        for (worker_id, digests) in missing {
            let worker_name = self
                .worker_cache
                .worker(protocol_key, &worker_id)
                .expect("Author of valid header is not in the worker cache")
                .name;
            let retry_config = RetryConfig::default(); // 30s timeout
            let handle = retry_config.retry(move || {
                let digests = digests.clone();
                let message = WorkerSynchronizeMessage {
                    digests: digests.clone(),
                    target: header.author(),
                    is_certified: false,
                };
                let client = self.client.clone();
                let worker_name = worker_name.clone();
                async move {
                    let result = client.synchronize(worker_name, message).await.map_err(|e| {
                        backoff::Error::transient(DagError::NetworkError(format!("{e:?}")))
                    });
                    if result.is_ok() {
                        for digest in &digests {
                            self.payload_store
                                .write(digest, &worker_id)
                                .map_err(|e| backoff::Error::permanent(DagError::StoreError(e)))?
                        }
                    }
                    result
                }
            });
            synchronize_handles.push(handle);
        }

        // Wait until results are back.
        futures::future::try_join_all(synchronize_handles)
            .await
            .map(|_| ())
            .map_err(|e| DagError::NetworkError(format!("error synchronizing batches: {e:?}")))
    }
}
