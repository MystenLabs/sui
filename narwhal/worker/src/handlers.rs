// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::Result;
use async_trait::async_trait;
use config::{Committee, SharedCommittee, SharedWorkerCache, WorkerCache, WorkerId, WorkerIndex};
use crypto::PublicKey;
use fastcrypto::Hash;
use futures::{stream::FuturesUnordered, StreamExt};

use rand::seq::SliceRandom;
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
    time::Duration,
};
use store::Store;
use tap::{TapFallible, TapOptional};
use tokio::sync::watch;
use tracing::{debug, error, info, trace, warn};
use types::{
    error::DagError, metered_channel::Sender, Batch, BatchDigest, PrimaryToWorker,
    ReconfigureNotification, RequestBatchRequest, RequestBatchResponse, WorkerBatchRequest,
    WorkerBatchResponse, WorkerDeleteBatchesMessage, WorkerMessage, WorkerPrimaryMessage,
    WorkerReconfigureMessage, WorkerSynchronizeMessage, WorkerToWorker, WorkerToWorkerClient,
};

#[cfg(test)]
#[path = "tests/handlers_tests.rs"]
pub mod handlers_tests;

/// Defines how the network receiver handles incoming workers messages.
#[derive(Clone)]
pub struct WorkerReceiverHandler {
    pub tx_processor: Sender<Batch>,
    pub store: Store<BatchDigest, Batch>,
}

#[async_trait]
impl WorkerToWorker for WorkerReceiverHandler {
    async fn send_message(
        &self,
        request: anemo::Request<WorkerMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let message = request.into_body();
        match message {
            WorkerMessage::Batch(batch) => self
                .tx_processor
                .send(batch)
                .await
                .map_err(|_| DagError::ShuttingDown),
        }
        .map(|_| anemo::Response::new(()))
        .map_err(|e| anemo::rpc::Status::internal(e.to_string()))
    }
    async fn request_batches(
        &self,
        request: anemo::Request<WorkerBatchRequest>,
    ) -> Result<anemo::Response<WorkerBatchResponse>, anemo::rpc::Status> {
        let message = request.into_body();
        // TODO [issue #7]: Do some accounting to prevent bad actors from monopolizing our resources
        // TODO: Add a limit on number of requested batches
        let batches: Vec<Batch> = self
            .store
            .read_all(message.digests)
            .await
            .map_err(|e| anemo::rpc::Status::from_error(Box::new(e)))?
            .into_iter()
            .flatten()
            .collect();
        Ok(anemo::Response::new(WorkerBatchResponse { batches }))
    }
}

/// Defines how the network receiver handles incoming primary messages.
pub struct PrimaryReceiverHandler {
    // The public key of this authority.
    pub name: PublicKey,
    // The id of this worker.
    pub id: WorkerId,
    // The committee information.
    pub committee: SharedCommittee,
    // The worker information cache.
    pub worker_cache: SharedWorkerCache,
    pub store: Store<BatchDigest, Batch>,
    // Timeout on RequestBatches RPC.
    pub request_batches_timeout: Duration,
    // Number of random nodes to query when retrying batch requests.
    pub request_batches_retry_nodes: usize,
    /// Send reconfiguration update to other tasks.
    pub tx_reconfigure: watch::Sender<ReconfigureNotification>,
    // Output channel to send messages to primary.
    pub tx_primary: Sender<WorkerPrimaryMessage>,
    // Output channel to process received batches.
    pub tx_batch_processor: Sender<Batch>,
}

#[async_trait]
impl PrimaryToWorker for PrimaryReceiverHandler {
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
        let mut available = HashSet::new();

        for digest in message.digests.iter() {
            // Check if we already have the batch.
            match self.store.read(*digest).await {
                Ok(None) => {
                    missing.insert(*digest);
                    debug!("Requesting sync for batch {digest}");
                }
                Ok(Some(_)) => {
                    available.insert(*digest);
                    trace!("Digest {digest} already in store, nothing to sync");
                    continue;
                }
                Err(e) => {
                    error!("{e}");
                    continue;
                }
            };
        }

        // Reply back immediately for the available ones.
        // Doing this will ensure the batch id will be populated to primary even
        // when other processes fail to do so (ex we received a batch from a peer
        // worker and message has been missed by primary).
        for digest in available {
            let message = WorkerPrimaryMessage::OthersBatch(digest, self.id);
            let _ = self.tx_primary.send(message).await.tap_err(|err| {
                debug!("{err:?} {}", DagError::ShuttingDown);
            });
        }

        if missing.is_empty() {
            debug!(
                "All batches are already available {:?} nothing to request from peers",
                message.digests
            );
            return Ok(anemo::Response::new(()));
        }

        // Send sync request to a single node.
        let worker_name = match self.worker_cache.load().worker(&message.target, &self.id) {
            Ok(worker_info) => worker_info.name,
            Err(e) => {
                return Err(anemo::rpc::Status::internal(format!(
                    "The primary asked us to sync with an unknown node: {e}"
                )));
            }
        };
        let batch_request = anemo::Request::new(WorkerBatchRequest {
            digests: missing.iter().cloned().collect(),
        })
        .with_timeout(self.request_batches_timeout);
        debug!(
            "Sending WorkerBatchRequest message to {worker_name} for missing batches {:?}",
            message.digests
        );

        let network = request
            .extensions()
            .get::<anemo::NetworkRef>()
            .and_then(anemo::NetworkRef::upgrade)
            .ok_or_else(|| {
                anemo::rpc::Status::internal("Unable to access network to send child RPCs")
            })?;
        let peer_id = anemo::PeerId(worker_name.0.to_bytes());
        if let Some(peer) = network.peer(peer_id) {
            match WorkerToWorkerClient::new(peer)
                .request_batches(batch_request)
                .await
            // TODO: duplicated code in the same file.
            {
                Ok(response) => {
                    for batch in response.into_body().batches {
                        let digest = &batch.digest();
                        if missing.remove(digest) {
                            // Only send batch to processor if we haven't received it already
                            // from another source.
                            if self.tx_batch_processor.send(batch).await.is_err() {
                                // Assume error sending to processor means we're shutting down.
                                return Err(anemo::rpc::Status::internal("shutting down"));
                            }
                        }
                    }
                }
                Err(e) => {
                    info!("WorkerBatchRequest to first target {worker_name} failed: {e:?}");
                }
            }
        } else {
            info!("Unable to reach primary peer {worker_name} on the network");
        }

        if missing.is_empty() {
            // If nothing remains to fetch, we're done.
            return Ok(anemo::Response::new(()));
        }

        // If first request timed out or was missing batches, try broadcasting to some others.
        // TODO: refactor this to retry forever unless RPC is canceled. This will require more
        // invasive changes to primary code and cancellation propagation support in anemo.
        let message = WorkerBatchRequest {
            digests: missing.iter().cloned().collect(),
        };
        let names: Vec<_> = self
            .worker_cache
            .load()
            .others_workers(&self.name, &self.id)
            .into_iter()
            .map(|(_, info)| info.name)
            .collect();
        let mut clients: Vec<_> = names
            .choose_multiple(&mut rand::thread_rng(), self.request_batches_retry_nodes)
            .filter_map(|name| network.peer(anemo::PeerId(name.0.to_bytes())))
            .map(WorkerToWorkerClient::new)
            .collect();
        let mut handles: FuturesUnordered<_> = clients
            .iter_mut()
            .map(|client| {
                client.request_batches(
                    anemo::Request::new(message.clone()).with_timeout(self.request_batches_timeout),
                )
            })
            .collect();
        while let Some(result) = handles.next().await {
            match result {
                Ok(response) => {
                    for batch in response.into_body().batches {
                        let digest = &batch.digest();
                        if missing.remove(digest) {
                            // Only send batch to processor if we haven't received it already
                            // from another source.
                            if self.tx_batch_processor.send(batch).await.is_err() {
                                // Assume error sending to processor means we're shutting down.
                                return Err(anemo::rpc::Status::internal("shutting down"));
                            }
                        }
                    }
                    if missing.is_empty() {
                        break;
                    }
                }
                Err(e) => {
                    info!("WorkerBatchRequest to retry target {worker_name} failed: {e:?}");
                }
            }
            if missing.is_empty() {
                // If nothing remains to fetch, we're done.
                return Ok(anemo::Response::new(()));
            }
        }

        Err(anemo::rpc::Status::unknown(format!(
            "Unable to retrieve batches after retry: {missing:?}"
        )))
    }

    async fn request_batch(
        &self,
        request: anemo::Request<RequestBatchRequest>,
    ) -> Result<anemo::Response<RequestBatchResponse>, anemo::rpc::Status> {
        let batch = request.into_body().batch;
        let batch = self
            .store
            .read(batch)
            .await
            .map_err(|e| anemo::rpc::Status::from_error(Box::new(e)))?;

        Ok(anemo::Response::new(RequestBatchResponse { batch }))
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

impl PrimaryReceiverHandler {
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
