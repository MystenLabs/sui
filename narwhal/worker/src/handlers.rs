// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anemo::{types::response::StatusCode, Network};
use anyhow::Result;
use async_trait::async_trait;
use config::{AuthorityIdentifier, Committee, WorkerCache, WorkerId};
use fastcrypto::hash::Hash;
use futures::{stream::FuturesUnordered, StreamExt};
use itertools::Itertools;
use network::{client::NetworkClient, WorkerToPrimaryClient};
use rand::seq::SliceRandom;
use std::{collections::HashSet, time::Duration};
use store::{rocks::DBMap, Map};
use tokio::time::sleep;
use tracing::{debug, trace, warn};
use types::{
    now, Batch, BatchAPI, BatchDigest, FetchBatchesRequest, FetchBatchesResponse, PrimaryToWorker,
    RequestBatchRequest, RequestBatchResponse, RequestBatchesRequest, RequestBatchesResponse,
    WorkerBatchMessage, WorkerDeleteBatchesMessage, WorkerOthersBatchMessage,
    WorkerSynchronizeMessage, WorkerToWorker, WorkerToWorkerClient,
};

use mysten_metrics::monitored_future;

use crate::{batch_fetcher::BatchFetcher, TransactionValidator};

#[cfg(test)]
#[path = "tests/handlers_tests.rs"]
pub mod handlers_tests;

/// Defines how the network receiver handles incoming workers messages.
#[derive(Clone)]
pub struct WorkerReceiverHandler<V> {
    pub id: WorkerId,
    pub client: NetworkClient,
    pub store: DBMap<BatchDigest, Batch>,
    pub validator: V,
}

#[async_trait]
impl<V: TransactionValidator> WorkerToWorker for WorkerReceiverHandler<V> {
    async fn report_batch(
        &self,
        request: anemo::Request<WorkerBatchMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let message = request.into_body();
        if let Err(err) = self.validator.validate_batch(&message.batch).await {
            // The batch is invalid, we don't want to process it.
            return Err(anemo::rpc::Status::new_with_message(
                StatusCode::BadRequest,
                format!("Invalid batch: {err}"),
            ));
        }
        let digest = message.batch.digest();
        let mut batch = message.batch.clone();
        batch.metadata_mut().created_at = now();
        self.store.insert(&digest, &batch).map_err(|e| {
            anemo::rpc::Status::internal(format!("failed to write to batch store: {e:?}"))
        })?;
        self.client
            .report_others_batch(WorkerOthersBatchMessage {
                digest,
                worker_id: self.id,
            })
            .await
            .map_err(|e| anemo::rpc::Status::internal(e.to_string()))?;
        Ok(anemo::Response::new(()))
    }

    async fn request_batch(
        &self,
        request: anemo::Request<RequestBatchRequest>,
    ) -> Result<anemo::Response<RequestBatchResponse>, anemo::rpc::Status> {
        // TODO [issue #7]: Do some accounting to prevent bad actors from monopolizing our resources
        let batch = request.into_body().batch;
        let batch = self.store.get(&batch).map_err(|e| {
            anemo::rpc::Status::internal(format!("failed to read from batch store: {e:?}"))
        })?;

        Ok(anemo::Response::new(RequestBatchResponse { batch }))
    }

    async fn request_batches(
        &self,
        request: anemo::Request<RequestBatchesRequest>,
    ) -> Result<anemo::Response<RequestBatchesResponse>, anemo::rpc::Status> {
        const MAX_REQUEST_BATCHES_RESPONSE_SIZE: usize = 6_000_000;
        const BATCH_DIGESTS_READ_CHUNK_SIZE: usize = 200;

        let digests_to_fetch = request.into_body().batch_digests;
        let digests_chunks = digests_to_fetch
            .chunks(BATCH_DIGESTS_READ_CHUNK_SIZE)
            .map(|chunk| chunk.to_vec())
            .collect_vec();
        let mut batches = Vec::new();
        let mut total_size = 0;
        let mut is_size_limit_reached = false;

        for digests_chunks in digests_chunks {
            let stored_batches = self.store.multi_get(digests_chunks).map_err(|e| {
                anemo::rpc::Status::internal(format!("failed to read from batch store: {e:?}"))
            })?;

            for stored_batch in stored_batches.into_iter().flatten() {
                let batch_size = stored_batch.size();
                if total_size + batch_size <= MAX_REQUEST_BATCHES_RESPONSE_SIZE {
                    batches.push(stored_batch);
                    total_size += batch_size;
                } else {
                    is_size_limit_reached = true;
                    break;
                }
            }
        }

        Ok(anemo::Response::new(RequestBatchesResponse {
            batches,
            is_size_limit_reached,
        }))
    }
}

/// Defines how the network receiver handles incoming primary messages.
pub struct PrimaryReceiverHandler<V> {
    // The id of this authority.
    pub authority_id: AuthorityIdentifier,
    // The id of this worker.
    pub id: WorkerId,
    // The committee information.
    pub committee: Committee,
    // The worker information cache.
    pub worker_cache: WorkerCache,
    // The batch store
    pub store: DBMap<BatchDigest, Batch>,
    // Timeout on RequestBatch RPC.
    pub request_batch_timeout: Duration,
    // Number of random nodes to query when retrying batch requests.
    pub request_batch_retry_nodes: usize,
    // Synchronize header payloads from other workers.
    pub network: Option<Network>,
    // Fetch certificate payloads from other workers.
    pub batch_fetcher: Option<BatchFetcher>,
    // Validate incoming batches
    pub validator: V,
}

#[async_trait]
impl<V: TransactionValidator> PrimaryToWorker for PrimaryReceiverHandler<V> {
    async fn synchronize(
        &self,
        request: anemo::Request<WorkerSynchronizeMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let Some(network) = self.network.as_ref() else {
            return Err(anemo::rpc::Status::new_with_message(
                StatusCode::BadRequest,
                "synchronize() is unsupported via RPC interface, please call via local worker handler instead",
            ));
        };
        let message = request.body();
        let mut missing = HashSet::new();
        for digest in message.digests.iter() {
            // Check if we already have the batch.
            match self.store.get(digest) {
                Ok(None) => {
                    missing.insert(*digest);
                    debug!("Requesting sync for batch {digest}");
                }
                Ok(Some(_)) => {
                    trace!("Digest {digest} already in store, nothing to sync");
                }
                Err(e) => {
                    return Err(anemo::rpc::Status::internal(format!(
                        "failed to read from batch store: {e:?}"
                    )));
                }
            };
        }

        // Keep attempting to retrieve missing batches until we get them all or the client
        // abandons the RPC.
        // TODO: synchronize() should only be used for header payloads, so remove the broadcast
        // logic after first attempt.
        // TODO: synchronize() should not be cancelled when the higher level RequestVote gets
        // cancelled, if payload requests are already inflight. This avoids wasting work and
        // potentially stuck when there are many batches to fetch.
        let mut first_attempt = true;
        loop {
            if missing.is_empty() {
                return Ok(anemo::Response::new(()));
            }

            // TODO: migrate to RequestBatches RPC, or use BatchFetcher.
            let batch_requests: Vec<_> = missing
                .iter()
                .cloned()
                .map(|batch| RequestBatchRequest { batch })
                .collect();

            let mut handles = FuturesUnordered::new();
            let request_batch_fn =
                |mut client: WorkerToWorkerClient<anemo::Peer>, batch_request, timeout| {
                    // Wrapper function enables us to move `client` into the future.
                    monitored_future!(async move {
                        client
                            .request_batch(anemo::Request::new(batch_request).with_timeout(timeout))
                            .await
                    })
                };
            if first_attempt {
                // Send first sync request to a single node.
                let worker_name = match self.worker_cache.worker(
                    self.committee
                        .authority(&message.target)
                        .unwrap()
                        .protocol_key(),
                    &self.id,
                ) {
                    Ok(worker_info) => worker_info.name,
                    Err(e) => {
                        return Err(anemo::rpc::Status::internal(format!(
                            "The primary asked us to sync with an unknown node: {e}"
                        )));
                    }
                };
                let peer_id = anemo::PeerId(worker_name.0.to_bytes());
                if let Some(peer) = network.peer(peer_id) {
                    debug!(
                        "Sending BatchRequests to {worker_name}: {:?}",
                        batch_requests
                    );
                    handles.extend(batch_requests.into_iter().map(|request| {
                        request_batch_fn(
                            WorkerToWorkerClient::new(peer.clone()),
                            request,
                            self.request_batch_timeout,
                        )
                    }));
                } else {
                    warn!("Unable to reach primary peer {worker_name} on the network");
                }
            } else {
                // If first request timed out or was missing batches, try broadcasting to some others.
                let names: Vec<_> = self
                    .worker_cache
                    .others_workers_by_id(
                        self.committee
                            .authority(&self.authority_id)
                            .unwrap()
                            .protocol_key(),
                        &self.id,
                    )
                    .into_iter()
                    .map(|(_, info)| info.name)
                    .collect();
                handles.extend(
                    names
                        .choose_multiple(&mut rand::thread_rng(), self.request_batch_retry_nodes)
                        .filter_map(|name| network.peer(anemo::PeerId(name.0.to_bytes())))
                        .flat_map(|peer| {
                            batch_requests.iter().cloned().map(move |request| {
                                let peer = peer.clone();
                                request_batch_fn(
                                    WorkerToWorkerClient::new(peer),
                                    request,
                                    self.request_batch_timeout,
                                )
                            })
                        }),
                );
                debug!(
                    "Sending BatchRequest retries to workers {names:?}: {:?}",
                    batch_requests
                );
            }

            // Fire off batch request(s) and process results. Stop as soon as we have all the
            // missing batches.
            while let Some(result) = handles.next().await {
                match result {
                    Ok(response) => {
                        if let Some(batch) = response.into_body().batch {
                            if !message.is_certified {
                                // This batch is not part of a certificate, so we need to validate it.
                                if let Err(err) = self.validator.validate_batch(&batch).await {
                                    // The batch is invalid, we don't want to process it.
                                    return Err(anemo::rpc::Status::new_with_message(
                                        StatusCode::BadRequest,
                                        format!("Invalid batch: {err}"),
                                    ));
                                }
                            }
                            let digest = batch.digest();
                            if missing.remove(&digest) {
                                let mut updated_batch = batch.clone();
                                updated_batch.metadata_mut().created_at = now();
                                self.store.insert(&digest, &updated_batch).map_err(|e| {
                                    anemo::rpc::Status::internal(format!(
                                        "failed to write to batch store: {e:?}"
                                    ))
                                })?;
                            }
                        }
                        if missing.is_empty() {
                            return Ok(anemo::Response::new(()));
                        }
                    }
                    Err(e) => {
                        debug!(
                            "RequestBatchRequest to worker {:?} failed: {e:?}",
                            e.peer_id()
                        )
                    }
                }
            }

            first_attempt = false;
            // Add a delay before retrying.
            sleep(Duration::from_secs(1)).await;
        }
    }

    async fn fetch_batches(
        &self,
        request: anemo::Request<FetchBatchesRequest>,
    ) -> Result<anemo::Response<FetchBatchesResponse>, anemo::rpc::Status> {
        let Some(batch_fetcher) = self.batch_fetcher.as_ref() else {
            return Err(anemo::rpc::Status::new_with_message(
                StatusCode::BadRequest,
                "fetch_batches() is unsupported via RPC interface, please call via local worker handler instead",
            ));
        };
        let request = request.into_body();
        let batches = batch_fetcher
            .fetch(request.digests, request.known_workers)
            .await;
        Ok(anemo::Response::new(FetchBatchesResponse { batches }))
    }

    async fn delete_batches(
        &self,
        request: anemo::Request<WorkerDeleteBatchesMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        for digest in request.into_body().digests {
            self.store.remove(&digest).map_err(|e| {
                anemo::rpc::Status::internal(format!("failed to remove from batch store: {e:?}"))
            })?;
        }
        Ok(anemo::Response::new(()))
    }
}
