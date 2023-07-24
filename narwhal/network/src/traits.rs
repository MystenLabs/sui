// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::CancelOnDropHandler;
use anyhow::Result;
use async_trait::async_trait;
use crypto::NetworkPublicKey;
use types::{
    error::LocalClientError, FetchBatchesRequest, FetchBatchesResponse, FetchCertificatesRequest,
    FetchCertificatesResponse, RequestBatchesRequest, RequestBatchesResponse,
    WorkerOthersBatchMessage, WorkerOurBatchMessage, WorkerOwnBatchMessage,
    WorkerSynchronizeMessage,
};

pub trait ReliableNetwork<Request: Clone + Send + Sync> {
    type Response: Clone + Send + Sync;

    fn send(
        &self,
        peer: NetworkPublicKey,
        message: &Request,
    ) -> CancelOnDropHandler<Result<anemo::Response<Self::Response>>>;

    fn broadcast(
        &self,
        peers: Vec<NetworkPublicKey>,
        message: &Request,
    ) -> Vec<CancelOnDropHandler<Result<anemo::Response<Self::Response>>>> {
        let mut handlers = Vec::new();
        for peer in peers {
            let handle = self.send(peer, message);
            handlers.push(handle);
        }
        handlers
    }
}

#[async_trait]
pub trait PrimaryToPrimaryRpc {
    async fn fetch_certificates(
        &self,
        peer: &NetworkPublicKey,
        request: impl anemo::types::request::IntoRequest<FetchCertificatesRequest> + Send,
    ) -> Result<FetchCertificatesResponse>;
}

#[async_trait]
pub trait PrimaryToWorkerClient {
    async fn synchronize(
        &self,
        worker_name: NetworkPublicKey,
        request: WorkerSynchronizeMessage,
    ) -> Result<(), LocalClientError>;

    async fn fetch_batches(
        &self,
        worker_name: NetworkPublicKey,
        request: FetchBatchesRequest,
    ) -> Result<FetchBatchesResponse, LocalClientError>;
}

#[async_trait]
pub trait WorkerToPrimaryClient {
    // TODO: Remove once we have upgraded to protocol version 12.
    async fn report_our_batch(
        &self,
        request: WorkerOurBatchMessage,
    ) -> Result<(), LocalClientError>;

    async fn report_own_batch(
        &self,
        request: WorkerOwnBatchMessage,
    ) -> Result<(), LocalClientError>;

    async fn report_others_batch(
        &self,
        request: WorkerOthersBatchMessage,
    ) -> Result<(), LocalClientError>;
}

#[async_trait]
pub trait WorkerRpc {
    async fn request_batches(
        &self,
        peer: NetworkPublicKey,
        request: impl anemo::types::request::IntoRequest<RequestBatchesRequest> + Send,
    ) -> Result<RequestBatchesResponse>;
}
