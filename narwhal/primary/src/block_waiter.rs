// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::block_synchronizer::handler::Handler;
use anyhow::Result;
use config::{AuthorityIdentifier, Committee, WorkerCache};
use fastcrypto::hash::Hash;
use futures::{
    stream::{FuturesOrdered, StreamExt as _},
    FutureExt,
};
use network::WorkerRpc;
use std::{collections::HashSet, sync::Arc};

use tracing::{debug, instrument};
use types::{
    BatchMessage, BlockError, BlockErrorKind, BlockResult, Certificate, CertificateAPI,
    CertificateDigest, HeaderAPI,
};

#[cfg(test)]
#[path = "tests/block_waiter_tests.rs"]
pub mod block_waiter_tests;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GetBlockResponse {
    pub digest: CertificateDigest,
    #[allow(dead_code)]
    pub batches: Vec<BatchMessage>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GetBlocksResponse {
    pub blocks: Vec<BlockResult<GetBlockResponse>>,
}

/// BlockWaiter is responsible for fetching the block data from the
/// downstream worker nodes. A block is basically the aggregate
/// of batches of transactions for a given certificate.
pub struct BlockWaiter<SynchronizerHandler: Handler + Send + Sync + 'static> {
    /// The id of this primary.
    authority_id: AuthorityIdentifier,
    /// The network's committee
    committee: Committee,

    /// The worker information cache.
    worker_cache: WorkerCache,

    /// Network driver allowing to send messages.
    worker_network: anemo::Network,

    /// We use the handler of the block synchronizer to interact with the
    /// block synchronizer in a synchronous way. Share a reference of this
    /// between components.
    block_synchronizer_handler: Arc<SynchronizerHandler>,
}

impl<SynchronizerHandler: Handler + Send + Sync + 'static> BlockWaiter<SynchronizerHandler> {
    #[must_use]
    pub fn new(
        authority_id: AuthorityIdentifier,
        committee: Committee,
        worker_cache: WorkerCache,
        worker_network: anemo::Network,
        block_synchronizer_handler: Arc<SynchronizerHandler>,
    ) -> BlockWaiter<SynchronizerHandler> {
        Self {
            authority_id,
            committee,
            worker_cache,
            worker_network,
            block_synchronizer_handler,
        }
    }

    /// Retrieves the requested blocks, fetching certificates from other primaries and batches
    /// from our workers as needed. The response includes an individual result for each block
    /// requested, which may contain block data or an error.
    #[instrument(level = "debug", skip_all, fields(digests = ?digests), err)]
    pub async fn get_blocks(&self, digests: Vec<CertificateDigest>) -> Result<GetBlocksResponse> {
        let certificates = self.get_certificates(digests.clone()).await;

        let found_certificates: Vec<Certificate> =
            certificates.iter().flat_map(|(_, c)| c).cloned().collect();
        let sync_result = self
            .block_synchronizer_handler
            .synchronize_block_payloads(found_certificates)
            .await;
        let successful_payload_sync_set = sync_result
            .iter()
            .flat_map(|r| r.as_ref().map(|c| c.digest()).ok())
            .collect::<HashSet<CertificateDigest>>();

        let block_futures: FuturesOrdered<_> = certificates
            .into_iter()
            .map(|(digest, cert)| {
                self.get_block(digest, cert, successful_payload_sync_set.contains(&digest))
            })
            .collect();

        Ok(GetBlocksResponse {
            blocks: block_futures.collect().await,
        })
    }

    #[instrument(level = "debug", skip_all, fields(certificate_digest = ?certificate_digest), err)]
    async fn get_block(
        &self,
        certificate_digest: CertificateDigest,
        // Immediately reports an error for this block if the certificate is not available.
        certificate: Option<Certificate>,
        // Immediately reports an error for this block if payloads could not be synced.
        synced_payloads: bool,
    ) -> BlockResult<GetBlockResponse> {
        if certificate.is_none() {
            return Err(BlockError {
                digest: certificate_digest,
                error: BlockErrorKind::BlockNotFound,
            });
        }
        if !synced_payloads {
            return Err(BlockError {
                digest: certificate_digest,
                error: BlockErrorKind::BatchError,
            });
        }

        // Send batch requests to workers.
        let certificate = certificate.unwrap();
        let batch_requests: Vec<_> = certificate
            .header()
            .payload()
            .iter()
            .map(|(batch_digest, (worker_id, _))| {
                debug!("Sending batch {batch_digest} request to worker id {worker_id}");
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
                self.worker_network
                    .request_batch(worker_name, *batch_digest)
                    .map(|result| match result {
                        Ok(Some(batch)) => Ok(BatchMessage {
                            digest: *batch_digest,
                            batch,
                        }),
                        Ok(None) | Err(_) => Err(BlockError {
                            digest: certificate_digest,
                            error: BlockErrorKind::BatchError,
                        }),
                    })
            })
            .collect();

        // Return a successful result only if all workers can find the requested batches.
        let mut batches = futures::future::try_join_all(batch_requests).await?;

        // Sort batches by digest to make the response deterministic.
        batches.sort_by(|a, b| a.digest.cmp(&b.digest));
        Ok(GetBlockResponse {
            digest: certificate_digest,
            batches,
        })
    }

    /// Will fetch the certificates via the block_synchronizer. If the
    /// certificate is missing then we expect the synchronizer to
    /// fetch it via the peers. Otherwise if available on the storage
    /// should return the result immediately. The method is blocking to
    /// retrieve all the results.
    #[instrument(level = "trace", skip_all, fields(num_certificate_digests = digests.len()))]
    async fn get_certificates(
        &self,
        digests: Vec<CertificateDigest>,
    ) -> Vec<(CertificateDigest, Option<Certificate>)> {
        let mut results = Vec::new();

        let block_header_results = self
            .block_synchronizer_handler
            .get_and_synchronize_block_headers(digests)
            .await;

        for result in block_header_results {
            if let Ok(certificate) = result {
                results.push((certificate.digest(), Some(certificate)));
            } else {
                results.push((result.err().unwrap().digest(), None));
            }
        }

        results
    }
}
