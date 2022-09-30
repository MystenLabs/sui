// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use async_trait::async_trait;
use store::Store;
use types::{
    error::DagError, metered_channel::Sender, Batch, BatchDigest, PrimaryToWorker,
    PrimaryWorkerMessage, WorkerMessage, WorkerToWorker,
};

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
        request: anemo::Request<types::WorkerMessage>,
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
        request: anemo::Request<types::WorkerBatchRequest>,
    ) -> Result<anemo::Response<types::WorkerBatchResponse>, anemo::rpc::Status> {
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
        Ok(anemo::Response::new(types::WorkerBatchResponse { batches }))
    }
}

/// Defines how the network receiver handles incoming primary messages.
#[derive(Clone)]
pub struct PrimaryReceiverHandler {
    pub tx_synchronizer: Sender<PrimaryWorkerMessage>,
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
}
