// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::CancelOnDropHandler;
use anyhow::Result;
use async_trait::async_trait;
use crypto::NetworkPublicKey;
use rand::prelude::{SliceRandom, SmallRng};
use tokio::task::JoinHandle;
use types::{
    Batch, BatchDigest, FetchCertificatesRequest, FetchCertificatesResponse,
    GetCertificatesRequest, GetCertificatesResponse,
};

pub trait UnreliableNetwork<Request: Clone + Send + Sync> {
    type Response: Clone + Send + Sync;

    fn unreliable_send(
        &mut self,
        peer: NetworkPublicKey,
        message: &Request,
    ) -> Result<JoinHandle<Result<anemo::Response<Self::Response>>>>;

    /// Broadcasts a message to all `peers` passed as an argument.
    /// The attempts to send individual messages are best effort and will not be retried.
    fn unreliable_broadcast(
        &mut self,
        peers: Vec<NetworkPublicKey>,
        message: &Request,
    ) -> Vec<Result<JoinHandle<Result<anemo::Response<Self::Response>>>>> {
        let mut handlers = Vec::new();
        for peer in peers {
            let handle = { self.unreliable_send(peer, message) };
            handlers.push(handle);
        }
        handlers
    }
}

pub trait Lucky {
    fn rng(&mut self) -> &mut SmallRng;
}

pub trait LuckyNetwork<Request> {
    type Response: Clone + Send + Sync;
    /// Pick a few addresses at random (specified by `nodes`) and try (best-effort) to send the
    /// message only to them. This is useful to pick nodes with whom to sync.
    fn lucky_broadcast(
        &mut self,
        peers: Vec<NetworkPublicKey>,
        message: &Request,
        num_nodes: usize,
    ) -> Vec<Result<JoinHandle<Result<anemo::Response<Self::Response>>>>>;
}

impl<T, M> LuckyNetwork<M> for T
where
    M: Clone + Send + Sync,
    T: UnreliableNetwork<M> + Send,
    T: Lucky,
{
    type Response = T::Response;
    fn lucky_broadcast(
        &mut self,
        mut peers: Vec<NetworkPublicKey>,
        message: &M,
        nodes: usize,
    ) -> Vec<Result<JoinHandle<Result<anemo::Response<Self::Response>>>>> {
        peers.shuffle(self.rng());
        peers.truncate(nodes);
        self.unreliable_broadcast(peers, message)
    }
}

#[async_trait]
pub trait ReliableNetwork<Request: Clone + Send + Sync> {
    type Response: Clone + Send + Sync;

    async fn send(
        &mut self,
        peer: NetworkPublicKey,
        message: &Request,
    ) -> CancelOnDropHandler<Result<anemo::Response<Self::Response>>>;

    async fn broadcast(
        &mut self,
        peers: Vec<NetworkPublicKey>,
        message: &Request,
    ) -> Vec<CancelOnDropHandler<Result<anemo::Response<Self::Response>>>> {
        let mut handlers = Vec::new();
        for peer in peers {
            let handle = self.send(peer, message).await;
            handlers.push(handle);
        }
        handlers
    }
}

#[async_trait]
pub trait PrimaryToPrimaryRpc {
    async fn get_certificates(
        &self,
        peer: &NetworkPublicKey,
        request: impl anemo::types::request::IntoRequest<GetCertificatesRequest> + Send,
    ) -> Result<GetCertificatesResponse>;
    async fn fetch_certificates(
        &self,
        peer: &NetworkPublicKey,
        request: FetchCertificatesRequest,
    ) -> Result<FetchCertificatesResponse>;
}

#[async_trait]
pub trait PrimaryToWorkerRpc {
    async fn delete_batches(&self, peer: NetworkPublicKey, digests: Vec<BatchDigest>)
        -> Result<()>;
}

#[async_trait]
pub trait WorkerRpc {
    async fn request_batch(
        &self,
        peer: NetworkPublicKey,
        batch: BatchDigest,
    ) -> Result<Option<Batch>>;
}
