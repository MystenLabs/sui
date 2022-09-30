// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::Result;
use async_trait::async_trait;
use config::{SharedWorkerCache, WorkerId};
use crypto::{NetworkPublicKey, PublicKey};
use fastcrypto::Hash;
use futures::{stream::FuturesUnordered, StreamExt};
use network::{LuckyNetwork, P2pNetwork, UnreliableNetwork};
use std::{collections::HashSet, time::Duration};
use store::Store;
use tap::TapFallible;
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
    time::{self, Timeout},
};
use tracing::{debug, error, info, trace};
use types::{
    error::DagError,
    metered_channel::{Receiver, Sender},
    Batch, BatchDigest, PrimaryToWorker, PrimaryWorkerMessage, ReconfigureNotification,
    WorkerBatchRequest, WorkerBatchResponse, WorkerMessage, WorkerPrimaryMessage,
    WorkerSynchronizeMessage, WorkerToWorker,
};

#[cfg(test)]
#[path = "tests/handlers_tests.rs"]
pub mod handlers_tests;

// Makes it possible via channels for anemo RPC handler functions to send child RPCs.
// TODO: replace this once anemo adds support for access to the network within handlers.
pub struct ChildRpcSender {
    // The public key of this authority.
    name: PublicKey,
    // The id of this worker.
    id: WorkerId,
    // The worker information cache.
    worker_cache: SharedWorkerCache,
    // Timeout on RequestBatches RPC.
    request_batches_timeout: Duration,
    // Incoming RequestBatches requests to be sent out. Uses `unreliable_send` if a target is
    // provided, or `lucky_broadcast` if a target node count is provided.
    rx_request_batches_rpc: Receiver<(
        Option<NetworkPublicKey>,
        Option<usize>,
        WorkerBatchRequest,
        oneshot::Sender<Result<anemo::Response<WorkerBatchResponse>>>,
    )>,
    /// Receive reconfiguration updates.
    rx_reconfigure: watch::Receiver<ReconfigureNotification>,
    // Network to use for sending requests.
    network: P2pNetwork,
}

impl ChildRpcSender {
    #[must_use]
    pub fn spawn(
        name: PublicKey,
        id: WorkerId,
        worker_cache: SharedWorkerCache,
        request_batches_timeout: Duration,
        rx_request_batches_rpc: Receiver<(
            Option<NetworkPublicKey>,
            Option<usize>,
            WorkerBatchRequest,
            oneshot::Sender<Result<anemo::Response<WorkerBatchResponse>>>,
        )>,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        network: P2pNetwork,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                id,
                worker_cache,
                request_batches_timeout,
                rx_request_batches_rpc,
                rx_reconfigure,
                network,
            }
            .run()
            .await;
        })
    }

    async fn handle_worker_batch_responses(
        responses: Vec<Timeout<JoinHandle<Result<anemo::Response<WorkerBatchResponse>>>>>,
        response_channel: oneshot::Sender<Result<anemo::Response<WorkerBatchResponse>>>,
    ) {
        let mut results: FuturesUnordered<_> = responses.into_iter().collect();
        let mut last_result: Result<anemo::Response<WorkerBatchResponse>> = Err(anyhow::anyhow!(
            "no WorkerBatchResponse received (or request(s) timed out)"
        ));
        while let Some(result) = results.next().await {
            match result {
                Ok(Ok(Ok(result))) => {
                    let _ = response_channel.send(Ok(result));
                    return;
                }
                Ok(Ok(Err(e))) => last_result = Err(e),
                _ => (),
            }
        }
        let _ = response_channel.send(last_result);
    }

    async fn run(&mut self) {
        let mut inflight_requests = FuturesUnordered::new();
        loop {
            tokio::select! {
                Some((target, num_nodes, request, response_channel)) = self.rx_request_batches_rpc.recv() => {
                    if target.is_some() && num_nodes.is_some() {
                        panic!("cannot set both target and num_nodes");
                    }
                    if let Some(target) = target {
                        match self.network.unreliable_send(target, &request) {
                            Ok(handle) => {
                                inflight_requests.push(Self::handle_worker_batch_responses(
                                    vec![time::timeout(self.request_batches_timeout, handle)], response_channel))
                            },
                            Err(e) => {
                                let _ = response_channel.send(Err(e));
                            },
                        }
                    } else if let Some(num_nodes) = num_nodes {
                        let names = self.worker_cache.load()
                            .others_workers(&self.name, &self.id)
                            .into_iter()
                            .map(|(_, info)| info.name)
                            .collect();
                        let handles: Vec<_> = self.network.lucky_broadcast(names, &request, num_nodes)
                            .into_iter()
                            .flatten()
                            .map(|handle| time::timeout(self.request_batches_timeout, handle))
                            .collect();
                        if handles.is_empty() {
                            let _ = response_channel.send(
                                Err(anyhow::anyhow!("could not lucky_broadcast WorkerBatchRequest to any node")));
                        } else {
                            inflight_requests.push(Self::handle_worker_batch_responses(handles, response_channel));
                        };
                    } else {
                        panic!("one of target or num_nodes must be set");
                    }
                },
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    if let ReconfigureNotification::Shutdown = self.rx_reconfigure.borrow().clone() {
                        return
                    }
                },
                Some(_) = inflight_requests.next() => {},
                else => { break }
            }
        }
    }
}

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
#[derive(Clone)]
pub struct PrimaryReceiverHandler {
    pub id: WorkerId,
    // The worker information cache.
    pub worker_cache: SharedWorkerCache,
    pub store: Store<BatchDigest, Batch>,
    // Number of random nodes to query when retrying batch requests.
    pub request_batches_retry_nodes: usize,
    pub tx_synchronizer: Sender<PrimaryWorkerMessage>,
    // Output channel to send child worker batch requests.
    pub tx_request_batches_rpc: Sender<(
        Option<NetworkPublicKey>,
        Option<usize>,
        WorkerBatchRequest,
        oneshot::Sender<Result<anemo::Response<WorkerBatchResponse>>>,
    )>,
    // Output channel to send messages to primary.
    pub tx_primary: Sender<WorkerPrimaryMessage>,
    // Output channel to process received batches.
    pub tx_batch_processor: Sender<Batch>,
}

#[async_trait]
impl PrimaryToWorker for PrimaryReceiverHandler {
    async fn send_message(
        &self,
        request: anemo::Request<PrimaryWorkerMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let message = request.into_body();

        self.tx_synchronizer
            .send(message)
            .await
            .map_err(|_| DagError::ShuttingDown)
            .map_err(|e| anemo::rpc::Status::internal(e.to_string()))?;

        Ok(anemo::Response::new(()))
    }
    async fn synchronize(
        &self,
        request: anemo::Request<WorkerSynchronizeMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let message = request.into_body();

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

        debug!(
            "Sending WorkerBatchRequest message to {} for missing batches {:?}",
            worker_name,
            missing.clone()
        );

        // TODO: restore a metric tracking number of batches with inflight requests?
        let (tx_batch_response, rx_batch_response) = oneshot::channel();
        let message = WorkerBatchRequest {
            digests: missing.iter().cloned().collect(),
        };
        self.tx_request_batches_rpc
            .send((Some(worker_name.clone()), None, message, tx_batch_response))
            .await
            .map_err(|e| anemo::rpc::Status::internal(e.to_string()))?;
        let response = rx_batch_response
            .await
            .map_err(|e| anemo::rpc::Status::internal(e.to_string()))?;
        if let Ok(response) = response {
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
        } else {
            let e = response.err().unwrap();
            info!("WorkerBatchRequest to first target {worker_name} failed: {e}");
        }

        if missing.is_empty() {
            // If nothing remains to fetch, we're done.
            return Ok(anemo::Response::new(()));
        }

        // If first request timed out or was missing batches, try broadcasting to some others.
        // TODO: refactor this to retry forever unless RPC is canceled. This will require more
        // invasive changes to primary code and cancellation propagation support in anemo.
        let (tx_batch_response, rx_batch_response) = oneshot::channel();
        let message = WorkerBatchRequest {
            digests: missing.iter().cloned().collect(),
        };
        self.tx_request_batches_rpc
            .send((
                None,
                Some(self.request_batches_retry_nodes),
                message,
                tx_batch_response,
            ))
            .await
            .map_err(|e| anemo::rpc::Status::internal(e.to_string()))?;
        let response = rx_batch_response
            .await
            .map_err(|e| anemo::rpc::Status::internal(e.to_string()))?;
        if let Ok(response) = response {
            for batch in response.into_body().batches {
                if missing.remove(&batch.digest()) {
                    // Only send batch to processor if we haven't received it already
                    // from another source.
                    if self.tx_batch_processor.send(batch).await.is_err() {
                        // Assume error sending to processor means we're shutting down.
                        return Err(anemo::rpc::Status::internal("shutting down"));
                    }
                }
            }
        }

        if missing.is_empty() {
            // If nothing remains to fetch, we're done.
            return Ok(anemo::Response::new(()));
        }
        Err(anemo::rpc::Status::unknown(format!(
            "Unable to retrieve batches after retry: {missing:?}"
        )))
    }
}
