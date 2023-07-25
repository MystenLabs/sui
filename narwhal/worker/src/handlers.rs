// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::{types::response::StatusCode, Network};
use anyhow::Result;
use async_trait::async_trait;
use config::{AuthorityIdentifier, Committee, WorkerCache, WorkerId};
use fastcrypto::hash::Hash;
use itertools::Itertools;
use network::{client::NetworkClient, WorkerToPrimaryClient};
use std::{collections::HashSet, time::Duration};
use store::{rocks::DBMap, Map};
use sui_protocol_config::ProtocolConfig;
use tracing::{debug, trace};
use types::{
    now, validate_batch_version, Batch, BatchAPI, BatchDigest, FetchBatchesRequest,
    FetchBatchesResponse, MetadataAPI, PrimaryToWorker, RequestBatchesRequest,
    RequestBatchesResponse, WorkerBatchMessage, WorkerOthersBatchMessage, WorkerSynchronizeMessage,
    WorkerToWorker, WorkerToWorkerClient,
};

use crate::{batch_fetcher::BatchFetcher, TransactionValidator};

#[cfg(test)]
#[path = "tests/handlers_tests.rs"]
pub mod handlers_tests;

/// Defines how the network receiver handles incoming workers messages.
#[derive(Clone)]
pub struct WorkerReceiverHandler<V> {
    pub protocol_config: ProtocolConfig,
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
        if let Err(err) = self
            .validator
            .validate_batch(&message.batch, &self.protocol_config)
            .await
        {
            return Err(anemo::rpc::Status::new_with_message(
                StatusCode::BadRequest,
                format!("Invalid batch: {err}"),
            ));
        }
        let digest = message.batch.digest();

        let mut batch = message.batch.clone();

        // TODO: Remove once we have upgraded to protocol version 12.
        if self.protocol_config.narwhal_versioned_metadata() {
            // Set received_at timestamp for remote batch.
            batch.versioned_metadata_mut().set_received_at(now());
        }
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
    pub protocol_config: ProtocolConfig,
    // The worker information cache.
    pub worker_cache: WorkerCache,
    // The batch store
    pub store: DBMap<BatchDigest, Batch>,
    // Timeout on RequestBatches RPC.
    pub request_batches_timeout: Duration,
    // Number of random nodes to query when retrying batch requests.
    pub request_batches_retry_nodes: usize,
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
        if missing.is_empty() {
            return Ok(anemo::Response::new(()));
        }

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
                    "The primary asked worker to sync with an unknown node: {e}"
                )));
            }
        };
        let Some(peer) = network.peer(anemo::PeerId(worker_name.0.to_bytes())) else {
            return Err(anemo::rpc::Status::internal(format!(
                "Not connected with worker peer {worker_name}"
            )));
        };
        let mut client = WorkerToWorkerClient::new(peer.clone());

        // Attempt to retrieve missing batches.
        // Retried at a higher level in Synchronizer::sync_batches_internal().
        let request = RequestBatchesRequest {
            batch_digests: missing.iter().cloned().collect(),
        };
        debug!("Sending RequestBatchesRequest to {worker_name}: {request:?}");

        let mut response = client
            .request_batches(
                anemo::Request::new(request).with_timeout(self.request_batches_timeout),
            )
            .await?
            .into_inner();
        for batch in response.batches.iter_mut() {
            if !message.is_certified {
                // This batch is not part of a certificate, so we need to validate it.
                if let Err(err) = self
                    .validator
                    .validate_batch(batch, &self.protocol_config)
                    .await
                {
                    return Err(anemo::rpc::Status::new_with_message(
                        StatusCode::BadRequest,
                        format!("Invalid batch: {err}"),
                    ));
                }
            }

            // TODO: Remove once we have upgraded to protocol version 12.
            validate_batch_version(batch, &self.protocol_config).map_err(|err| {
                anemo::rpc::Status::new_with_message(
                    StatusCode::BadRequest,
                    format!("Invalid batch: {err}"),
                )
            })?;

            let digest = batch.digest();
            if missing.remove(&digest) {
                // TODO: Remove once we have upgraded to protocol version 12.
                if self.protocol_config.narwhal_versioned_metadata() {
                    // Set received_at timestamp for remote batch.
                    batch.versioned_metadata_mut().set_received_at(now());
                }
                self.store.insert(&digest, batch).map_err(|e| {
                    anemo::rpc::Status::internal(format!("failed to write to batch store: {e:?}"))
                })?;
            }
        }

        if missing.is_empty() {
            return Ok(anemo::Response::new(()));
        }
        Err(anemo::rpc::Status::internal(
            "failed to synchronize batches!",
        ))
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
}
