// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anemo::types::response::StatusCode;
use anyhow::Result;
use async_trait::async_trait;
use config::{Committee, SharedCommittee, SharedWorkerCache, WorkerCache, WorkerId, WorkerIndex};
use crypto::PublicKey;
use fastcrypto::hash::Hash;
use futures::{stream::FuturesUnordered, StreamExt};

use rand::seq::SliceRandom;
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
    time::Duration,
};
use store::Store;
use tap::TapOptional;
use tokio::{sync::watch, time::sleep};
use tracing::{debug, error, info, trace, warn};
use types::{
    metered_channel::Sender, Batch, BatchDigest, PrimaryToWorker, ReconfigureNotification,
    RequestBatchRequest, RequestBatchResponse, WorkerBatchMessage, WorkerDeleteBatchesMessage,
    WorkerOthersBatchMessage, WorkerReconfigureMessage, WorkerSynchronizeMessage, WorkerToWorker,
    WorkerToWorkerClient,
};

use sui_metrics::monitored_future;

use crate::TransactionValidator;

#[cfg(test)]
#[path = "tests/handlers_tests.rs"]
pub mod handlers_tests;

/// Defines how the network receiver handles incoming workers messages.
#[derive(Clone)]
pub struct WorkerReceiverHandler<V> {
    pub id: WorkerId,
    pub tx_others_batch: Sender<WorkerOthersBatchMessage>,
    pub store: Store<BatchDigest, Batch>,
    pub validator: V,
}

#[async_trait]
impl<V: TransactionValidator> WorkerToWorker for WorkerReceiverHandler<V> {
    async fn report_batch(
        &self,
        request: anemo::Request<WorkerBatchMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let message = request.into_body();
        if let Err(err) = self.validator.validate_batch(&message.batch) {
            // The batch is invalid, we don't want to process it.
            return Err(anemo::rpc::Status::new_with_message(
                StatusCode::BadRequest,
                format!("Invalid batch: {err}"),
            ));
        }
        let digest = message.batch.digest();
        self.store.async_write(digest, message.batch).await;
        self.tx_others_batch
            .send(WorkerOthersBatchMessage {
                digest,
                worker_id: self.id,
            })
            .await
            .map(|_| anemo::Response::new(()))
            .map_err(|e| anemo::rpc::Status::internal(e.to_string()))
    }

    async fn request_batch(
        &self,
        request: anemo::Request<RequestBatchRequest>,
    ) -> Result<anemo::Response<RequestBatchResponse>, anemo::rpc::Status> {
        // TODO [issue #7]: Do some accounting to prevent bad actors from monopolizing our resources
        let batch = request.into_body().batch;
        let batch = self
            .store
            .read(batch)
            .await
            .map_err(|e| anemo::rpc::Status::from_error(Box::new(e)))?;

        Ok(anemo::Response::new(RequestBatchResponse { batch }))
    }
}

/// Defines how the network receiver handles incoming primary messages.
pub struct PrimaryReceiverHandler<V> {
    // The public key of this authority.
    pub name: PublicKey,
    // The id of this worker.
    pub id: WorkerId,
    // The committee information.
    pub committee: SharedCommittee,
    // The worker information cache.
    pub worker_cache: SharedWorkerCache,
    // The batch store
    pub store: Store<BatchDigest, Batch>,
    // Timeout on RequestBatch RPC.
    pub request_batch_timeout: Duration,
    // Number of random nodes to query when retrying batch requests.
    pub request_batch_retry_nodes: usize,
    /// Send reconfiguration update to other tasks.
    pub tx_reconfigure: watch::Sender<ReconfigureNotification>,
    // Validate incoming batches
    pub validator: V,
}

#[async_trait]
impl<V: TransactionValidator> PrimaryToWorker for PrimaryReceiverHandler<V> {
    async fn reconfigure(
        &self,
        request: anemo::Request<WorkerReconfigureMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let message = request.into_body().message;
        match &message {
            ReconfigureNotification::NewEpoch(new_committee) => {
                self.committee.swap(Arc::new(new_committee.clone()));
                self.update_worker_cache(new_committee);
                tracing::debug!("Committee updated to {}", self.committee);
            }
            ReconfigureNotification::UpdateCommittee(new_committee) => {
                self.committee.swap(Arc::new(new_committee.clone()));
                self.update_worker_cache(new_committee);
                tracing::debug!("Committee updated to {}", self.committee);
            }
            ReconfigureNotification::Shutdown => (), // no-op
        };

        // Notify all other tasks.
        self.tx_reconfigure
            .send(message)
            .map_err(|e| anemo::rpc::Status::internal(e.to_string()))?;

        Ok(anemo::Response::new(()))
    }

    async fn synchronize(
        &self,
        request: anemo::Request<WorkerSynchronizeMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let message = request.body();

        let mut missing = HashSet::new();
        for digest in message.digests.iter() {
            // Check if we already have the batch.
            match self.store.read(*digest).await {
                Ok(None) => {
                    missing.insert(*digest);
                    debug!("Requesting sync for batch {digest}");
                }
                Ok(Some(_)) => {
                    trace!("Digest {digest} already in store, nothing to sync");
                }
                Err(e) => {
                    error!("Failed to read from batch store: {e}");
                    return Err(anemo::rpc::Status::from_error(Box::new(e)));
                }
            };
        }

        // Keep attempting to retrieve missing batches until we get them all or the client
        // abandons the RPC.
        let mut first_attempt = true;
        loop {
            if missing.is_empty() {
                return Ok(anemo::Response::new(()));
            }

            let batch_requests: Vec<_> = missing
                .iter()
                .cloned()
                .map(|batch| RequestBatchRequest { batch })
                .collect();
            let network = request
                .extensions()
                .get::<anemo::NetworkRef>()
                .and_then(anemo::NetworkRef::upgrade)
                .ok_or_else(|| {
                    anemo::rpc::Status::internal("Unable to access network to send child RPCs")
                })?;

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
                let worker_name = match self.worker_cache.load().worker(&message.target, &self.id) {
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
                    .load()
                    .others_workers_by_id(&self.name, &self.id)
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
                            if let Err(err) = self.validator.validate_batch(&batch) {
                                // The batch is invalid, we don't want to process it.
                                return Err(anemo::rpc::Status::new_with_message(
                                    StatusCode::BadRequest,
                                    format!("Invalid batch: {err}"),
                                ));
                            }
                            let digest = batch.digest();
                            if missing.remove(&digest) {
                                self.store.sync_write(digest, batch).await.map_err(|e| {
                                    anemo::rpc::Status::internal(format!(
                                        "failed to write to batch store: {e:?}"
                                    ))
                                })?;
                            }
                        }
                        if missing.is_empty() {
                            break;
                        }
                    }
                    Err(e) => {
                        info!(
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

    async fn delete_batches(
        &self,
        request: anemo::Request<WorkerDeleteBatchesMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let digests = request.into_body().digests;
        self.store
            .remove_all(digests)
            .await
            .map_err(|e| anemo::rpc::Status::from_error(Box::new(e)))?;

        Ok(anemo::Response::new(()))
    }
}

impl<V: TransactionValidator> PrimaryReceiverHandler<V> {
    fn update_worker_cache(&self, new_committee: &Committee) {
        self.worker_cache.swap(Arc::new(WorkerCache {
            epoch: new_committee.epoch,
            workers: new_committee
                .keys()
                .iter()
                .map(|key| {
                    (
                        (*key).clone(),
                        self.worker_cache
                            .load()
                            .workers
                            .get(key)
                            .tap_none(|| {
                                warn!(
                                    "Worker cache does not have a key for the new committee member"
                                )
                            })
                            .unwrap_or(&WorkerIndex(BTreeMap::new()))
                            .clone(),
                    )
                })
                .collect(),
        }));
    }
}
