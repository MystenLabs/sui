// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::traits::{PrimaryToPrimaryRpc, WorkerRpc};
use crate::{traits::ReliableNetwork, CancelOnDropHandler, RetryConfig};
use anemo::PeerId;
use anyhow::format_err;
use anyhow::Result;
use async_trait::async_trait;
use crypto::NetworkPublicKey;
use types::{
    FetchCertificatesRequest, FetchCertificatesResponse, PrimaryToPrimaryClient,
    RequestBatchesRequest, RequestBatchesResponse, WorkerBatchMessage, WorkerToWorkerClient,
};

fn send<F, R, Fut>(
    network: anemo::Network,
    peer: NetworkPublicKey,
    f: F,
) -> CancelOnDropHandler<Result<anemo::Response<R>>>
where
    F: Fn(anemo::Peer) -> Fut + Send + Sync + 'static + Clone,
    R: Send + Sync + 'static + Clone,
    Fut: std::future::Future<Output = Result<anemo::Response<R>, anemo::rpc::Status>> + Send,
{
    // Safety
    // Since this spawns an unbounded task, this should be called in a time-restricted fashion.

    let peer_id = PeerId(peer.0.to_bytes());
    let message_send = move || {
        let network = network.clone();
        let f = f.clone();

        async move {
            if let Some(peer) = network.peer(peer_id) {
                f(peer).await.map_err(|e| {
                    // this returns a backoff::Error::Transient
                    // so that if anemo::Status is returned, we retry
                    backoff::Error::transient(anyhow::anyhow!("RPC error: {e:?}"))
                })
            } else {
                Err(backoff::Error::transient(anyhow::anyhow!(
                    "not connected to peer {peer_id}"
                )))
            }
        }
    };

    let retry_config = RetryConfig {
        retrying_max_elapsed_time: None, // retry forever
        ..Default::default()
    };
    let task = tokio::spawn(retry_config.retry(message_send));

    CancelOnDropHandler(task)
}

//
// Primary-to-Primary
//

#[async_trait]
impl PrimaryToPrimaryRpc for anemo::Network {
    async fn fetch_certificates(
        &self,
        peer: &NetworkPublicKey,
        request: impl anemo::types::request::IntoRequest<FetchCertificatesRequest> + Send,
    ) -> Result<FetchCertificatesResponse> {
        let peer_id = PeerId(peer.0.to_bytes());
        let peer = self
            .peer(peer_id)
            .ok_or_else(|| format_err!("Network has no connection with peer {peer_id}"))?;
        let response = PrimaryToPrimaryClient::new(peer)
            .fetch_certificates(request)
            .await
            .map_err(|e| format_err!("Network error {:?}", e))?;
        Ok(response.into_body())
    }
}

impl ReliableNetwork<WorkerBatchMessage> for anemo::Network {
    type Response = ();
    fn send(
        &self,
        peer: NetworkPublicKey,
        message: &WorkerBatchMessage,
    ) -> CancelOnDropHandler<Result<anemo::Response<()>>> {
        let message = message.to_owned();
        let f = move |peer| {
            let message = message.clone();
            async move { WorkerToWorkerClient::new(peer).report_batch(message).await }
        };

        send(self.clone(), peer, f)
    }
}

#[async_trait]
impl WorkerRpc for anemo::Network {
    async fn request_batches(
        &self,
        peer: NetworkPublicKey,
        request: impl anemo::types::request::IntoRequest<RequestBatchesRequest> + Send,
    ) -> Result<RequestBatchesResponse> {
        let peer_id = PeerId(peer.0.to_bytes());
        let peer = self
            .peer(peer_id)
            .ok_or_else(|| format_err!("Network has no connection with peer {peer_id}"))?;
        let response = WorkerToWorkerClient::new(peer)
            .request_batches(request)
            .await
            .map_err(|e| format_err!("Network error {:?}", e))?;
        Ok(response.into_body())
    }
}
